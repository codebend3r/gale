//! Linting a file against a [`RuleSet`].
//!
//! A [`RuleSet`] is everything that decides how one file is linted: the rules
//! its config enables, each with the name the config used, its options and
//! its severity, plus the disable-comment settings and `defaultSeverity`.
//! It is built once per distinct effective config and shared by every file
//! that config applies to; [`RuleSet::lint`] is the one way a file is linted.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::{SourceMap, Syntax, lintable_source};
use gale_diagnostics::{Diagnostic, LintResult, Severity, SourceLineIndex, Span};

use crate::disables::{self, DisablePolicy};
use crate::embedded::{EmbeddedRoot, HostLanguage, lint_host};
use crate::file_cache::FileCache;
use crate::known_rules::{self, RuleSupport};
use crate::lint_env::LintEnv;
use crate::options::Setting;
use crate::panic_guard::{self, Caught, ISSUES_URL};
use crate::pattern;
use crate::registry::{RuleRegistry, resolve_deprecated_alias};
use crate::rule::{Check, Rule, RuleContext, RuleMeta};

/// The secondary options every Stylelint rule accepts, whatever the rule
/// itself does with its options, read once from a rule's setting.
#[derive(Debug, Clone, Default)]
struct CommonOptions {
  /// `message`, which replaces the warning text.
  message: Option<String>,
  /// `url`, a documentation link attached to the warning.
  url: Option<String>,
  /// `disableFix: true`, which keeps the report but drops its autofix.
  disable_fix: bool,
  /// `reportDisables: true`, which reports every comment disabling the rule.
  report_disables: bool,
}

impl CommonOptions {
  /// The common options in a rule's setting.
  fn of(setting: &Setting) -> Self {
    let Some(secondary) = setting.secondary() else {
      return Self::default();
    };
    let text = |key: &str| {
      secondary
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
    };
    let flag = |key: &str| secondary.get(key).and_then(|v| v.as_bool()) == Some(true);
    Self {
      message: text("message"),
      url: text("url"),
      disable_fix: flag("disableFix"),
      report_disables: flag("reportDisables"),
    }
  }

  /// Apply the options to one of the rule's problems.
  fn apply(&self, diag: &mut Diagnostic) {
    if let Some(message) = &self.message {
      crate::custom_message::apply(diag, message);
    }
    if let Some(url) = &self.url {
      diag.url = Some(url.clone());
    }
    if self.disable_fix {
      diag.fix = None;
    }
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

/// The `tracing` target the per-phase timings are emitted under, at debug
/// level.  The CLI turns it on for `GALE_DEBUG_PERF=1`; `GALE_LOG` can
/// select it like any other target (`GALE_LOG=gale_perf=debug`).
pub const PERF_TARGET: &str = "gale_perf";

/// A rule that takes longer than this on a file is timed on its own.
const SLOW_RULE_SECS: f64 = 0.001;

/// Times the phases of linting one style sheet, when the timings are on.
///
/// Off, it costs one check of the tracing filter per file and no clock
/// reads.
struct PhaseTimer<'a> {
  file_path: &'a str,
  /// When the current phase started, or `None` with the timings off.
  started: Option<Instant>,
}

impl<'a> PhaseTimer<'a> {
  /// Start timing the first phase of linting `file_path`.
  fn start(file_path: &'a str) -> Self {
    let on = tracing::enabled!(target: PERF_TARGET, tracing::Level::DEBUG);
    Self {
      file_path,
      started: on.then(Instant::now),
    }
  }

  /// Whether the timings are on.
  fn on(&self) -> bool {
    self.started.is_some()
  }

  /// Report the phase that just ended as `phase`, and start the next.
  fn lap(&mut self, phase: &'static str) {
    if let Some(started) = self.started {
      let now = Instant::now();
      tracing::debug!(
        target: PERF_TARGET,
        file = self.file_path,
        phase,
        secs = (now - started).as_secs_f64(),
      );
      self.started = Some(now);
    }
  }
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
  /// The options as the config wrote them, scanned for regex literals that
  /// do not compile.
  options: Option<serde_json::Value>,
  /// The options as the rule reads them.
  setting: Setting,
  /// The secondary options every rule accepts.
  common: CommonOptions,
  /// The severity every problem of the rule is given: the config's for the
  /// rule, else its `defaultSeverity`, else the rule's own default.
  severity: Severity,
  /// The rule configured with its options, once for the whole set; `None`
  /// for a rule whose options are invalid, which does not run.
  check: Option<Box<dyn Check>>,
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
  /// The configuration-comment policy for sheets and embedded documents.
  disable_policy: DisablePolicy,
  /// The problems with the rules' options, as invalid-option texts, each
  /// once: those that keep a rule from running, and regex literals that do
  /// not compile.  Found when the set is built and reported on every file.
  invalid_options: Vec<String>,
  env: LintEnv,
}

/// What a rule set assumes about a rule gale does not have, to read its
/// common secondary options: that its primary option is a plain value.
const UNKNOWN_RULE: RuleMeta = RuleMeta {
  name: "",
  description: "",
  default_severity: Severity::Warning,
  primary_option_array: false,
  primary_option_object: false,
};

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
      let Some(rule) = registry.get(&setting.name) else {
        // A rule gale does not have still honours `reportDisables`.
        let unknown_setting = Setting::new(&UNKNOWN_RULE, setting.options.as_ref());
        if CommonOptions::of(&unknown_setting).report_disables {
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
      let meta = rule.meta();
      let rule_setting = Setting::new(&meta, setting.options.as_ref());
      let common = CommonOptions::of(&rule_setting);
      if common.report_disables {
        forbids_disable.insert(rule.name().to_string());
        forbids_disable.insert(setting.name.clone());
      }
      let active = ActiveRule {
        rule,
        config_name: setting.name,
        options: setting.options,
        setting: rule_setting,
        common,
        severity: setting
          .severity
          .or(default_severity)
          .unwrap_or(meta.default_severity),
        check: None,
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

    let mut invalid_options: Vec<String> = Vec::new();
    let mut report = |text: String| {
      if !invalid_options.contains(&text) {
        invalid_options.push(text);
      }
    };
    for active in &mut rules {
      // Each rule is configured once here, for every file the set lints.  A
      // rule whose options are invalid does not run.
      match active.rule.configure(&active.setting, &env) {
        Ok(check) => active.check = Some(check),
        Err(problems) => problems
          .into_iter()
          .for_each(|problem| report(problem.into_text())),
      }
      // A regex literal that does not compile is reported wherever it is,
      // but only stops the rule when the rule says so: in a list it just
      // matches nothing.
      if let Some(options) = &active.options {
        for invalid in pattern::invalid_options(active.rule.name(), options) {
          report(invalid);
        }
      }
    }

    Self {
      rules,
      by_name,
      unknown,
      disable_policy: DisablePolicy {
        options: disables,
        default_severity,
        configured,
        forbids_disable,
      },
      invalid_options,
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
    self.disable_policy.options
  }

  /// The environment rules see.
  pub fn env(&self) -> LintEnv {
    self.env
  }

  /// The problems with the rules' options, as Stylelint words them, which
  /// every file linted with this set reports as invalid-option warnings.
  pub fn invalid_options(&self) -> &[String] {
    &self.invalid_options
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
        let mut result = lint_host(
          source,
          file_path,
          host,
          &self.disable_policy,
          |text, syntax, root| self.lint_sheet(text, file_path, syntax, Some(root)),
        );
        // A file with no style sheet in it still has the set's options.
        if result.invalid_option_warnings.is_empty() {
          result.invalid_option_warnings = self.invalid_options.clone();
        }
        result
      }
      None => self.lint_sheet(source, file_path, syntax, None),
    }
  }

  /// Lint one style sheet, reporting a panic outside any one rule as a
  /// single internal-error problem on the file.
  fn lint_sheet(
    &self,
    source: &str,
    file_path: &str,
    syntax: Syntax,
    embedded: Option<&EmbeddedRoot>,
  ) -> LintResult {
    guard_file(source, file_path, || {
      self.lint_sheet_unguarded(source, file_path, syntax, embedded)
    })
  }

  /// [`Self::lint_sheet`] without the file-level panic guard.
  fn lint_sheet_unguarded(
    &self,
    source: &str,
    file_path: &str,
    syntax: Syntax,
    embedded: Option<&EmbeddedRoot>,
  ) -> LintResult {
    let mut timer = PhaseTimer::start(file_path);
    // Rules read the text the tree is parsed from, which for Sass is the
    // converted SCSS.  `finish` maps their spans back.
    let (text, source_map) = lintable_source(source, syntax);
    // Parsed here, before any rule runs and outside their panic guards, so
    // a parser bug is one internal error on the file (see `guard_file`)
    // rather than one per rule.
    let cache = FileCache::new(&text, syntax);
    let tree = cache.postcss_tree();
    timer.lap("parse");

    let mut run = RuleRun::new(self, file_path, &cache, embedded);
    run.check(&tree, timer.on());
    timer.lap("rules");
    let RuleRun {
      mut diagnostics,
      failures,
      ..
    } = run;

    for diag in &mut diagnostics {
      if diag.file_path.is_empty() {
        diag.file_path = file_path.to_string();
      }
      self.settle(diag);
    }

    if let Some(rejected) = self.apply_disables(
      &mut diagnostics,
      source,
      source_map.as_ref(),
      &cache,
      file_path,
      embedded.is_some(),
    ) {
      return finish(
        file_path,
        source,
        source_map.as_ref(),
        vec![rejected],
        Vec::new(),
      );
    }
    timer.lap("disables");

    // Rule crashes go in last so that disables, severity overrides and the
    // `message` option cannot hide or soften them.
    diagnostics.extend(failures);

    let unknown = self.unknown_rule_problems(source, file_path);
    let mut result = finish(file_path, source, source_map.as_ref(), diagnostics, unknown);
    result.invalid_option_warnings = self.invalid_options.clone();
    timer.lap("finish");
    result
  }

  /// Give a rule's problem what its configuration says: the `message`,
  /// `url` and `disableFix` options, its severity (the configured one, else
  /// the `defaultSeverity`, else the rule's default), and the rule name the
  /// config used.
  fn settle(&self, diag: &mut Diagnostic) {
    let active = self
      .by_name
      .get(diag.rule_name.as_str())
      .map(|&index| &self.rules[index]);
    if let Some(active) = active {
      active.common.apply(diag);
    }
    let severity = match active {
      Some(active) => Some(active.severity),
      None => self.disable_policy.default_severity,
    };
    if let Some(severity) = severity {
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
  /// The rules linted `cache`'s text (the converted SCSS, for Sass), whose
  /// offsets `source_map` takes back to `source` so that lines are those
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
    source_map: Option<&SourceMap>,
    cache: &FileCache,
    file_path: &str,
    embedded: bool,
  ) -> Option<Diagnostic> {
    let text = cache.source();
    if embedded || !disables::may_have_directives(text) {
      return None;
    }
    let lines = SourceLineIndex::build(source);
    let line_of = |offset: usize| match source_map {
      Some(map) => lines.line(map.to_original(offset)),
      None => lines.line(offset),
    };
    let mut collector = disables::Collector::new(&line_of);
    collector.scan_tree(&cache.postcss_tree(), cache.syntax(), 0);
    let (ranges, rejected) = collector.finish();
    if let Some(rejected) = rejected {
      return Some(rejected.to_diagnostic(file_path));
    }
    self
      .disable_policy
      .apply(diagnostics, &ranges, &line_of, file_path);
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
/// When the rules linted converted text (Sass), every span is mapped back
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
  source_map: Option<&SourceMap>,
  mut diagnostics: Vec<Diagnostic>,
  unknown_rules: Vec<Diagnostic>,
) -> LintResult {
  if let Some(map) = source_map {
    for diag in &mut diagnostics {
      for span in std::iter::once(&mut diag.span)
        .chain(diag.related.iter_mut().map(|related| &mut related.span))
      {
        let start = clamp_to_char_boundary(source, map.to_original(span.offset));
        let end = clamp_to_char_boundary(source, map.to_original(span.end()));
        *span = Span::new(start, end.saturating_sub(start));
      }
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
/// enough to file a bug.  It sits at the start of the file: a rule checks
/// the whole file at once.
fn internal_error(active: &ActiveRule, caught: &Caught) -> Diagnostic {
  Diagnostic::new(
    active.config_name.as_str(),
    format!(
      "Internal error in rule \"{}\": {}. This is a bug in gale; please report it at {ISSUES_URL}",
      active.rule.name(),
      caught.describe()
    ),
  )
  .severity(Severity::Error)
  .span(Span::new(0, 0))
}

/// The rule name for a crash that happened outside any one rule, such as in
/// the parser.
pub const INTERNAL_ERROR_RULE: &str = "internal-error";

/// Lint one file through `lint`, reporting a panic that escapes it (from the
/// tree's parser or the disable-comment scanner, say) as a single internal-error
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
/// Every rule's check runs inside [`panic_guard::catch`].  A rule that
/// panics is reported once, as an internal error at the start of the file;
/// the other rules carry on.
struct RuleRun<'a> {
  set: &'a RuleSet,
  file_path: &'a str,
  source: &'a str,
  syntax: Syntax,
  /// What the rules share for this file, built from `source`.
  cache: &'a FileCache<'a>,
  embedded: Option<&'a EmbeddedRoot>,
  /// What the rules reported.
  diagnostics: Vec<Diagnostic>,
  /// One internal-error problem per rule that panicked.  Kept apart from
  /// `diagnostics` so disables and severity overrides never touch them.
  failures: Vec<Diagnostic>,
}

impl<'a> RuleRun<'a> {
  /// Prepare a pass of `set` over one file, the text `cache` was built for.
  fn new(
    set: &'a RuleSet,
    file_path: &'a str,
    cache: &'a FileCache<'a>,
    embedded: Option<&'a EmbeddedRoot>,
  ) -> Self {
    Self {
      set,
      file_path,
      source: cache.source(),
      syntax: cache.syntax(),
      cache,
      embedded,
      diagnostics: Vec::new(),
      failures: Vec::new(),
    }
  }

  /// The context every rule is called with.
  fn context(&self) -> RuleContext<'a> {
    RuleContext {
      file_path: self.file_path,
      source: self.source,
      syntax: self.syntax,
      cache: Some(self.cache),
      env: self.set.env,
      embedded: self.embedded,
    }
  }

  /// Keep what rule `index` returned, or record its panic.
  fn record(&mut self, index: usize, outcome: Result<Vec<Diagnostic>, Caught>) {
    match outcome {
      Ok(found) => self.diagnostics.extend(found),
      Err(caught) => self
        .failures
        .push(internal_error(&self.set.rules[index], &caught)),
    }
  }

  /// Run every rule over `tree`, the tree of the file's text.
  ///
  /// With `timed`, a rule that is slow on the file is timed on its own.
  fn check(&mut self, tree: &PostcssTree, timed: bool) {
    let context = self.context();
    for index in 0..self.set.rules.len() {
      let set: &'a RuleSet = self.set;
      let rule = set.rules[index].rule;
      if self.embedded.is_some_and(|root| !root.allows(rule.name())) {
        continue;
      }
      let Some(check) = set.rules[index].check.as_deref() else {
        continue;
      };
      let started = timed.then(Instant::now);
      let source = self.source;
      let outcome = panic_guard::catch(|| {
        debug_panic(rule.name(), source);
        check.check(tree, &context)
      });
      self.record(index, outcome);
      if let Some(started) = started {
        let secs = started.elapsed().as_secs_f64();
        if secs > SLOW_RULE_SECS {
          tracing::debug!(
            target: PERF_TARGET,
            file = self.file_path,
            rule = rule.name(),
            secs,
            "slow rule"
          );
        }
      }
    }
  }
}

/// `offset`, pulled back to the nearest character boundary in `source`.
fn clamp_to_char_boundary(source: &str, offset: usize) -> usize {
  source.floor_char_boundary(offset.min(source.len()))
}

#[cfg(test)]
mod tests {
  use gale_css_parser::postcss_tree::PostcssTree;

  use super::*;

  use crate::options::InvalidOption;
  use crate::registry::RuleRegistry;

  /// Implement [`Rule`] for `$rule`, a unit struct that is also its own
  /// configured form: a test rule that takes no options and warns.
  macro_rules! test_rule {
    ($rule:ident, $name:literal, $description:literal) => {
      impl Rule for $rule {
        fn meta(&self) -> RuleMeta {
          RuleMeta {
            name: $name,
            description: $description,
            default_severity: Severity::Warning,
            primary_option_array: false,
            primary_option_object: false,
          }
        }

        fn configure(
          &self,
          _setting: &Setting,
          _env: &LintEnv,
        ) -> Result<Box<dyn Check + '_>, Vec<InvalidOption>> {
          Ok(Box::new($rule))
        }
      }
    };
  }

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

  // -- Rules configured once per set --

  thread_local! {
    /// How often [`FlagsRules`] has been configured on this test's thread.
    static FLAGS_CONFIGURED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
  }

  /// A rule with typed options: it flags every style rule, with the word
  /// its primary option names (`"a"` or `"b"`).
  struct FlagsRules {
    word: &'static str,
  }

  impl FlagsRules {
    fn from_setting(
      setting: &Setting,
      _env: &LintEnv,
    ) -> Result<Self, Vec<crate::options::InvalidOption>> {
      FLAGS_CONFIGURED.set(FLAGS_CONFIGURED.get() + 1);
      let mut options = setting.options();
      let word = options.primary_one_of(&["a", "b"]).unwrap_or("a");
      options.flag("ignoreNothing");
      options.finish()?;
      Ok(Self { word })
    }
  }

  impl Check for FlagsRules {
    fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
      tree
        .walk_rules()
        .map(|rule| Diagnostic::new("test/flags", self.word).span(Span::new(rule.start, 1)))
        .collect()
    }
  }

  crate::rule::declare_rule! {
    /// Flags every style rule.
    struct Flags => FlagsRules {
      name: "test/flags",
      description: "Flags every style rule",
      default_severity: Error,
    }
  }

  /// A set running [`Flags`] as `setting` says, with `default_severity`.
  fn flags_set(setting: RuleSetting, default_severity: Option<Severity>) -> RuleSet {
    let mut registry = RuleRegistry::default();
    registry.register(Box::new(Flags));
    set_in(
      registry,
      RuleSetSpec {
        rules: vec![setting],
        default_severity,
        ..RuleSetSpec::default()
      },
    )
  }

  #[test]
  fn a_rule_with_typed_options_is_configured_once_per_set() {
    let before = FLAGS_CONFIGURED.get();
    let set = flags_set(
      RuleSetting::new("test/flags").with_options(serde_json::json!("b")),
      None,
    );
    for _ in 0..3 {
      let result = set.lint("a {}\nb {}\n", "test.css", Syntax::Css);
      let words: Vec<&str> = result
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
      assert_eq!(words, ["b", "b"]);
    }
    assert_eq!(FLAGS_CONFIGURED.get() - before, 1);
    assert!(set.invalid_options().is_empty());
  }

  #[test]
  fn the_runner_gives_problems_their_severity() {
    let severity_of = |setting: RuleSetting, default: Option<Severity>| {
      let result = flags_set(setting, default).lint("a {}\n", "test.css", Syntax::Css);
      result.diagnostics[0].severity
    };
    // The rule leaves its problems at `Diagnostic::new`'s warning; with no
    // severity configured it reports at its own default.
    assert_eq!(
      severity_of(RuleSetting::new("test/flags"), None),
      Severity::Error
    );
    assert_eq!(
      severity_of(RuleSetting::new("test/flags"), Some(Severity::Warning)),
      Severity::Warning,
      "defaultSeverity comes before the rule's default"
    );
    assert_eq!(
      severity_of(
        RuleSetting::new("test/flags").with_severity(Severity::Warning),
        Some(Severity::Error)
      ),
      Severity::Warning,
      "the rule's own severity comes first"
    );
  }

  #[test]
  fn invalid_options_stop_the_rule_and_are_reported_on_every_file() {
    let set = flags_set(
      RuleSetting::new("test/flags").with_options(serde_json::json!(["c", { "ignoreAll": true }])),
      None,
    );
    let expected = [
      "Invalid option value \"c\" for rule \"test/flags\"",
      "Invalid option name \"ignoreAll\" for rule \"test/flags\"",
    ];
    assert_eq!(set.invalid_options(), expected);
    for (source, path) in [
      ("a {}\n", "a.css"),
      ("", "empty.css"),
      ("<template></template>\n", "nothing.vue"),
      ("<style>a {}</style>\n", "styled.vue"),
    ] {
      let result = set.lint(source, path, Syntax::Css);
      assert!(
        result.diagnostics.is_empty(),
        "{path}: {:?}",
        result.diagnostics
      );
      assert_eq!(result.invalid_option_warnings, expected, "{path}");
      assert!(result.errored(), "{path}");
    }
  }

  #[test]
  fn every_rule_gets_the_common_secondary_options() {
    let set = flags_set(
      RuleSetting::new("test/flags").with_options(serde_json::json!([
        "a",
        { "message": "custom", "url": "https://example.test", "ignoreNothing": true }
      ])),
      None,
    );
    let result = set.lint("a {}\n", "test.css", Syntax::Css);
    assert!(result.invalid_option_warnings.is_empty());
    assert_eq!(result.diagnostics[0].message, "custom");
    assert_eq!(
      result.diagnostics[0].url.as_deref(),
      Some("https://example.test")
    );
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

  /// A rule that panics on any style rule, the way an out-of-bounds slice
  /// would.
  struct PanickingRule;

  test_rule!(PanickingRule, "test/panics", "Panics on every style rule");

  impl Check for PanickingRule {
    fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
      for rule in tree.walk_rules() {
        let _ = &rule.selector()[..rule.selector().len() + 1];
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
    assert_eq!(crash.span.offset, 0, "reported at the start of the file");

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

  test_rule!(
    ReportsTreeRules,
    "test/tree-rules",
    "Reports every style rule in the shared statement tree"
  );

  impl Check for ReportsTreeRules {
    /// Reports each style rule of the tree it is handed, which must be the
    /// one every rule shares.
    fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
      assert!(std::ptr::eq(tree, &*ctx.postcss_tree()), "the shared tree");
      tree
        .walk_rules()
        .map(|rule| Diagnostic::new("test/tree-rules", "rule").span(Span::new(rule.start, 1)))
        .collect()
    }
  }

  /// A rule that panics while it holds the shared tree.
  struct PanicsHoldingTree;

  test_rule!(
    PanicsHoldingTree,
    "test/panics-holding-tree",
    "Panics while holding the shared statement tree"
  );

  impl Check for PanicsHoldingTree {
    /// Panics holding the shared tree.
    fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
      panic!("crashed holding {} statements", tree.nodes.len());
    }
  }

  struct MustNotRunEmbedded;
  test_rule!(
    MustNotRunEmbedded,
    "@stylistic/unicode-bom",
    "Must not execute in embedded roots"
  );

  impl Check for MustNotRunEmbedded {
    fn check(&self, _tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
      panic!("excluded rule executed");
    }
  }

  #[test]
  fn embedded_exclusions_are_applied_before_rule_execution() {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(MustNotRunEmbedded));
    let set = set_in(registry, spec_for(&["@stylistic/unicode-bom"]));
    assert!(
      set
        .lint("<style>a{}</style>", "x.vue", Syntax::Css)
        .diagnostics
        .is_empty()
    );
    assert_eq!(set.lint("a{}", "x.css", Syntax::Css).diagnostics.len(), 1);
  }

  /// A set running `rules`, in that order, with the test rules above
  /// registered.
  fn set_with_tree_rules(rules: &[&str]) -> RuleSet {
    let mut registry = RuleRegistry::default();
    registry.register(Box::new(ReportsTreeRules));
    registry.register(Box::new(PanicsHoldingTree));
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
  fn embedded_disable_reports_share_document_scope() {
    let set = RuleSet::new(RuleSetSpec {
      rules: vec![
        RuleSetting::new("block-no-empty")
          .with_options(serde_json::json!([true, {"reportDisables":true}])),
      ],
      disables: DisableOptions {
        report_descriptionless: true,
        ..Default::default()
      },
      ..Default::default()
    });
    let source = "<style>/* stylelint-disable block-no-empty */</style>\n<style>a{} /* stylelint-enable block-no-empty */</style>";
    let result = set.lint(source, "x.vue", Syntax::Css);
    assert_eq!(
      result
        .diagnostics
        .iter()
        .filter(|d| d.rule_name == "reportDisables")
        .count(),
      1
    );
    assert_eq!(
      result
        .diagnostics
        .iter()
        .filter(|d| d.rule_name == "--report-descriptionless-disables")
        .count(),
      1
    );
    assert!(
      !result
        .diagnostics
        .iter()
        .any(|d| d.rule_name == "CssSyntaxError" || d.rule_name == "block-no-empty")
    );
  }

  #[test]
  fn embedded_custom_messages_are_not_parsed_as_source_locations() {
    let message = "first used at line 123";
    let set = set_with_options(
      "no-duplicate-selectors",
      serde_json::json!([true, {"message":message}]),
    );
    let result = set.lint(
      "<template/>\n<style>\n.a{}\n.a{}\n</style>",
      "x.vue",
      Syntax::Css,
    );
    assert_eq!(result.diagnostics[0].message, message);
    assert_eq!(result.diagnostics[0].related.len(), 1);
  }

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
  fn sass_is_linted_as_the_scss_it_converts_to() {
    let set = set_for(&["color-named"]);
    let result = set.lint(".foo\n  color: red", "test.sass", Syntax::Sass);
    assert_eq!(result.file_path, "test.sass");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].span.offset, 14);
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
  fn malformed_scss_is_linted_as_far_as_it_parses() {
    // The tree recovers from the unclosed string the way
    // `postcss-safe-parser` does, so the rules still see the file.
    let set = set_for(&["block-no-empty"]);
    let result = set.lint(
      "a {}\n.foo { content: \"unclosed; }",
      "test.scss",
      Syntax::Scss,
    );
    assert_eq!(result.file_path, "test.scss");
    let rules: Vec<&str> = result
      .diagnostics
      .iter()
      .map(|d| d.rule_name.as_str())
      .collect();
    assert_eq!(rules, ["block-no-empty"]);
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
