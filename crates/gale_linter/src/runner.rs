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
use crate::known_rules::{self, RuleSupport};
use crate::lint_env::LintEnv;
use crate::options::{DISABLE_FIX_KEY, MESSAGE_KEY, REPORT_DISABLES_KEY, Setting, URL_KEY};
use crate::panic_guard::{self, Caught};
use crate::pattern;
use crate::registry::{RuleRegistry, canonical_name};
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
      message: text(MESSAGE_KEY),
      url: text(URL_KEY),
      disable_fix: flag(DISABLE_FIX_KEY),
      report_disables: flag(REPORT_DISABLES_KEY),
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
      .map(|name| canonical_name(name).to_string())
      .collect();
    let mut forbids_disable = HashSet::new();
    let mut rules: Vec<ActiveRule> = Vec::with_capacity(settings.len());
    let mut by_name: HashMap<&'static str, usize> = HashMap::with_capacity(settings.len());
    let mut unknown = Vec::new();

    for setting in settings {
      configured.insert(canonical_name(&setting.name).to_string());
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
        never_reports: registry
          .all()
          .iter()
          .filter(|rule| rule.never_reports())
          .map(|rule| rule.name())
          .collect(),
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
    let tree = PostcssTree::parse(&text, syntax);
    timer.lap("parse");

    let mut run = RuleRun::new(self, file_path, &tree, embedded);
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
      &tree,
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
    match active {
      Some(active) => {
        active.common.apply(diag);
        diag.severity = active.severity;
        if active.config_name != diag.rule_name {
          diag.rule_name = active.config_name.clone();
        }
      }
      None => {
        if let Some(severity) = self.disable_policy.default_severity {
          diag.severity = severity;
        }
      }
    }
  }

  /// Apply the file's configuration comments to `diagnostics`: drop the
  /// problems they disable and add the reports about the comments.
  ///
  /// The rules linted `tree`'s text (the converted SCSS, for Sass), whose
  /// offsets `source_map` takes back to `source` so that lines are those
  /// of the file the author wrote.  In a style sheet embedded in an
  /// HTML-like file the host applies the ranges of the whole document
  /// instead, so only the reports are added here.  The comments are found
  /// in the statement tree the rules share, `tree`, parsed from that text.
  ///
  /// Returns Stylelint's `CssSyntaxError` for a comment it rejects (an
  /// `enable` with nothing disabled, say), which replaces every other
  /// problem in the file, `ignoreDisables` or not.
  fn apply_disables(
    &self,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    source_map: Option<&SourceMap>,
    tree: &PostcssTree,
    file_path: &str,
    embedded: bool,
  ) -> Option<Diagnostic> {
    let text = tree.source();
    if embedded || !disables::may_have_directives(text) {
      return None;
    }
    let lines = SourceLineIndex::build(source);
    let line_of = |offset: usize| match source_map {
      Some(map) => lines.line(map.to_original(offset)),
      None => lines.line(offset),
    };
    self.disable_policy.run(
      diagnostics,
      &line_of,
      |collector| collector.scan_tree(tree, tree.syntax(), 0),
      file_path,
    )
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

  sort_diagnostics(&mut diagnostics);

  LintResult::new(file_path, source, diagnostics)
}

/// Put `diagnostics` in report order: by offset, then by rule name.
pub(crate) fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
  diagnostics.sort_by(|a, b| {
    a.span
      .offset
      .cmp(&b.span.offset)
      .then_with(|| a.rule_name.cmp(&b.rule_name))
  });
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
    caught.report(&format!("in rule \"{}\"", active.rule.name())),
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
      caught.report("while linting this file"),
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
  /// The file's statements, parsed once from its text and shared by every
  /// rule.
  tree: &'a PostcssTree<'a>,
  embedded: Option<&'a EmbeddedRoot>,
  /// What the rules reported.
  diagnostics: Vec<Diagnostic>,
  /// One internal-error problem per rule that panicked.  Kept apart from
  /// `diagnostics` so disables and severity overrides never touch them.
  failures: Vec<Diagnostic>,
}

impl<'a> RuleRun<'a> {
  /// Prepare a pass of `set` over one file, the text `tree` was parsed from.
  fn new(
    set: &'a RuleSet,
    file_path: &'a str,
    tree: &'a PostcssTree<'a>,
    embedded: Option<&'a EmbeddedRoot>,
  ) -> Self {
    Self {
      set,
      file_path,
      tree,
      embedded,
      diagnostics: Vec::new(),
      failures: Vec::new(),
    }
  }

  /// The context every rule is called with.
  fn context(&self) -> RuleContext<'a> {
    RuleContext {
      file_path: self.file_path,
      source: self.tree.source(),
      syntax: self.tree.syntax(),
      env: self.set.env,
      embedded: self.embedded,
    }
  }

  /// Keep what `active` returned, or record its panic.
  fn record(&mut self, active: &ActiveRule, outcome: Result<Vec<Diagnostic>, Caught>) {
    match outcome {
      Ok(found) => self.diagnostics.extend(found),
      Err(caught) => self.failures.push(internal_error(active, &caught)),
    }
  }

  /// Run every rule over `tree`, the tree of the file's text.
  ///
  /// With `timed`, a rule that is slow on the file is timed on its own.
  fn check(&mut self, tree: &PostcssTree, timed: bool) {
    let context = self.context();
    let set: &'a RuleSet = self.set;
    for active in &set.rules {
      let rule = active.rule;
      if self.embedded.is_some_and(|root| !root.allows(rule.name())) {
        continue;
      }
      let Some(check) = active.check.as_deref() else {
        continue;
      };
      let started = timed.then(Instant::now);
      let source = self.tree.source();
      let outcome = panic_guard::catch(|| {
        debug_panic(rule.name(), source);
        check.check(tree, &context)
      });
      self.record(active, outcome);
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
mod tests;
