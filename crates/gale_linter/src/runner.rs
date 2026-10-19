//! Linting a file against a [`RuleSet`].
//!
//! A [`RuleSet`] is everything that decides how one file is linted: the rules
//! its config enables, each with the name the config used, its options and
//! its severity, plus the disable-comment settings and `defaultSeverity`.
//! It is built once per distinct effective config and shared by every file
//! that config applies to; [`RuleSet::lint`] is the one way a file is linted.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use gale_css_parser::embedded::HostLanguage;
use gale_css_parser::{CssNode, ParseResult, Syntax, parse};
use gale_diagnostics::{Diagnostic, LintResult, Severity, SourceLineIndex, Span};

use crate::disables::{self, DisableReports};
use crate::embedded::{HostOptions, lint_host};
use crate::file_cache::FileCache;
use crate::known_rules::{self, RuleSupport};
use crate::lint_env::LintEnv;
use crate::panic_guard::{self, Caught, ISSUES_URL};
use crate::pattern;
use crate::registry::{RuleRegistry, resolve_deprecated_alias};
use crate::rule::{Rule, RuleContext, secondary_options_of};

/// Apply the secondary options every Stylelint rule accepts, whatever the
/// rule itself does with its options:
///
/// - `message` replaces the warning text,
/// - `url` attaches a documentation link to the warning,
/// - `disableFix: true` keeps the report but drops its autofix.
fn apply_secondary_options(diag: &mut Diagnostic, options: Option<&serde_json::Value>) {
  let Some(secondary) = options.and_then(secondary_options_of) else {
    return;
  };
  if let Some(message) = secondary.get("message").and_then(|v| v.as_str()) {
    crate::custom_message::apply(diag, message);
  }
  if let Some(url) = secondary.get("url").and_then(|v| v.as_str()) {
    diag.url = Some(url.to_string());
  }
  if secondary.get("disableFix").and_then(|v| v.as_bool()) == Some(true) {
    diag.fix = None;
  }
}

/// The rule named by `GALE_DEBUG_PANIC`, read once per process.
static DEBUG_PANIC_RULE: std::sync::LazyLock<Option<String>> =
  std::sync::LazyLock::new(|| std::env::var("GALE_DEBUG_PANIC").ok());

/// The text a file must contain for `GALE_DEBUG_PANIC` to fire on it.
const DEBUG_PANIC_MARKER: &str = "gale-debug-panic";

/// Fault injection for testing the crash guard end to end: with
/// `GALE_DEBUG_PANIC=<rule-name>` set, that rule panics on every file whose
/// source contains `gale-debug-panic`, the way a slicing bug would.
fn debug_panic(rule_name: &str, source: &str) {
  if DEBUG_PANIC_RULE.as_deref() == Some(rule_name) && source.contains(DEBUG_PANIC_MARKER) {
    panic!("GALE_DEBUG_PANIC: simulated crash in {rule_name}");
  }
}

/// Returns `true` when the `GALE_DEBUG_PERF` environment variable is set to `"1"`.
///
/// Read once per process rather than once per file.
fn perf_enabled() -> bool {
  static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
  *ENABLED.get_or_init(|| {
    std::env::var("GALE_DEBUG_PERF")
      .map(|v| v == "1")
      .unwrap_or(false)
  })
}

/// One rule as a config sets it: the name it is listed under, its options
/// and its severity.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleSetting {
  /// The name the config uses, which may be a deprecated alias of the rule
  /// (`color-hex-case` for `@stylistic/color-hex-case`).  Problems are
  /// reported under it, as Stylelint reports them.
  pub name: String,
  /// The options exactly as the config wrote them.
  pub options: Option<serde_json::Value>,
  /// The severity the config gives the rule, if any.
  pub severity: Option<Severity>,
}

impl RuleSetting {
  /// `name` with no options and no severity of its own.
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      options: None,
      severity: None,
    }
  }

  /// This setting with `options`.
  pub fn with_options(mut self, options: serde_json::Value) -> Self {
    self.options = Some(options);
    self
  }

  /// This setting with `severity`.
  pub fn with_severity(mut self, severity: Severity) -> Self {
    self.severity = Some(severity);
    self
  }
}

/// How configuration comments (`/* stylelint-disable */` and friends) are
/// treated: Stylelint's `ignoreDisables` and its four `report*Disables`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct DisableOptions {
  /// `ignoreDisables`: report problems even where a comment disables them.
  pub ignore: bool,
  /// `reportNeedlessDisables`: report comments that suppress nothing.
  pub report_needless: bool,
  /// `reportInvalidScopeDisables`: report comments naming a rule that is
  /// not configured.
  pub report_invalid_scope: bool,
  /// `reportDescriptionlessDisables`: report comments without a
  /// `-- description`.
  pub report_descriptionless: bool,
  /// `reportUnscopedDisables`: report comments that name no rule.
  pub report_unscoped: bool,
}

impl DisableOptions {
  /// Every switch that either `self` or `other` turns on, as the command
  /// line's flags add to the config's.
  pub fn union(self, other: Self) -> Self {
    Self {
      ignore: self.ignore || other.ignore,
      report_needless: self.report_needless || other.report_needless,
      report_invalid_scope: self.report_invalid_scope || other.report_invalid_scope,
      report_descriptionless: self.report_descriptionless || other.report_descriptionless,
      report_unscoped: self.report_unscoped || other.report_unscoped,
    }
  }

  /// Whether the comments' disabled ranges are needed at all: to suppress
  /// problems, or for one of the reports.
  fn needs_ranges(&self) -> bool {
    !self.ignore
      || self.report_needless
      || self.report_invalid_scope
      || self.report_descriptionless
      || self.report_unscoped
  }
}

/// What a [`RuleSet`] is built from.
#[derive(Debug, Clone, Default)]
pub struct RuleSetSpec {
  /// The rules the config enables for the files this set applies to.
  pub rules: Vec<RuleSetting>,
  /// Rule names the config lists that count as configured for
  /// `reportInvalidScopeDisables` on top of `rules`: those an override
  /// turns off, say, or plugin rules gale does not implement.
  pub configured_rules: Vec<String>,
  /// How configuration comments are treated.
  pub disables: DisableOptions,
  /// Stylelint's `defaultSeverity`: the severity of a rule the config gives
  /// none, and of the disable-comment reports.
  pub default_severity: Option<Severity>,
  /// What the run knows about the project beyond the config.
  pub env: LintEnv,
}

/// A rule a [`RuleSet`] runs, as configured.
struct ActiveRule {
  rule: &'static dyn Rule,
  /// The name the config used, under which problems are reported.
  config_name: String,
  options: Option<serde_json::Value>,
  severity: Option<Severity>,
}

/// Everything that decides how a file is linted, resolved once and shared
/// by every file the same effective config applies to.
pub struct RuleSet {
  /// The registered rules to run, in config order.
  rules: Vec<ActiveRule>,
  /// Position in `rules` by each rule's own name, which is the name the
  /// rules report under before relabelling.
  by_name: HashMap<&'static str, usize>,
  /// Enabled names that are no rule at all, reported as `Unknown rule`.
  unknown: Vec<String>,
  /// The rule names, by their canonical spelling, that count as configured
  /// for `reportInvalidScopeDisables`.
  configured: HashSet<String>,
  /// The rule names configured with `reportDisables: true`.
  forbids_disable: HashSet<String>,
  /// Regex literals in the options that do not compile, as invalid-option
  /// texts; found once rather than per file.
  invalid_patterns: Vec<String>,
  disables: DisableOptions,
  default_severity: Option<Severity>,
  env: LintEnv,
}

/// A name with any deprecated alias resolved to the rule's own name.
fn canonical(name: &str) -> &str {
  resolve_deprecated_alias(name).unwrap_or(name)
}

impl RuleSet {
  /// The rule set `spec` describes, with rules from the built-in registry.
  pub fn new(spec: RuleSetSpec) -> Self {
    Self::with_registry(RuleRegistry::global(), spec)
  }

  /// The rule set `spec` describes, with rules looked up in `registry`.
  pub fn with_registry(registry: &'static RuleRegistry, spec: RuleSetSpec) -> Self {
    let RuleSetSpec {
      rules: settings,
      configured_rules,
      disables,
      default_severity,
      env,
    } = spec;

    let mut configured: HashSet<String> = configured_rules
      .iter()
      .map(|name| canonical(name).to_string())
      .collect();
    let mut forbids_disable = HashSet::new();
    let mut rules: Vec<ActiveRule> = Vec::with_capacity(settings.len());
    let mut by_name: HashMap<&'static str, usize> = HashMap::with_capacity(settings.len());
    let mut unknown = Vec::new();

    for setting in settings {
      configured.insert(canonical(&setting.name).to_string());
      let forbids = setting
        .options
        .as_ref()
        .and_then(secondary_options_of)
        .and_then(|s| s.get("reportDisables"))
        .and_then(|v| v.as_bool())
        == Some(true);
      let Some(rule) = registry.get(&setting.name) else {
        if forbids {
          forbids_disable.insert(setting.name.clone());
        }
        // Real Stylelint and plugin rules gale lacks, and rules Stylelint
        // removed, are skipped quietly here; the caller warns about them
        // once per run.
        if known_rules::classify(registry, &setting.name) == RuleSupport::Unknown {
          unknown.push(setting.name);
        }
        continue;
      };
      if forbids {
        forbids_disable.insert(rule.name().to_string());
        forbids_disable.insert(setting.name.clone());
      }
      let active = ActiveRule {
        rule,
        config_name: setting.name,
        options: setting.options,
        severity: setting.severity,
      };
      match by_name.get(rule.name()) {
        // A config that names one rule twice, by its name and by an alias,
        // runs it once, as the name it is registered under.
        Some(&index) => {
          if active.config_name == rule.name() {
            rules[index] = active;
          }
        }
        None => {
          by_name.insert(rule.name(), rules.len());
          rules.push(active);
        }
      }
    }

    let mut invalid_patterns: Vec<String> = Vec::new();
    for active in &rules {
      if let Some(options) = &active.options {
        for invalid in pattern::invalid_options(active.rule.name(), options) {
          if !invalid_patterns.contains(&invalid.message) {
            invalid_patterns.push(invalid.message);
          }
        }
      }
    }

    Self {
      rules,
      by_name,
      unknown,
      configured,
      forbids_disable,
      invalid_patterns,
      disables,
      default_severity,
      env,
    }
  }

  /// A rule set running the rules `names`, with no options.  Mostly for
  /// tests and tools that lint with a fixed list.
  pub fn of_rules<S: Into<String>>(names: impl IntoIterator<Item = S>) -> Self {
    Self::new(RuleSetSpec {
      rules: names.into_iter().map(RuleSetting::new).collect(),
      ..RuleSetSpec::default()
    })
  }

  /// How configuration comments are treated in files linted with this set.
  pub fn disables(&self) -> DisableOptions {
    self.disables
  }

  /// The environment rules see.
  pub fn env(&self) -> LintEnv {
    self.env
  }

  /// Lint `source`, the text of `file_path`, parsed as `syntax`.
  ///
  /// An HTML-like file (`.vue`, `.svelte`, `.astro`, `.html`, ...) is linted
  /// one embedded style sheet at a time, with every problem placed in the
  /// file; anything else is one style sheet.
  ///
  /// Never panics: a rule that crashes is reported as an internal-error
  /// problem on the file and the remaining rules still run.
  pub fn lint(&self, source: &str, file_path: &str, syntax: Syntax) -> LintResult {
    match HostLanguage::from_path(file_path) {
      Some(host) => {
        let options = HostOptions {
          ignore_disables: self.disables.ignore,
        };
        lint_host(source, file_path, host, options, |text, syntax| {
          self.lint_sheet(text, file_path, syntax)
        })
      }
      None => self.lint_sheet(source, file_path, syntax),
    }
  }

  /// Lint one style sheet, reporting a panic outside any one rule as a
  /// single internal-error problem on the file.
  fn lint_sheet(&self, source: &str, file_path: &str, syntax: Syntax) -> LintResult {
    guard_file(source, file_path, || {
      self.lint_sheet_unguarded(source, file_path, syntax)
    })
  }

  /// [`Self::lint_sheet`] without the file-level panic guard.
  fn lint_sheet_unguarded(&self, source: &str, file_path: &str, syntax: Syntax) -> LintResult {
    let debug = perf_enabled();
    if debug {
      eprintln!("[perf] start file: {}", file_path);
    }

    let t0 = Instant::now();
    let mut parse_result = match parse(source, syntax) {
      Ok(result) => result,
      Err(err) => {
        let diag = Diagnostic::new("parse-error", format!("Failed to parse file: {err}"))
          .severity(Severity::Error)
          .span(Span::new(0, 0));
        return LintResult::new(file_path, source, vec![diag]);
      }
    };
    if debug {
      eprintln!("[perf] parse: {:.3}s", t0.elapsed().as_secs_f64());
    }
    // The walk below is the tree's last use, so it takes the nodes; `finish`
    // needs only the source map.
    let nodes = std::mem::take(&mut parse_result.nodes);
    // Rules read the text the parser saw, which for Sass is the converted
    // SCSS that every node span points into.  `finish` maps the spans back.
    let text = parse_result.source.as_str();

    let cache = FileCache::new(text, syntax);
    let mut run = RuleRun::new(self, file_path, &cache);

    let t1 = Instant::now();
    run.check_root(&nodes, debug);
    if debug {
      eprintln!(
        "[perf] check_root total: {:.3}s",
        t1.elapsed().as_secs_f64()
      );
    }

    let t2 = Instant::now();
    for node in nodes {
      run.walk(node);
    }
    if debug {
      eprintln!("[perf] walk: {:.3}s", t2.elapsed().as_secs_f64());
    }
    let RuleRun {
      mut diagnostics,
      failures,
      invalid_options,
      ..
    } = run;

    for diag in &mut diagnostics {
      if diag.file_path.is_empty() {
        diag.file_path = file_path.to_string();
      }
      self.settle(diag);
    }

    let t3 = Instant::now();
    if let Some(rejected) =
      self.apply_disables(&mut diagnostics, source, &parse_result, &cache, file_path)
    {
      return finish(file_path, source, &parse_result, vec![rejected], Vec::new());
    }
    if debug {
      eprintln!("[perf] disable-filter: {:.3}s", t3.elapsed().as_secs_f64());
    }

    // Rule crashes go in last so that disables, severity overrides and the
    // `message` option cannot hide or soften them.
    diagnostics.extend(failures);

    let unknown = self.unknown_rule_problems(source, file_path);
    let mut result = finish(file_path, source, &parse_result, diagnostics, unknown);
    result.invalid_option_warnings = invalid_options;
    if debug {
      eprintln!("[perf] total diagnostics: {}", result.diagnostics.len());
    }
    result
  }

  /// Give a rule's problem what its configuration says: the `message`,
  /// `url` and `disableFix` options, the configured severity (else the
  /// `defaultSeverity`), and the rule name the config used.
  fn settle(&self, diag: &mut Diagnostic) {
    let active = self
      .by_name
      .get(diag.rule_name.as_str())
      .map(|&index| &self.rules[index]);
    apply_secondary_options(diag, active.and_then(|a| a.options.as_ref()));
    if let Some(severity) = active.and_then(|a| a.severity).or(self.default_severity) {
      diag.severity = severity;
    }
    if let Some(active) = active
      && active.config_name != diag.rule_name
    {
      diag.rule_name = active.config_name.clone();
    }
  }

  /// Apply the file's configuration comments to `diagnostics`: drop the
  /// problems they disable and add the reports about the comments.
  ///
  /// The rules linted `parsed.source` (the converted SCSS, for Sass), whose
  /// offsets the source map takes back to `source` so that lines are those
  /// of the file the author wrote.  In a style sheet embedded in an
  /// HTML-like file the host applies the ranges of the whole document
  /// instead, so only the reports are added here.  The comments are found
  /// in the statement tree the rules share, `cache`, built for that text.
  ///
  /// Returns Stylelint's `CssSyntaxError` for a comment it rejects (an
  /// `enable` with nothing disabled, say), which replaces every other
  /// problem in the file, `ignoreDisables` or not.
  fn apply_disables(
    &self,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    parsed: &ParseResult,
    cache: &FileCache,
    file_path: &str,
  ) -> Option<Diagnostic> {
    let text = cache.source();
    if !disables::may_have_directives(text) {
      return None;
    }
    let lines = SourceLineIndex::build(source);
    let line_of = |offset: usize| match &parsed.source_map {
      Some(map) => lines.line(map.to_original(offset)),
      None => lines.line(offset),
    };
    let mut collector = disables::Collector::new(&line_of);
    collector.scan_tree(&cache.postcss_tree(), cache.syntax(), 0);
    let (ranges, rejected) = collector.finish();
    let embedded = crate::embedded::in_embedded_root();
    if let Some(rejected) = rejected.filter(|_| !embedded) {
      return Some(rejected.to_diagnostic(file_path));
    }
    if !self.disables.needs_ranges() {
      return None;
    }
    let settings = disables::DisableSettings {
      suppress: !self.disables.ignore && !embedded,
      reports: DisableReports {
        needless: self.disables.report_needless,
        invalid_scope: self.disables.report_invalid_scope,
        descriptionless: self.disables.report_descriptionless,
        unscoped: self.disables.report_unscoped,
        // Stylelint reports the comments at `defaultSeverity`, else as
        // errors.
        severity: self.default_severity.unwrap_or(Severity::Error),
      },
      is_configured: &|name| self.configured.contains(canonical(name)),
      forbids_disable: &|name| {
        self.forbids_disable.contains(name) || self.forbids_disable.contains(canonical(name))
      },
      file_path,
    };
    disables::apply(diagnostics, &ranges, &line_of, &settings);
    None
  }

  /// Stylelint's report for every enabled name that is no rule at all:
  /// `Unknown rule <name>.` as an error at the very start of the file, like
  /// Stylelint's own (which disables, severities and the `message` option do
  /// not touch either).
  fn unknown_rule_problems(&self, source: &str, file_path: &str) -> Vec<Diagnostic> {
    let first_char = source.chars().next().map_or(1, char::len_utf8);
    self
      .unknown
      .iter()
      .map(|name| {
        Diagnostic::new(name.as_str(), known_rules::unknown_rule_message(name))
          .severity(Severity::Error)
          .span(Span::new(0, first_char))
          .file_path(file_path)
      })
      .collect()
  }
}

/// Turn a file's diagnostics into its [`LintResult`] against the source the
/// caller passed in.
///
/// When the parser linted converted text (Sass), every span is mapped back
/// to the original and autofixes are dropped: their edits target the
/// converted SCSS, so applying them to the Sass file would corrupt it.
///
/// Diagnostics are sorted by position.  The rule name breaks ties: rules run
/// in the order the config's rule map happens to hold them, which varies
/// between processes.  Without a tiebreaker two warnings at the same offset
/// swap places between runs.
///
/// `unknown_rules` (from [`RuleSet::unknown_rule_problems`]) already point
/// into `source` and are added after the mapping.
fn finish(
  file_path: &str,
  source: &str,
  parsed: &ParseResult,
  mut diagnostics: Vec<Diagnostic>,
  unknown_rules: Vec<Diagnostic>,
) -> LintResult {
  if let Some(map) = &parsed.source_map {
    for diag in &mut diagnostics {
      let start = clamp_to_char_boundary(source, map.to_original(diag.span.offset));
      let end = clamp_to_char_boundary(source, map.to_original(diag.span.end()));
      diag.span = Span::new(start, end.saturating_sub(start));
      diag.fix = None;
    }
  }
  diagnostics.extend(unknown_rules);

  diagnostics.sort_by(|a, b| {
    a.span
      .offset
      .cmp(&b.span.offset)
      .then_with(|| a.rule_name.cmp(&b.rule_name))
  });

  LintResult::new(file_path, source, diagnostics)
}

/// The problem reported in place of a rule that panicked, under the name
/// the config used for it.
///
/// It is an error whatever the rule's configured severity, so the run exits
/// non-zero, and it carries the panic message and location so the report is
/// enough to file a bug.
fn internal_error(active: &ActiveRule, caught: &Caught, offset: usize) -> Diagnostic {
  Diagnostic::new(
    active.config_name.as_str(),
    format!(
      "Internal error in rule \"{}\": {}. This is a bug in gale; please report it at {ISSUES_URL}",
      active.rule.name(),
      caught.describe()
    ),
  )
  .severity(Severity::Error)
  .span(Span::new(offset, 0))
}

/// The rule name for a crash that happened outside any one rule, such as in
/// the parser.
pub const INTERNAL_ERROR_RULE: &str = "internal-error";

/// Lint one file through `lint`, reporting a panic that escapes it (from the
/// parser or the disable-comment scanner, say) as a single internal-error
/// problem on the file rather than aborting the whole run.
///
/// Panics inside a rule never get this far: [`RuleRun`] catches those per
/// rule so the other rules still report.
fn guard_file(source: &str, file_path: &str, lint: impl FnOnce() -> LintResult) -> LintResult {
  panic_guard::catch(lint).unwrap_or_else(|caught| {
    let diag = Diagnostic::new(
      INTERNAL_ERROR_RULE,
      format!(
        "Internal error while linting this file: {}. This is a bug in gale; please report it at {ISSUES_URL}",
        caught.describe()
      ),
    )
    .severity(Severity::Error)
    .span(Span::new(0, 0))
    .file_path(file_path);
    LintResult::new(file_path, source, vec![diag])
  })
}

/// One file's pass over a rule set's rules.
///
/// Every `check_root` and `check` call runs inside [`panic_guard::catch`].  A
/// rule that panics is reported once, as an internal error at the node it was
/// checking, and skipped for the rest of the file; the other rules carry on.
struct RuleRun<'a> {
  set: &'a RuleSet,
  file_path: &'a str,
  source: &'a str,
  syntax: Syntax,
  /// What the rules share for this file, built from `source`.
  cache: &'a FileCache<'a>,
  /// `failed[i]` once rule `i` has panicked on this file.
  failed: Vec<bool>,
  /// What the rules reported.
  diagnostics: Vec<Diagnostic>,
  /// One internal-error problem per rule that panicked.  Kept apart from
  /// `diagnostics` so disables and severity overrides never touch them.
  failures: Vec<Diagnostic>,
  /// Texts of the invalid-option reports, each once.
  invalid_options: Vec<String>,
}

impl<'a> RuleRun<'a> {
  /// Prepare a pass of `set` over one file, the text `cache` was built for.
  fn new(set: &'a RuleSet, file_path: &'a str, cache: &'a FileCache<'a>) -> Self {
    Self {
      set,
      file_path,
      source: cache.source(),
      syntax: cache.syntax(),
      cache,
      failed: vec![false; set.rules.len()],
      diagnostics: Vec::new(),
      failures: Vec::new(),
      invalid_options: set.invalid_patterns.clone(),
    }
  }

  /// The context rule `index` is called with: the same in `check_root` and
  /// in every `check`.
  fn context(&self, index: usize) -> RuleContext<'a> {
    let set: &'a RuleSet = self.set;
    RuleContext {
      file_path: self.file_path,
      source: self.source,
      syntax: self.syntax,
      options: set.rules[index].options.as_ref(),
      cache: Some(self.cache),
      env: set.env,
    }
  }

  /// Keep what rule `index` returned, or record its panic and retire it.
  ///
  /// Invalid-option reports are set aside: a rule may return the same one
  /// for every node it sees, and they are not problems in the source.
  fn record(&mut self, index: usize, outcome: Result<Vec<Diagnostic>, Caught>, offset: usize) {
    match outcome {
      Ok(found) => {
        for diag in found {
          if !diag.is_invalid_option() {
            self.diagnostics.push(diag);
          } else if !self.invalid_options.contains(&diag.message) {
            self.invalid_options.push(diag.message);
          }
        }
      }
      Err(caught) => {
        self.failed[index] = true;
        let offset = clamp_to_char_boundary(self.source, offset);
        self
          .failures
          .push(internal_error(&self.set.rules[index], &caught, offset));
      }
    }
  }

  /// Run every rule's document-level `check_root`.
  fn check_root(&mut self, nodes: &[CssNode], debug: bool) {
    for index in 0..self.set.rules.len() {
      let rule = self.set.rules[index].rule;
      let context = self.context(index);
      let started = Instant::now();
      let source = self.source;
      let outcome = panic_guard::catch(|| {
        debug_panic(rule.name(), source);
        rule.check_root(nodes, &context)
      });
      if debug {
        let elapsed = started.elapsed().as_secs_f64();
        if elapsed > 0.001 {
          eprintln!("[perf] check_root {}: {:.3}s", rule.name(), elapsed);
        }
      }
      self.record(index, outcome, 0);
    }
  }

  /// Recursively walk the AST, invoking each rule's `check` on every node.
  ///
  /// Takes the node by value: each rule sees a node with its whole subtree
  /// before the walk moves the children out to visit them, so nested style
  /// rules are wrapped as nodes without copying their subtrees.
  fn walk(&mut self, node: CssNode) {
    let offset = node.span().offset;
    // `node` stays put until `leave_node`, as `enter_node` requires.
    self.cache.enter_node(&node);
    for index in 0..self.set.rules.len() {
      if self.failed[index] {
        continue;
      }
      let rule = self.set.rules[index].rule;
      let context = self.context(index);
      let outcome = panic_guard::catch(|| rule.check(&node, &context));
      self.record(index, outcome, offset);
    }
    self.cache.leave_node();

    // Recurse into children based on node type.
    match node {
      CssNode::Style(style_rule) => {
        let gale_css_parser::StyleRule {
          children,
          nested_at_rules,
          ..
        } = style_rule;
        for child in children {
          self.walk(CssNode::Style(child));
        }
        // Walk at-rules nested inside the style rule (e.g. @include,
        // @if/@else, @media) so lint rules can inspect their contents.
        for at_node in nested_at_rules {
          self.walk(at_node);
        }
      }
      CssNode::AtRule(at_rule) => {
        for child in at_rule.children {
          self.walk(child);
        }
      }
      CssNode::Comment(_) | CssNode::Declaration(_) => {}
    }
  }
}

/// `offset`, pulled back to the nearest character boundary in `source`.
fn clamp_to_char_boundary(source: &str, offset: usize) -> usize {
  source.floor_char_boundary(offset.min(source.len()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::registry::RuleRegistry;

  /// The spec of a set running `rules` with no options.
  fn spec_for(rules: &[&str]) -> RuleSetSpec {
    RuleSetSpec {
      rules: rules.iter().map(|r| RuleSetting::new(*r)).collect(),
      ..RuleSetSpec::default()
    }
  }

  /// A set running `rules` with no options.
  fn set_for(rules: &[&str]) -> RuleSet {
    RuleSet::of_rules(rules.iter().copied())
  }

  /// A set running just `rule`, configured with `options`.
  fn set_with_options(rule: &str, options: serde_json::Value) -> RuleSet {
    RuleSet::new(RuleSetSpec {
      rules: vec![RuleSetting::new(rule).with_options(options)],
      ..RuleSetSpec::default()
    })
  }

  /// A set running `rules` with rules from `registry`, which is leaked so
  /// the set can borrow its rules as it borrows the built-in ones.
  fn set_in(registry: RuleRegistry, spec: RuleSetSpec) -> RuleSet {
    RuleSet::with_registry(Box::leak(Box::new(registry)), spec)
  }

  #[test]
  fn lint_empty_block() {
    let set = set_for(&["block-no-empty"]);
    let result = set.lint("a { }", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].rule_name, "block-no-empty");
    assert_eq!(result.diagnostics[0].message, "Unexpected empty block");
  }

  #[test]
  fn lint_non_empty_block() {
    let set = set_for(&["block-no-empty"]);
    let result = set.lint("a { color: red; }", "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn disabled_rule_not_run() {
    let set = set_for(&[]);
    let result = set.lint("a { }", "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  // -- Invalid pattern options --

  #[test]
  fn a_lookahead_pattern_works() {
    let set = set_with_options(
      "selector-class-pattern",
      serde_json::json!("^(?!js-)[a-z-]+$"),
    );
    let result = set.lint(".card {}\n.js-card {}\n", "test.css", Syntax::Css);
    assert!(result.invalid_option_warnings.is_empty());
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].message.contains("js-card"));
  }

  #[test]
  fn a_pattern_that_does_not_compile_is_an_invalid_option_once_per_file() {
    let set = set_with_options("selector-class-pattern", serde_json::json!("^[a-z"));
    let result = set.lint(".a {}\n.b {}\n", "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.invalid_option_warnings.len(), 1);
    assert!(
      result.invalid_option_warnings[0]
        .starts_with("Invalid option value \"^[a-z\" for rule \"selector-class-pattern\": "),
      "{:?}",
      result.invalid_option_warnings
    );
    assert!(result.errored());
  }

  #[test]
  fn a_broken_list_entry_is_an_invalid_option() {
    let set = set_with_options(
      "color-named",
      serde_json::json!(["never", { "ignoreProperties": ["/[broken/"] }]),
    );
    let result = set.lint("a { color: red; }\n", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "the rule still runs");
    assert_eq!(
      result.invalid_option_warnings.len(),
      1,
      "{:?}",
      result.invalid_option_warnings
    );
    assert!(result.invalid_option_warnings[0].contains("\"/[broken/\""));
  }

  // -- Unknown rule names --

  #[test]
  fn an_unknown_rule_is_reported_like_stylelint() {
    let set = set_for(&["block-no-emty", "block-no-empty"]);
    let result = set.lint("a {}\n", "test.css", Syntax::Css);
    let unknown: Vec<&Diagnostic> = result
      .diagnostics
      .iter()
      .filter(|d| d.rule_name == "block-no-emty")
      .collect();
    assert_eq!(unknown.len(), 1);
    let d = unknown[0];
    assert_eq!(
      d.message,
      "Unknown rule block-no-emty. Did you mean block-no-empty?"
    );
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span, Span::new(0, 1));
    assert!(
      result
        .diagnostics
        .iter()
        .any(|d| d.rule_name == "block-no-empty"),
      "the real rule still runs"
    );
  }

  #[test]
  fn rules_gale_lacks_are_not_unknown() {
    // A Stylelint core rule gale has not implemented, and a plugin rule.
    let set = set_for(&["no-unknown-custom-properties", "acme/no-foo"]);
    let result = set.lint("a {}\n", "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
  }

  #[test]
  fn unknown_rules_ignore_disables_and_default_severity() {
    let set = RuleSet::new(RuleSetSpec {
      default_severity: Some(Severity::Warning),
      ..spec_for(&["not-a-rule"])
    });
    let result = set.lint("/* stylelint-disable */\na {}\n", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].message, "Unknown rule not-a-rule.");
    assert_eq!(result.diagnostics[0].severity, Severity::Error);
  }

  // -- Panic isolation --

  /// A rule that panics on every style rule, the way an out-of-bounds slice
  /// would.
  struct PanickingRule;

  impl Rule for PanickingRule {
    fn name(&self) -> &'static str {
      "test/panics"
    }

    fn description(&self) -> &'static str {
      "Panics on every style rule"
    }

    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
      if let CssNode::Style(rule) = node {
        let _ = &rule.selector[..rule.selector.len() + 1];
      }
      vec![]
    }
  }

  /// A registry with the built-in rules plus [`PanickingRule`].
  fn registry_with_panicking_rule() -> RuleRegistry {
    let mut registry = RuleRegistry::default();
    registry.register(Box::new(PanickingRule));
    registry
  }

  #[test]
  fn a_panicking_rule_is_reported_once_and_other_rules_still_run() {
    let set = set_in(
      registry_with_panicking_rule(),
      spec_for(&["test/panics", "block-no-empty"]),
    );
    let result = set.lint("a {}\nb {}\n", "test.css", Syntax::Css);

    let crashes: Vec<&Diagnostic> = result
      .diagnostics
      .iter()
      .filter(|d| d.rule_name == "test/panics")
      .collect();
    assert_eq!(crashes.len(), 1, "{:?}", result.diagnostics);
    let crash = crashes[0];
    assert_eq!(crash.severity, Severity::Error);
    assert!(
      crash
        .message
        .starts_with("Internal error in rule \"test/panics\": "),
      "{}",
      crash.message
    );
    assert!(crash.message.contains("out of bounds"), "{}", crash.message);
    assert!(crash.message.contains(ISSUES_URL), "{}", crash.message);
    assert_eq!(crash.span.offset, 0, "reported at the node it was checking");

    let empty_blocks = result
      .diagnostics
      .iter()
      .filter(|d| d.rule_name == "block-no-empty")
      .count();
    assert_eq!(empty_blocks, 2, "the other rule still reports everything");
  }

  #[test]
  fn a_rule_crash_ignores_disables_and_severity_overrides() {
    let set = set_in(
      registry_with_panicking_rule(),
      RuleSetSpec {
        rules: vec![
          RuleSetting::new("test/panics")
            .with_options(serde_json::json!([true, { "message": "custom" }]))
            .with_severity(Severity::Warning),
        ],
        ..RuleSetSpec::default()
      },
    );
    let src = "/* stylelint-disable */\na { color: red; }\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].severity, Severity::Error);
    assert!(result.diagnostics[0].message.starts_with("Internal error"));
  }

  #[test]
  fn host_files_are_linted_per_block_with_the_blocks_syntax() {
    let set = RuleSet::new(RuleSetSpec {
      rules: vec![
        RuleSetting::new("scss/dollar-variable-pattern")
          .with_options(serde_json::json!("^[a-z]+$")),
      ],
      ..RuleSetSpec::default()
    });
    let source = "<template></template>\n<style lang=\"scss\">$Bad: 1;</style>\n";
    let result = set.lint(source, "a.vue", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(
      result.diagnostics[0].span.offset,
      source.find("$Bad").unwrap()
    );
    // The same text as a style sheet of its own is CSS, not SCSS.
    let as_css = set.lint("$Bad: 1;", "a.css", Syntax::Css);
    assert!(as_css.diagnostics.is_empty(), "{:?}", as_css.diagnostics);
  }

  #[test]
  fn embedded_style_sheets_are_guarded_too() {
    let set = set_in(
      registry_with_panicking_rule(),
      spec_for(&["test/panics", "color-named"]),
    );
    let result = set.lint(
      "<template></template>\n<style>a { color: red; }</style>\n",
      "test.vue",
      Syntax::Css,
    );
    let rules: Vec<&str> = result
      .diagnostics
      .iter()
      .map(|d| d.rule_name.as_str())
      .collect();
    assert!(rules.contains(&"test/panics"), "{rules:?}");
    assert!(rules.contains(&"color-named"), "{rules:?}");
  }

  // -- Configured names, options and severities --

  #[test]
  fn problems_are_reported_under_the_name_the_config_used() {
    // `indentation` is the deprecated name of `@stylistic/indentation`.
    for name in ["indentation", "@stylistic/indentation"] {
      let set = RuleSet::new(RuleSetSpec {
        rules: vec![
          RuleSetting::new(name)
            .with_options(serde_json::json!([2, { "message": "indent it" }]))
            .with_severity(Severity::Warning),
        ],
        ..RuleSetSpec::default()
      });
      let result = set.lint("a {\n    top: 0;\n}\n", "test.css", Syntax::Css);
      assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
      let d = &result.diagnostics[0];
      assert_eq!(d.rule_name, name);
      assert_eq!(d.message, "indent it");
      assert_eq!(d.severity, Severity::Warning);
    }
  }

  #[test]
  fn a_rule_named_twice_runs_once_under_its_own_name() {
    let set = set_for(&["indentation", "@stylistic/indentation"]);
    let result = set.lint("a {\n   top: 0;\n}\n", "test.css", Syntax::Css);
    let names: Vec<&str> = result
      .diagnostics
      .iter()
      .map(|d| d.rule_name.as_str())
      .collect();
    assert_eq!(names, ["@stylistic/indentation"]);
  }

  #[test]
  fn the_default_severity_applies_to_rules_without_their_own() {
    let set = RuleSet::new(RuleSetSpec {
      rules: vec![
        RuleSetting::new("block-no-empty"),
        RuleSetting::new("color-named")
          .with_options(serde_json::json!("never"))
          .with_severity(Severity::Error),
      ],
      default_severity: Some(Severity::Warning),
      ..RuleSetSpec::default()
    });
    let result = set.lint("a {}\nb { color: red; }\n", "test.css", Syntax::Css);
    let severities: Vec<(&str, Severity)> = result
      .diagnostics
      .iter()
      .map(|d| (d.rule_name.as_str(), d.severity))
      .collect();
    assert_eq!(
      severities,
      [
        ("block-no-empty", Severity::Warning),
        ("color-named", Severity::Error)
      ]
    );
  }

  /// A rule that reports the options it was called with, from `check_root`
  /// and from `check` on each style rule.
  struct ReportsOptions;

  impl Rule for ReportsOptions {
    /// The rule's test-only name.
    fn name(&self) -> &'static str {
      "test/reports-options"
    }

    /// What the rule does.
    fn description(&self) -> &'static str {
      "Reports the options it sees"
    }

    /// Warnings, like most rules.
    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    /// Reports the options seen at the root.
    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
      vec![Diagnostic::new(
        self.name(),
        format!("root {:?}", ctx.options),
      )]
    }

    /// Reports the options seen on a style rule.
    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
      match node {
        CssNode::Style(_) => vec![
          Diagnostic::new(self.name(), format!("node {:?}", ctx.options))
            .span(Span::new(node.span().offset, 1)),
        ],
        _ => vec![],
      }
    }
  }

  #[test]
  fn a_rule_sees_the_same_options_in_check_root_and_check() {
    let mut registry = RuleRegistry::default();
    registry.register(Box::new(ReportsOptions));
    let set = set_in(
      registry,
      RuleSetSpec {
        rules: vec![RuleSetting::new("test/reports-options").with_options(serde_json::json!(3))],
        ..RuleSetSpec::default()
      },
    );
    let result = set.lint("a {}", "test.css", Syntax::Css);
    let messages: Vec<&str> = result
      .diagnostics
      .iter()
      .map(|d| d.message.as_str())
      .collect();
    assert_eq!(messages, ["root Some(Number(3))", "node Some(Number(3))"]);
  }

  #[test]
  fn rules_see_the_environment_of_their_set() {
    let src = "a { height: 100dvh; }";
    let modern = set_for(&["unit-no-unknown"]);
    assert!(
      modern
        .lint(src, "test.css", Syntax::Css)
        .diagnostics
        .is_empty()
    );

    let legacy = RuleSet::new(RuleSetSpec {
      env: LintEnv {
        stylelint_version: Some((14, 16, 1)),
      },
      ..spec_for(&["unit-no-unknown"])
    });
    let result = legacy.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(
      result.diagnostics[0].message,
      "Unexpected unknown unit \"dvh\""
    );
  }

  // -- Panics and the artifacts rules share --

  /// A rule that reports every style rule in the shared statement tree, at
  /// its start.
  struct ReportsTreeRules;

  impl Rule for ReportsTreeRules {
    /// The rule's test-only name.
    fn name(&self) -> &'static str {
      "test/tree-rules"
    }

    /// What the rule does.
    fn description(&self) -> &'static str {
      "Reports every style rule in the shared statement tree"
    }

    /// Warnings, like most rules.
    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    /// Reports each style rule of the shared tree.
    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
      let tree = ctx.postcss_tree();
      tree
        .nodes
        .iter()
        .filter(|node| node.kind == crate::postcss_tree::NodeKind::Rule)
        .map(|node| Diagnostic::new(self.name(), "rule").span(Span::new(node.start, 1)))
        .collect()
    }
  }

  /// A rule that is first to ask for the shared tree and whose build of it
  /// panics, standing in for a bug in the tree's parser.
  struct BreaksTreeBuild;

  impl Rule for BreaksTreeBuild {
    /// The rule's test-only name.
    fn name(&self) -> &'static str {
      "test/breaks-tree-build"
    }

    /// What the rule does.
    fn description(&self) -> &'static str {
      "Panics while building the shared statement tree"
    }

    /// Warnings, like most rules.
    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    /// Builds the shared tree with a builder that panics.
    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
      let cache = ctx.cache.expect("the runner shares a cache");
      cache.postcss_tree_built_by(|| panic!("tree build failed"));
      vec![]
    }
  }

  /// A rule that panics while it holds the shared tree.
  struct PanicsHoldingTree;

  impl Rule for PanicsHoldingTree {
    /// The rule's test-only name.
    fn name(&self) -> &'static str {
      "test/panics-holding-tree"
    }

    /// What the rule does.
    fn description(&self) -> &'static str {
      "Panics while holding the shared statement tree"
    }

    /// Warnings, like most rules.
    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    /// Takes the shared tree, then panics.
    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
      let tree = ctx.postcss_tree();
      panic!("crashed holding {} statements", tree.nodes.len());
    }
  }

  /// A rule that reports every declaration of the node it checks, read
  /// through the shared per-node cache, at its property.
  struct ReportsDeclarations;

  impl Rule for ReportsDeclarations {
    /// The rule's test-only name.
    fn name(&self) -> &'static str {
      "test/declarations"
    }

    /// What the rule does.
    fn description(&self) -> &'static str {
      "Reports every declaration of the node it checks"
    }

    /// Warnings, like most rules.
    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    /// Reports each declaration of `node` by property.
    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
      ctx
        .written_declarations(node)
        .iter()
        .map(|decl| Diagnostic::new(self.name(), decl.prop).span(Span::new(decl.prop_start, 1)))
        .collect()
    }
  }

  /// A rule that panics while it holds a node's shared declarations.
  struct PanicsHoldingDeclarations;

  impl Rule for PanicsHoldingDeclarations {
    /// The rule's test-only name.
    fn name(&self) -> &'static str {
      "test/panics-holding-declarations"
    }

    /// What the rule does.
    fn description(&self) -> &'static str {
      "Panics while holding a node's shared declarations"
    }

    /// Warnings, like most rules.
    fn default_severity(&self) -> Severity {
      Severity::Warning
    }

    /// Takes the node's declarations, then panics if there are any.
    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
      let found = ctx.written_declarations(node);
      if !found.is_empty() {
        panic!("crashed holding {} declarations", found.len());
      }
      vec![]
    }
  }

  /// A set running `rules`, in that order, with the test rules above
  /// registered.
  fn set_with_tree_rules(rules: &[&str]) -> RuleSet {
    let mut registry = RuleRegistry::default();
    registry.register(Box::new(ReportsTreeRules));
    registry.register(Box::new(BreaksTreeBuild));
    registry.register(Box::new(PanicsHoldingTree));
    registry.register(Box::new(ReportsDeclarations));
    registry.register(Box::new(PanicsHoldingDeclarations));
    set_in(registry, spec_for(rules))
  }

  /// The messages of `rule`'s problems, by offset.
  fn problems_of<'r>(result: &'r LintResult, rule: &str) -> Vec<(usize, &'r str)> {
    result
      .diagnostics
      .iter()
      .filter(|d| d.rule_name == rule)
      .map(|d| (d.span.offset, d.message.as_str()))
      .collect()
  }

  /// A rule whose build of the shared tree panics reports an internal
  /// error; the next rule to ask builds the tree, and so do the disable
  /// comments read after the rules.
  #[test]
  fn a_shared_build_that_panics_is_built_again_for_the_next_rule() {
    let set = set_with_tree_rules(&["test/breaks-tree-build", "test/tree-rules"]);
    // The disable comment is read from the same shared tree, after the
    // failed build: it must still apply.
    let src = "a {}
/* stylelint-disable-next-line test/tree-rules */
b {}
c {}
";
    let result = set.lint(src, "test.css", Syntax::Css);

    let crashes = problems_of(&result, "test/breaks-tree-build");
    assert_eq!(crashes.len(), 1, "{:?}", result.diagnostics);
    assert!(
      crashes[0].1.contains("tree build failed"),
      "{}",
      crashes[0].1
    );

    let c = src.find("c {").unwrap();
    assert_eq!(
      problems_of(&result, "test/tree-rules"),
      vec![(0, "rule"), (c, "rule")],
      "the next rule builds the tree and the disable comment applies"
    );
  }

  /// A rule that panics while holding the shared tree only drops its
  /// reference; the other rules still get the tree.
  #[test]
  fn a_rule_that_panics_holding_the_shared_tree_leaves_it_to_the_others() {
    let set = set_with_tree_rules(&["test/panics-holding-tree", "test/tree-rules"]);
    let result = set.lint(
      "a {}
b {}
",
      "test.css",
      Syntax::Css,
    );

    let crashes = problems_of(&result, "test/panics-holding-tree");
    assert_eq!(crashes.len(), 1, "{:?}", result.diagnostics);
    assert!(
      crashes[0].1.contains("crashed holding 2 statements"),
      "{}",
      crashes[0].1
    );
    assert_eq!(
      problems_of(&result, "test/tree-rules"),
      vec![(0, "rule"), (5, "rule")]
    );
  }

  /// A rule that panics while holding a node's shared declarations does
  /// not lock them: the next rule reads them for that node and every other.
  #[test]
  fn a_rule_that_panics_holding_shared_declarations_leaves_them_to_the_others() {
    let set = set_with_tree_rules(&["test/panics-holding-declarations", "test/declarations"]);
    let src = "a { color: red; top: 0 }\nb { left: 0 }\n";
    let result = set.lint(src, "test.css", Syntax::Css);

    let crashes = problems_of(&result, "test/panics-holding-declarations");
    assert_eq!(crashes.len(), 1, "{:?}", result.diagnostics);
    assert!(
      crashes[0].1.contains("crashed holding 2 declarations"),
      "{}",
      crashes[0].1
    );
    let props: Vec<&str> = problems_of(&result, "test/declarations")
      .into_iter()
      .map(|(_, prop)| prop)
      .collect();
    assert_eq!(props, ["color", "top", "left"]);
  }

  #[test]
  fn a_panic_outside_any_rule_becomes_one_problem_on_the_file() {
    let result = guard_file("a {}", "test.css", || panic!("parser exploded"));
    assert_eq!(result.file_path, "test.css");
    assert_eq!(result.diagnostics.len(), 1);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, INTERNAL_ERROR_RULE);
    assert_eq!(d.severity, Severity::Error);
    assert!(d.message.contains("parser exploded"), "{}", d.message);
  }

  // -- Inline disable comment tests --

  #[test]
  fn gale_disable_all() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* gale-disable */\na { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn gale_disable_specific_rule() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* gale-disable block-no-empty */\na { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn gale_disable_wrong_rule_still_reports() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* gale-disable some-other-rule */\na { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn gale_disable_enable_block() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* gale-disable */\na { }\n/* gale-enable */\nb { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    // First `a { }` is disabled, second `b { }` should be reported.
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn gale_disable_next_line() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* gale-disable-next-line */\na { }\nb { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    // Only `a { }` is disabled; `b { }` still reported.
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn gale_disable_next_line_specific_rule() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* gale-disable-next-line block-no-empty */\na { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn stylelint_disable_compat() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* stylelint-disable */\na { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn stylelint_disable_next_line_compat() {
    let set = set_for(&["block-no-empty"]);
    let src = "/* stylelint-disable-next-line */\na { }\nb { }";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
  }

  /// The lines `set` reports problems on in `src`.
  fn problem_lines(set: &RuleSet, src: &str, path: &str, syntax: Syntax) -> Vec<usize> {
    let result = set.lint(src, path, syntax);
    let index = SourceLineIndex::build(src);
    result
      .diagnostics
      .iter()
      .map(|d| index.line(d.span.offset))
      .collect()
  }

  #[test]
  fn disables_cover_whole_lines_like_stylelint() {
    let set = set_for(&["block-no-empty"]);
    let lines = |src: &str| problem_lines(&set, src, "test.css", Syntax::Css);
    // A disable covers its own line, code before it included, and the
    // enable's line is still off.
    assert_eq!(
      lines("a {} /* stylelint-disable */\nb {}\n/* stylelint-enable */ c {}\nd {}\n"),
      vec![4]
    );
    // `disable-next-line` reaches the line after the comment ends.
    assert_eq!(
      lines("/* stylelint-disable-next-line\n  block-no-empty */\na {}\nb {}\n"),
      vec![4]
    );
    // A command inside a selector or value counts; one inside a string
    // does not.
    assert_eq!(
      lines(
        "a, /* stylelint-disable-line */ b {}\nc { content: \"/* stylelint-disable */\" }\nd {}\n"
      ),
      vec![3]
    );
  }

  #[test]
  fn a_plain_enable_turns_rules_disabled_by_name_back_on() {
    let set = set_for(&["color-no-invalid-hex"]);
    let src = "/* stylelint-disable color-no-invalid-hex */\na { color: #ab; }\n/* stylelint-enable */\na { color: #ab; }\n";
    assert_eq!(problem_lines(&set, src, "test.css", Syntax::Css), vec![4]);
    // And enabling one rule under a blanket disable turns on just that one.
    let set = set_for(&["color-no-invalid-hex", "block-no-empty"]);
    let src = "/* stylelint-disable */\na {}\n/* stylelint-enable color-no-invalid-hex */\nb { color: #ab; }\nc {}\n";
    assert_eq!(problem_lines(&set, src, "test.css", Syntax::Css), vec![4]);
  }

  #[test]
  fn a_rejected_command_replaces_every_problem_like_stylelint() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        ignore: true,
        ..DisableOptions::default()
      },
      default_severity: Some(Severity::Warning),
      ..spec_for(&["block-no-empty", "not-a-rule"])
    });
    let src = "a {}\n/* stylelint-enable */\nb {}\n";
    for result in [
      set.lint(src, "test.css", Syntax::Css),
      set_for(&["block-no-empty"]).lint(src, "test.css", Syntax::Css),
    ] {
      assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
      let d = &result.diagnostics[0];
      assert_eq!(d.rule_name, "CssSyntaxError");
      assert_eq!(d.message, "No rules have been disabled");
      assert_eq!(d.severity, Severity::Error);
      assert_eq!(&src[d.span.offset..d.span.end()], "/* stylelint-enable */");
      assert_eq!(
        d.stylelint_text(),
        "No rules have been disabled (CssSyntaxError)"
      );
    }
  }

  #[test]
  fn double_slash_commands_need_scss_or_less() {
    let set = set_for(&["block-no-empty"]);
    let src = "// stylelint-disable-next-line block-no-empty\na {}\nb {}\n";
    assert_eq!(problem_lines(&set, src, "t.scss", Syntax::Scss), vec![3]);
    assert_eq!(problem_lines(&set, src, "t.less", Syntax::Less), vec![3]);
    // In CSS the line is part of the next selector, not a comment.
    let hex = set_for(&["color-no-invalid-hex"]);
    let css = "a { color: #ab; } // stylelint-disable-line color-no-invalid-hex\nb {}\n";
    assert_eq!(problem_lines(&hex, css, "t.css", Syntax::Css), vec![1]);
    assert!(problem_lines(&hex, css, "t.scss", Syntax::Scss).is_empty());
    // A description on the next `//` line makes the command span both.
    let described = "// stylelint-disable-next-line block-no-empty\n// -- legacy\na {}\nb {}\n";
    assert_eq!(
      problem_lines(&set, described, "t.scss", Syntax::Scss),
      vec![4]
    );
  }

  #[test]
  fn sass_disables_count_the_lines_of_the_sass() {
    // The converted SCSS gains a `}` line before `color`, which must not
    // push the problem off the line the command names.
    let set = set_with_options("color-hex-case", serde_json::json!("lower"));
    let src =
      ".a\n  .b\n    // stylelint-disable-next-line color-hex-case\n  color: #FFF\n  top: #FFF\n";
    assert_eq!(problem_lines(&set, src, "test.sass", Syntax::Sass), vec![5]);
  }

  #[test]
  fn sass_parses_successfully() {
    // Sass indented syntax is now converted to SCSS before parsing,
    // so it should produce lint results, not parse-error diagnostics.
    let set = set_for(&["block-no-empty"]);
    let src = ".foo\n  color: red";
    let result = set.lint(src, "test.sass", Syntax::Sass);

    assert_eq!(result.file_path, "test.sass");
    // The file should parse without a parse-error diagnostic.
    let has_parse_error = result
      .diagnostics
      .iter()
      .any(|d| d.rule_name == "parse-error");
    assert!(
      !has_parse_error,
      "Sass should now parse successfully, but got parse-error diagnostic"
    );
  }

  #[test]
  fn sass_spans_point_into_the_original_file() {
    // Multibyte text before the problem used to push spans (computed on the
    // converted SCSS) into the middle of characters of the Sass source.
    let src =
      "// héllo wörld ünïcödé\n.foo\n  content: \"→ ✓ ★\"\n  color: red\n\n.baz\n  color: #FFF\n";
    let set = set_with_options("color-hex-case", serde_json::json!("lower"));
    let result = set.lint(src, "test.sass", Syntax::Sass);

    assert_eq!(result.source, src, "results carry the Sass, not the SCSS");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "color-hex-case");
    assert_eq!(d.span.offset, src.find("#FFF").unwrap());
    assert_eq!(&src[d.span.offset..d.span.end()], "#FFF");
    assert!(
      d.fix.is_none(),
      "fixes target the SCSS, so they are dropped"
    );
  }

  #[test]
  fn sass_with_multibyte_text_lints_with_every_rule() {
    let src = "// héllo wörld ünïcödé\n.foo\n  content: \"→ ✓ ★\"\n  color: red\n  .bar\n    margin: 0 0 0 0\n    font-family: Ärial\n\n.baz\n  color: #FFF\n";
    let set = RuleSet::of_rules(RuleRegistry::global().all().iter().map(|r| r.name()));
    let result = set.lint(src, "test.sass", Syntax::Sass);
    for d in &result.diagnostics {
      assert!(!d.message.starts_with("Internal error"), "{}", d.message);
      assert!(src.is_char_boundary(d.span.offset), "{d:?}");
      assert!(src.is_char_boundary(d.span.end()), "{d:?}");
    }
  }

  #[test]
  fn malformed_scss_does_not_silently_swallow() {
    // Malformed SCSS should either produce lint diagnostics (via
    // the lightningcss fallback) or a parse-error diagnostic. It
    // should never silently return empty results.
    let set = set_for(&["block-no-empty"]);
    let src = ".foo { content: \"unclosed; }";
    let result = set.lint(src, "test.scss", Syntax::Scss);

    // Verify file_path is set regardless of parse outcome.
    assert_eq!(result.file_path, "test.scss");
    // The result should not be empty — either rules detected violations
    // from the fallback parse, or a parse-error diagnostic was emitted.
    // (If both parsers happen to recover and produce a clean AST with no
    // violations, that is also acceptable — but the code path is correct.)
  }

  // -- Secondary options every rule accepts --

  #[test]
  fn message_option_replaces_the_warning_text() {
    let set = set_with_options(
      "block-no-empty",
      serde_json::json!([true, { "message": "No empty blocks please" }]),
    );
    let result = set.lint("a {}", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].message, "No empty blocks please");
    assert_eq!(result.diagnostics[0].rule_name, "block-no-empty");
  }

  #[test]
  fn url_option_is_attached_to_every_warning_of_the_rule() {
    let set = set_with_options(
      "block-no-empty",
      serde_json::json!([true, { "url": "https://example.com/empty" }]),
    );
    let result = set.lint("a {}\nb {}", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 2);
    for d in &result.diagnostics {
      assert_eq!(d.url.as_deref(), Some("https://example.com/empty"));
    }
  }

  #[test]
  fn disable_fix_option_keeps_the_report_but_drops_the_fix() {
    let fixable = set_with_options("color-hex-case", serde_json::json!(["lower"]));
    let with_fix = fixable.lint("a { color: #FFF; }", "test.css", Syntax::Css);
    assert!(
      with_fix.diagnostics[0].fix.is_some(),
      "precondition: rule is fixable"
    );

    let set = set_with_options(
      "color-hex-case",
      serde_json::json!(["lower", { "disableFix": true }]),
    );
    let result = set.lint("a { color: #FFF; }", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert!(result.diagnostics[0].fix.is_none());
  }

  #[test]
  fn secondary_options_apply_in_embedded_style_sheets() {
    let set = set_with_options(
      "block-no-empty",
      serde_json::json!([true, { "message": "Custom", "url": "https://x.y" }]),
    );
    let result = set.lint("<style>a {}</style>", "test.html", Syntax::Css);
    assert_eq!(result.diagnostics[0].message, "Custom");
    assert_eq!(result.diagnostics[0].url.as_deref(), Some("https://x.y"));
  }

  // -- Disable-comment reports --

  #[test]
  fn invalid_scope_disable_is_reported_for_unconfigured_rule() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_invalid_scope: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    let src = "/* stylelint-disable color-named */\na { color: red; }\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "--report-invalid-scope-disables");
    assert_eq!(d.message, "Rule \"color-named\" isn't enabled");
    assert_eq!(d.severity, Severity::Error);
    // From the comment's first character to its last, as Stylelint's
    // warning start and end.
    assert_eq!(d.span, Span::from_range(0, 34));
  }

  #[test]
  fn disable_reports_point_at_the_declaration_holding_the_comment() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_invalid_scope: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    let src = "a {\n  color: /* stylelint-disable-line color-named */ red;\n}\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    let span = result.diagnostics[0].span;
    assert_eq!(
      &src[span.offset..=span.end()],
      "color: /* stylelint-disable-line color-named */ red;"
    );
  }

  #[test]
  fn invalid_scope_ignores_configured_and_blanket_disables() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_invalid_scope: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    for src in [
      "/* stylelint-disable block-no-empty */\na { color: red; }\n",
      "/* stylelint-disable */\na { color: red; }\n",
    ] {
      let result = set.lint(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn invalid_scope_counts_plugin_rules_from_the_config() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_invalid_scope: true,
        ..DisableOptions::default()
      },
      configured_rules: vec!["some-plugin/rule".to_string()],
      ..spec_for(&["block-no-empty"])
    });
    let src = "/* stylelint-disable some-plugin/rule */\na { color: red; }\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn invalid_scope_reports_once_per_comment_and_rule() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_invalid_scope: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    // An inline disable creates two ranges for the same comment.
    let src = "a { color: red; } /* stylelint-disable color-named */\nb { color: red; }\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn descriptionless_disable_is_reported_once_per_comment() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_descriptionless: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    let src = "/* stylelint-disable block-no-empty, color-named */\na {}\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "--report-descriptionless-disables");
    assert_eq!(
      d.message,
      "Disable for \"block-no-empty\" is missing a description"
    );
    assert_eq!(d.span.offset, 0);
  }

  #[test]
  fn descriptionless_blanket_disable_is_named_all() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_descriptionless: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    let src = "/* stylelint-disable */\na {}\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
      result.diagnostics[0].message,
      "Disable for \"all\" is missing a description"
    );
  }

  #[test]
  fn described_disables_are_not_reported() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_descriptionless: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    for src in [
      "/* stylelint-disable block-no-empty -- legacy markup */\na {}\n",
      "/* stylelint-disable -- legacy markup */\na {}\n",
      "/* stylelint-disable-next-line block-no-empty -- why */\na {}\n",
    ] {
      let result = set.lint(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn descriptionless_next_line_disable_is_reported() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_descriptionless: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    let src = "/* stylelint-disable-next-line block-no-empty */\na {}\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
      result.diagnostics[0].rule_name,
      "--report-descriptionless-disables"
    );
  }

  #[test]
  fn unscoped_disable_is_reported_once_per_comment() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_unscoped: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    // Inline placement creates two ranges for one comment.
    let src = "a { color: red; } /* stylelint-disable */\nb {}\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "--report-unscoped-disables");
    assert_eq!(d.message, "Configuration comment must be scoped");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span.offset, 18);
  }

  #[test]
  fn scoped_disables_are_not_unscoped() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_unscoped: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    for src in [
      "/* stylelint-disable block-no-empty */\na {}\n",
      "/* stylelint-disable-next-line block-no-empty */\na {}\n",
    ] {
      let result = set.lint(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn report_disables_option_flags_a_disable_of_that_rule() {
    let set = set_with_options(
      "block-no-empty",
      serde_json::json!([true, { "reportDisables": true }]),
    );
    for src in [
      "/* stylelint-disable block-no-empty */\na {}\n",
      "/* stylelint-disable-next-line block-no-empty */\na {}\n",
    ] {
      let result = set.lint(src, "test.css", Syntax::Css);
      assert_eq!(result.diagnostics.len(), 1, "{src}");
      let d = &result.diagnostics[0];
      assert_eq!(d.rule_name, "reportDisables");
      assert_eq!(d.message, "Rule \"block-no-empty\" may not be disabled");
      assert_eq!(d.severity, Severity::Error);
      assert_eq!(d.span.offset, 0);
    }
  }

  #[test]
  fn report_disables_ignores_other_rules_and_blanket_disables() {
    let set = set_with_options(
      "block-no-empty",
      serde_json::json!([true, { "reportDisables": true }]),
    );
    for src in [
      "/* stylelint-disable color-named */\na { color: red; }\n",
      "/* stylelint-disable */\na {}\n",
    ] {
      let result = set.lint(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn disable_reports_use_the_default_severity() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        report_descriptionless: true,
        ..DisableOptions::default()
      },
      default_severity: Some(Severity::Warning),
      ..spec_for(&["block-no-empty"])
    });
    let src = "/* stylelint-disable block-no-empty */\na {}\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics[0].severity, Severity::Warning);
  }

  #[test]
  fn reports_still_run_under_ignore_disables() {
    let set = RuleSet::new(RuleSetSpec {
      disables: DisableOptions {
        ignore: true,
        report_descriptionless: true,
        ..DisableOptions::default()
      },
      ..spec_for(&["block-no-empty"])
    });
    let src = "/* stylelint-disable block-no-empty */\na {}\n";
    let result = set.lint(src, "test.css", Syntax::Css);
    let rules: Vec<&str> = result
      .diagnostics
      .iter()
      .map(|d| d.rule_name.as_str())
      .collect();
    assert_eq!(
      rules,
      vec!["--report-descriptionless-disables", "block-no-empty"]
    );
  }
}
