use std::collections::HashMap;
use std::time::Instant;

use gale_css_parser::{CssNode, ParseResult, Syntax, parse};
use gale_diagnostics::{Diagnostic, LintResult, Severity, SourceLineIndex, Span};

use crate::disables::{self, DisableReports};
use crate::known_rules::{self, RuleSupport};
use crate::panic_guard::{self, Caught, ISSUES_URL};
use crate::pattern;
use crate::registry::RuleRegistry;
use crate::rule::{PerFileOptions, Rule, RuleContext, secondary_options_of};

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

/// The main lint runner that applies enabled rules to parsed CSS.
pub struct LintRunner {
  registry: RuleRegistry,
  enabled_rules: Vec<String>,
  /// Per-rule options from the config (keyed by rule name).
  rule_options: HashMap<String, serde_json::Value>,
  /// Per-rule severity overrides from the config (keyed by rule name).
  /// When a rule is listed here its diagnostics will use this severity
  /// instead of the rule's `default_severity()`.
  rule_severities: HashMap<String, Severity>,
  /// When `true`, report `stylelint-disable` comments that don't suppress
  /// any warnings (Stylelint's `reportNeedlessDisables`).
  report_needless_disables: bool,
  /// When `true`, ignore all `/* stylelint-disable */` comments — diagnostics
  /// are reported regardless of disable directives (Stylelint's `ignoreDisables`).
  ignore_disables: bool,
  /// Stylelint's `reportInvalidScopeDisables`.
  report_invalid_scope_disables: bool,
  /// Stylelint's `reportDescriptionlessDisables`.
  report_descriptionless_disables: bool,
  /// Stylelint's `reportUnscopedDisables`.
  report_unscoped_disables: bool,
  /// Rule names from the config (including plugin rules Gale doesn't
  /// implement).  Used to suppress false needless-disable reports for
  /// rules that are configured but not in Gale's registry.
  configured_rules: Vec<String>,
  /// Global default severity from the config (`defaultSeverity`).
  /// Applied to rules that don't have an explicit severity override.
  default_severity: Option<Severity>,
}

impl LintRunner {
  /// Create a new runner with the given registry and list of enabled rule names.
  pub fn new(registry: RuleRegistry, enabled_rules: Vec<String>) -> Self {
    Self {
      registry,
      enabled_rules,
      rule_options: HashMap::new(),
      rule_severities: HashMap::new(),
      report_needless_disables: false,
      ignore_disables: false,
      report_invalid_scope_disables: false,
      report_descriptionless_disables: false,
      report_unscoped_disables: false,
      configured_rules: Vec::new(),
      default_severity: None,
    }
  }

  /// Create a new runner with per-rule options.
  pub fn with_options(
    registry: RuleRegistry,
    enabled_rules: Vec<String>,
    rule_options: HashMap<String, serde_json::Value>,
  ) -> Self {
    Self {
      registry,
      enabled_rules,
      rule_options,
      rule_severities: HashMap::new(),
      report_needless_disables: false,
      ignore_disables: false,
      report_invalid_scope_disables: false,
      report_descriptionless_disables: false,
      report_unscoped_disables: false,
      configured_rules: Vec::new(),
      default_severity: None,
    }
  }

  /// Create a new runner with per-rule options and severity overrides.
  pub fn with_options_and_severities(
    registry: RuleRegistry,
    enabled_rules: Vec<String>,
    rule_options: HashMap<String, serde_json::Value>,
    rule_severities: HashMap<String, Severity>,
  ) -> Self {
    Self {
      registry,
      enabled_rules,
      rule_options,
      rule_severities,
      report_needless_disables: false,
      ignore_disables: false,
      report_invalid_scope_disables: false,
      report_descriptionless_disables: false,
      report_unscoped_disables: false,
      configured_rules: Vec::new(),
      default_severity: None,
    }
  }

  /// Enable or disable `reportNeedlessDisables` checking.
  pub fn set_report_needless_disables(&mut self, enabled: bool) {
    self.report_needless_disables = enabled;
  }

  /// Enable or disable `ignoreDisables` — when `true`, inline disable
  /// comments are ignored and all diagnostics are reported.
  pub fn set_ignore_disables(&mut self, enabled: bool) {
    self.ignore_disables = enabled;
  }

  /// Enable or disable `reportInvalidScopeDisables` — report disable
  /// comments that name a rule which is not configured.
  pub fn set_report_invalid_scope_disables(&mut self, enabled: bool) {
    self.report_invalid_scope_disables = enabled;
  }

  /// Enable or disable `reportDescriptionlessDisables` — report disable
  /// comments that carry no `-- description`.
  pub fn set_report_descriptionless_disables(&mut self, enabled: bool) {
    self.report_descriptionless_disables = enabled;
  }

  /// Enable or disable `reportUnscopedDisables` — report disable comments
  /// that name no rule.
  pub fn set_report_unscoped_disables(&mut self, enabled: bool) {
    self.report_unscoped_disables = enabled;
  }

  /// The disable-comment reports this runner emits, at the severity
  /// Stylelint would use (`defaultSeverity`, falling back to error).
  fn disable_reports(&self) -> DisableReports {
    DisableReports {
      needless: self.report_needless_disables,
      invalid_scope: self.report_invalid_scope_disables,
      descriptionless: self.report_descriptionless_disables,
      unscoped: self.report_unscoped_disables,
      severity: self.default_severity.unwrap_or(Severity::Error),
    }
  }

  /// Apply the file's configuration comments to `diagnostics`: drop the
  /// problems they disable and add the reports about the comments.
  ///
  /// The rules linted `parsed.source` (the converted SCSS, for Sass), whose
  /// offsets the source map takes back to `source` so that lines are those
  /// of the file the author wrote.  In a style sheet embedded in an
  /// HTML-like file the host applies the ranges of the whole document
  /// instead, so only the reports are added here.
  #[allow(clippy::too_many_arguments)]
  fn apply_disables(
    &self,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    parsed: &ParseResult,
    syntax: Syntax,
    file_path: &str,
    is_configured: &dyn Fn(&str) -> bool,
    forbids_disable: &dyn Fn(&str) -> bool,
  ) {
    let text = parsed.source.as_str();
    if !self.needs_disabled_ranges() || !disables::may_have_directives(text) {
      return;
    }
    let lines = SourceLineIndex::build(source);
    let line_of = |offset: usize| match &parsed.source_map {
      Some(map) => lines.line(map.to_original(offset)),
      None => lines.line(offset),
    };
    let mut collector = disables::Collector::new(&line_of);
    collector.scan(text, syntax, 0);
    let (ranges, _rejected) = collector.finish();
    let settings = disables::DisableSettings {
      suppress: !self.ignore_disables && !crate::embedded::in_embedded_root(),
      reports: self.disable_reports(),
      is_configured,
      forbids_disable,
      file_path,
    };
    disables::apply(diagnostics, &ranges, &line_of, &settings);
  }

  /// Whether the comment reports need the disabled ranges at all.
  fn needs_disabled_ranges(&self) -> bool {
    !self.ignore_disables
      || self.report_needless_disables
      || self.report_invalid_scope_disables
      || self.report_descriptionless_disables
      || self.report_unscoped_disables
  }

  /// Whether a rule was configured with `reportDisables: true`, looking at
  /// the per-file options first and the runner's own second.
  fn rule_forbids_disable(
    &self,
    extra_options: &HashMap<String, serde_json::Value>,
    name: &str,
  ) -> bool {
    let lookup = |n: &str| {
      extra_options
        .get(n)
        .or_else(|| self.rule_options.get(n))
        .and_then(secondary_options_of)
        .and_then(|s| s.get("reportDisables"))
        .and_then(|v| v.as_bool())
        == Some(true)
    };
    lookup(name) || crate::registry::resolve_deprecated_alias(name).is_some_and(lookup)
  }

  /// Whether a rule name counts as configured for `reportInvalidScopeDisables`.
  ///
  /// Both the config's own keys (which may include plugin rules Gale does
  /// not implement) and the enabled rules count, compared by canonical name.
  fn is_configured_rule(&self, extra_enabled: &[String], name: &str) -> bool {
    let canonical = |n: &str| -> String {
      crate::registry::resolve_deprecated_alias(n)
        .map(|s| s.to_string())
        .unwrap_or_else(|| n.to_string())
    };
    let wanted = canonical(name);
    self
      .configured_rules
      .iter()
      .chain(self.enabled_rules.iter())
      .chain(extra_enabled.iter())
      .any(|r| canonical(r) == wanted)
  }

  /// Set the list of rule names from the config (including plugin rules
  /// Gale doesn't implement).
  pub fn set_configured_rules(&mut self, rules: Vec<String>) {
    self.configured_rules = rules;
  }

  /// Set the global default severity (from config's `defaultSeverity`).
  pub fn set_default_severity(&mut self, severity: Option<Severity>) {
    self.default_severity = severity;
  }

  /// Check if a rule name is known to the registry.
  pub fn has_rule(&self, name: &str) -> bool {
    self.registry.get(name).is_some()
  }

  /// Return a reference to the underlying rule registry.
  pub fn registry(&self) -> &RuleRegistry {
    &self.registry
  }

  /// Parse and lint a CSS source string, returning all diagnostics.
  ///
  /// Never panics: a rule that crashes is reported as an internal-error
  /// problem on the file and the remaining rules still run.
  pub fn lint_source(&self, source: &str, file_path: &str, syntax: Syntax) -> LintResult {
    guard_file(source, file_path, || {
      self.lint_source_unguarded(source, file_path, syntax)
    })
  }

  /// [`Self::lint_source`] without the file-level panic guard.
  fn lint_source_unguarded(&self, source: &str, file_path: &str, syntax: Syntax) -> LintResult {
    let _options_memo = PerFileOptions::begin();
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

    // Collect enabled rules from the registry.
    let (active_rules, missing) = self.resolve_rules(&self.enabled_rules);
    let options: Vec<Option<&serde_json::Value>> = active_rules
      .iter()
      .map(|rule| self.rule_options.get(rule.name()))
      .collect();

    let mut run = RuleRun::new(
      &active_rules,
      options.clone(),
      options,
      file_path,
      text,
      syntax,
    );

    // Run document-level checks (check_root).
    let t1 = Instant::now();
    run.check_root(&nodes, debug);
    if debug {
      eprintln!(
        "[perf] check_root total: {:.3}s",
        t1.elapsed().as_secs_f64()
      );
    }

    // Walk each top-level node for per-node checks.
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

    // Set file_path and apply severity overrides on all diagnostics.
    let t3 = Instant::now();
    for diag in &mut diagnostics {
      if diag.file_path.is_empty() {
        diag.file_path = file_path.to_string();
      }
      apply_secondary_options(diag, self.rule_options.get(&diag.rule_name));
      // Apply config-specified severity overrides.
      if let Some(&sev) = self.rule_severities.get(&diag.rule_name) {
        diag.severity = sev;
      } else if let Some(default_sev) = self.default_severity {
        // If no explicit severity override for this rule, apply the
        // global defaultSeverity from the config.
        diag.severity = default_sev;
      }
    }
    if debug {
      eprintln!("[perf] set_file_path: {:.3}s", t3.elapsed().as_secs_f64());
    }

    // Filter diagnostics based on inline disable comments and optionally
    // report needless disable comments.  When `ignore_disables` is true,
    // skip filtering entirely so all diagnostics are reported.
    let t4 = Instant::now();
    self.apply_disables(
      &mut diagnostics,
      source,
      &parse_result,
      syntax,
      file_path,
      &|rule_name| self.is_configured_rule(&[], rule_name),
      &|rule_name| self.rule_forbids_disable(&HashMap::new(), rule_name),
    );
    if debug {
      eprintln!("[perf] disable-filter: {:.3}s", t4.elapsed().as_secs_f64());
    }

    // Rule crashes go in last so that disables, severity overrides and the
    // `message` option cannot hide or soften them.
    diagnostics.extend(failures);

    let t5 = Instant::now();
    let unknown = self.unknown_rule_problems(&missing, source, file_path);
    let mut result = finish(file_path, source, &parse_result, diagnostics, unknown);
    result.invalid_option_warnings = invalid_options;
    if debug {
      eprintln!("[perf] sort: {:.3}s", t5.elapsed().as_secs_f64());
      eprintln!("[perf] total diagnostics: {}", result.diagnostics.len());
    }
    result
  }

  /// Parse and lint a CSS source string using a custom set of enabled rule
  /// names instead of the runner's default list.  Used when config overrides
  /// change the effective rules for a specific file.
  ///
  /// Never panics, for the same reason as [`Self::lint_source`].
  pub fn lint_source_with_rules(
    &self,
    source: &str,
    file_path: &str,
    syntax: Syntax,
    enabled_rules: &[String],
    rule_options: &HashMap<String, serde_json::Value>,
    rule_severities: &HashMap<String, Severity>,
  ) -> LintResult {
    guard_file(source, file_path, || {
      self.lint_source_with_rules_unguarded(
        source,
        file_path,
        syntax,
        enabled_rules,
        rule_options,
        rule_severities,
      )
    })
  }

  /// [`Self::lint_source_with_rules`] without the file-level panic guard.
  fn lint_source_with_rules_unguarded(
    &self,
    source: &str,
    file_path: &str,
    syntax: Syntax,
    enabled_rules: &[String],
    rule_options: &HashMap<String, serde_json::Value>,
    rule_severities: &HashMap<String, Severity>,
  ) -> LintResult {
    let _options_memo = PerFileOptions::begin();
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

    let (active_rules, missing) = self.resolve_rules(enabled_rules);

    // `check_root` sees the per-file options, falling back to the runner's
    // own.  The node walk sees the per-file options alone when there are
    // any, and the runner's otherwise.
    let root_options: Vec<Option<&serde_json::Value>> = active_rules
      .iter()
      .map(|rule| {
        rule_options
          .get(rule.name())
          .or_else(|| self.rule_options.get(rule.name()))
      })
      .collect();
    let merged_options = if rule_options.is_empty() {
      &self.rule_options
    } else {
      rule_options
    };
    let node_options: Vec<Option<&serde_json::Value>> = active_rules
      .iter()
      .map(|rule| merged_options.get(rule.name()))
      .collect();

    let mut run = RuleRun::new(
      &active_rules,
      root_options,
      node_options,
      file_path,
      text,
      syntax,
    );

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
      mut failures,
      invalid_options,
      ..
    } = run;

    // Build alias map: canonical rule name -> config-specified name.
    // When the config uses a deprecated name (e.g. "function-comma-space-after"),
    // the registry resolves it to the canonical name (e.g. "@stylistic/function-comma-space-after").
    // We need to relabel diagnostics back to the config name for output compatibility.
    let alias_map: HashMap<String, String> = enabled_rules
      .iter()
      .filter_map(|config_name| {
        let rule = self.registry.get(config_name)?;
        let canonical = rule.name();
        if canonical != config_name.as_str() {
          Some((canonical.to_string(), config_name.clone()))
        } else {
          None
        }
      })
      .collect();

    for diag in &mut diagnostics {
      if diag.file_path.is_empty() {
        diag.file_path = file_path.to_string();
      }
      apply_secondary_options(
        diag,
        rule_options
          .get(&diag.rule_name)
          .or_else(|| self.rule_options.get(&diag.rule_name)),
      );
      // Apply config-specified severity overrides BEFORE relabeling,
      // because severities are keyed by canonical rule name.
      if let Some(&sev) = rule_severities
        .get(&diag.rule_name)
        .or_else(|| self.rule_severities.get(&diag.rule_name))
      {
        diag.severity = sev;
      } else if let Some(default_sev) = self.default_severity {
        diag.severity = default_sev;
      }
      // Relabel rule name from canonical to config-specified alias.
      if let Some(config_name) = alias_map.get(&diag.rule_name) {
        diag.rule_name = config_name.clone();
      }
    }

    self.apply_disables(
      &mut diagnostics,
      source,
      &parse_result,
      syntax,
      file_path,
      &|rule_name| self.is_configured_rule(enabled_rules, rule_name),
      &|rule_name| self.rule_forbids_disable(rule_options, rule_name),
    );

    // Rule crashes go in last, as in `lint_source`, under the rule name the
    // config used.
    for failure in &mut failures {
      if let Some(config_name) = alias_map.get(&failure.rule_name) {
        failure.rule_name = config_name.clone();
      }
    }
    diagnostics.extend(failures);

    let unknown = self.unknown_rule_problems(&missing, source, file_path);
    let mut result = finish(file_path, source, &parse_result, diagnostics, unknown);
    result.invalid_option_warnings = invalid_options;
    result
  }

  /// The registered rules behind `names`, and the names the registry does
  /// not know.
  fn resolve_rules<'r>(&'r self, names: &'r [String]) -> (Vec<&'r dyn Rule>, Vec<&'r str>) {
    let mut active = Vec::with_capacity(names.len());
    let mut missing = Vec::new();
    for name in names {
      match self.registry.get(name) {
        Some(rule) => active.push(rule),
        None => missing.push(name.as_str()),
      }
    }
    (active, missing)
  }

  /// Stylelint's report for every enabled name that is no rule at all:
  /// `Unknown rule <name>.` as an error at the very start of the file, like
  /// Stylelint's own (which disables, severities and the `message` option do
  /// not touch either).
  ///
  /// `missing` holds the enabled names the registry does not know.  Those
  /// that are real Stylelint or plugin rules gale has not implemented, or
  /// rules Stylelint has removed, are skipped silently here; the CLI warns
  /// about them once per run.
  fn unknown_rule_problems(
    &self,
    missing: &[&str],
    source: &str,
    file_path: &str,
  ) -> Vec<Diagnostic> {
    let first_char = source.chars().next().map_or(1, char::len_utf8);
    missing
      .iter()
      .filter(|name| known_rules::classify(&self.registry, name) == RuleSupport::Unknown)
      .map(|name| {
        Diagnostic::new(*name, known_rules::unknown_rule_message(name))
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
/// in the order `enabled_rules` happens to hold them, which comes from a
/// HashMap and so varies between processes.  Without a tiebreaker two
/// warnings at the same offset swap places between runs.
///
/// `unknown_rules` (from [`LintRunner::unknown_rule_problems`]) already point
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

/// The problem reported in place of a rule that panicked.
///
/// It is an error whatever the rule's configured severity, so the run exits
/// non-zero, and it carries the panic message and location so the report is
/// enough to file a bug.
fn internal_error(rule_name: &str, caught: &Caught, offset: usize) -> Diagnostic {
  Diagnostic::new(
    rule_name,
    format!(
      "Internal error in rule \"{rule_name}\": {}. This is a bug in gale; please report it at {ISSUES_URL}",
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

/// One file's pass over its active rules.
///
/// Every `check_root` and `check` call runs inside [`panic_guard::catch`].  A
/// rule that panics is reported once, as an internal error at the node it was
/// checking, and skipped for the rest of the file; the other rules carry on.
struct RuleRun<'a> {
  rules: &'a [&'a dyn Rule],
  /// The options each rule sees in `check_root`, by index into `rules`.
  root_options: Vec<Option<&'a serde_json::Value>>,
  /// The options each rule sees in `check`, by index into `rules`.
  node_options: Vec<Option<&'a serde_json::Value>>,
  file_path: &'a str,
  source: &'a str,
  syntax: Syntax,
  /// `failed[i]` once `rules[i]` has panicked on this file.
  failed: Vec<bool>,
  /// What the rules reported.
  diagnostics: Vec<Diagnostic>,
  /// One internal-error problem per rule that panicked.  Kept apart from
  /// `diagnostics` so disables and severity overrides never touch them.
  failures: Vec<Diagnostic>,
  /// Texts of the invalid-option reports the rules returned, each once.
  invalid_options: Vec<String>,
}

impl<'a> RuleRun<'a> {
  /// Prepare a pass of `rules` over one file.
  fn new(
    rules: &'a [&'a dyn Rule],
    root_options: Vec<Option<&'a serde_json::Value>>,
    node_options: Vec<Option<&'a serde_json::Value>>,
    file_path: &'a str,
    source: &'a str,
    syntax: Syntax,
  ) -> Self {
    let mut run = Self {
      rules,
      root_options,
      node_options,
      file_path,
      source,
      syntax,
      failed: vec![false; rules.len()],
      diagnostics: Vec::new(),
      failures: Vec::new(),
      invalid_options: Vec::new(),
    };
    run.validate_patterns();
    run
  }

  /// Report every regex literal in the rules' options that does not
  /// compile, so a broken `ignore*: ["/[a-z/"]` entry is an invalid option
  /// rather than an entry that silently matches nothing.
  fn validate_patterns(&mut self) {
    for (index, rule) in self.rules.iter().enumerate() {
      let options = [self.root_options[index], self.node_options[index]];
      for value in options.into_iter().flatten() {
        for invalid in pattern::invalid_options(rule.name(), value) {
          if !self.invalid_options.contains(&invalid.message) {
            self.invalid_options.push(invalid.message);
          }
        }
      }
    }
  }

  /// The context a rule's options are read through for one call.
  fn context(&self, options: Option<&'a serde_json::Value>) -> RuleContext<'a> {
    RuleContext {
      file_path: self.file_path,
      source: self.source,
      syntax: self.syntax,
      options,
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
          .push(internal_error(self.rules[index].name(), &caught, offset));
      }
    }
  }

  /// Run every rule's document-level `check_root`.
  fn check_root(&mut self, nodes: &[CssNode], debug: bool) {
    for index in 0..self.rules.len() {
      let rule = self.rules[index];
      let context = self.context(self.root_options[index]);
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
    for index in 0..self.rules.len() {
      if self.failed[index] {
        continue;
      }
      let rule = self.rules[index];
      let context = self.context(self.node_options[index]);
      let outcome = panic_guard::catch(|| rule.check(&node, &context));
      self.record(index, outcome, offset);
    }

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

  #[test]
  fn lint_empty_block() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let result = runner.lint_source("a { }", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].rule_name, "block-no-empty");
    assert_eq!(result.diagnostics[0].message, "Unexpected empty block");
  }

  #[test]
  fn lint_non_empty_block() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let result = runner.lint_source("a { color: red; }", "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn disabled_rule_not_run() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec![]);
    let result = runner.lint_source("a { }", "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  // -- Invalid pattern options --

  #[test]
  fn a_lookahead_pattern_works() {
    let runner = runner_with_options(
      "selector-class-pattern",
      serde_json::json!("^(?!js-)[a-z-]+$"),
    );
    let result = runner.lint_source(".card {}\n.js-card {}\n", "test.css", Syntax::Css);
    assert!(result.invalid_option_warnings.is_empty());
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(result.diagnostics[0].message.contains("js-card"));
  }

  #[test]
  fn a_pattern_that_does_not_compile_is_an_invalid_option_once_per_file() {
    let runner = runner_with_options("selector-class-pattern", serde_json::json!("^[a-z"));
    let result = runner.lint_source(".a {}\n.b {}\n", "test.css", Syntax::Css);
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
    let runner = runner_with_options(
      "color-named",
      serde_json::json!(["never", { "ignoreProperties": ["/[broken/"] }]),
    );
    let result = runner.lint_source("a { color: red; }\n", "test.css", Syntax::Css);
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
    let runner = runner_for(&["block-no-emty", "block-no-empty"]);
    let result = runner.lint_source("a {}\n", "test.css", Syntax::Css);
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
    let runner = runner_for(&["no-unknown-custom-properties", "acme/no-foo"]);
    let result = runner.lint_source("a {}\n", "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
  }

  #[test]
  fn unknown_rules_ignore_disables_and_default_severity() {
    let mut runner = runner_for(&["not-a-rule"]);
    runner.set_default_severity(Some(Severity::Warning));
    let result = runner.lint_source_with_rules(
      "/* stylelint-disable */\na {}\n",
      "test.css",
      Syntax::Css,
      &["not-a-rule".to_string()],
      &HashMap::new(),
      &HashMap::new(),
    );
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
    let runner = LintRunner::new(
      registry_with_panicking_rule(),
      vec!["test/panics".to_string(), "block-no-empty".to_string()],
    );
    let result = runner.lint_source("a {}\nb {}\n", "test.css", Syntax::Css);

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
    let mut severities = HashMap::new();
    severities.insert("test/panics".to_string(), Severity::Warning);
    let mut options = HashMap::new();
    options.insert(
      "test/panics".to_string(),
      serde_json::json!([true, { "message": "custom" }]),
    );
    let runner = LintRunner::with_options_and_severities(
      registry_with_panicking_rule(),
      vec!["test/panics".to_string()],
      options,
      severities,
    );
    let src = "/* stylelint-disable */\na { color: red; }\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert_eq!(result.diagnostics[0].severity, Severity::Error);
    assert!(result.diagnostics[0].message.starts_with("Internal error"));
  }

  #[test]
  fn per_file_rule_sets_are_guarded_too() {
    let runner = LintRunner::new(registry_with_panicking_rule(), vec![]);
    let result = runner.lint_source_with_rules(
      "a { color: red; }",
      "test.css",
      Syntax::Css,
      &["test/panics".to_string(), "color-named".to_string()],
      &HashMap::new(),
      &HashMap::new(),
    );
    let rules: Vec<&str> = result
      .diagnostics
      .iter()
      .map(|d| d.rule_name.as_str())
      .collect();
    assert!(rules.contains(&"test/panics"), "{rules:?}");
    assert!(rules.contains(&"color-named"), "{rules:?}");
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
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* gale-disable */\na { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn gale_disable_specific_rule() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* gale-disable block-no-empty */\na { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn gale_disable_wrong_rule_still_reports() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* gale-disable some-other-rule */\na { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn gale_disable_enable_block() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* gale-disable */\na { }\n/* gale-enable */\nb { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    // First `a { }` is disabled, second `b { }` should be reported.
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn gale_disable_next_line() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* gale-disable-next-line */\na { }\nb { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    // Only `a { }` is disabled; `b { }` still reported.
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn gale_disable_next_line_specific_rule() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* gale-disable-next-line block-no-empty */\na { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn stylelint_disable_compat() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* stylelint-disable */\na { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn stylelint_disable_next_line_compat() {
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = "/* stylelint-disable-next-line */\na { }\nb { }";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
  }

  /// The lines `runner` reports problems on in `src`.
  fn problem_lines(runner: &LintRunner, src: &str, path: &str, syntax: Syntax) -> Vec<usize> {
    let result = runner.lint_source(src, path, syntax);
    let index = SourceLineIndex::build(src);
    result
      .diagnostics
      .iter()
      .map(|d| index.line(d.span.offset))
      .collect()
  }

  #[test]
  fn disables_cover_whole_lines_like_stylelint() {
    let runner = runner_for(&["block-no-empty"]);
    let lines = |src: &str| problem_lines(&runner, src, "test.css", Syntax::Css);
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
    let runner = runner_for(&["color-no-invalid-hex"]);
    let src = "/* stylelint-disable color-no-invalid-hex */\na { color: #ab; }\n/* stylelint-enable */\na { color: #ab; }\n";
    assert_eq!(
      problem_lines(&runner, src, "test.css", Syntax::Css),
      vec![4]
    );
    // And enabling one rule under a blanket disable turns on just that one.
    let runner = runner_for(&["color-no-invalid-hex", "block-no-empty"]);
    let src = "/* stylelint-disable */\na {}\n/* stylelint-enable color-no-invalid-hex */\nb { color: #ab; }\nc {}\n";
    assert_eq!(
      problem_lines(&runner, src, "test.css", Syntax::Css),
      vec![4]
    );
  }

  #[test]
  fn double_slash_commands_need_scss_or_less() {
    let runner = runner_for(&["block-no-empty"]);
    let src = "// stylelint-disable-next-line block-no-empty\na {}\nb {}\n";
    assert_eq!(problem_lines(&runner, src, "t.scss", Syntax::Scss), vec![3]);
    assert_eq!(problem_lines(&runner, src, "t.less", Syntax::Less), vec![3]);
    // In CSS the line is part of the next selector, not a comment.
    let hex = runner_for(&["color-no-invalid-hex"]);
    let css = "a { color: #ab; } // stylelint-disable-line color-no-invalid-hex\nb {}\n";
    assert_eq!(problem_lines(&hex, css, "t.css", Syntax::Css), vec![1]);
    assert!(problem_lines(&hex, css, "t.scss", Syntax::Scss).is_empty());
    // A description on the next `//` line makes the command span both.
    let described = "// stylelint-disable-next-line block-no-empty\n// -- legacy\na {}\nb {}\n";
    assert_eq!(
      problem_lines(&runner, described, "t.scss", Syntax::Scss),
      vec![4]
    );
  }

  #[test]
  fn sass_disables_count_the_lines_of_the_sass() {
    // The converted SCSS gains a `}` line before `color`, which must not
    // push the problem off the line the command names.
    let mut options = HashMap::new();
    options.insert("color-hex-case".to_string(), serde_json::json!("lower"));
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec!["color-hex-case".to_string()],
      options,
    );
    let src =
      ".a\n  .b\n    // stylelint-disable-next-line color-hex-case\n  color: #FFF\n  top: #FFF\n";
    assert_eq!(
      problem_lines(&runner, src, "test.sass", Syntax::Sass),
      vec![5]
    );
  }

  #[test]
  fn sass_parses_successfully() {
    // Sass indented syntax is now converted to SCSS before parsing,
    // so it should produce lint results, not parse-error diagnostics.
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = ".foo\n  color: red";
    let result = runner.lint_source(src, "test.sass", Syntax::Sass);

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
  fn sass_parses_successfully_with_rules() {
    // Same check through lint_source_with_rules path.
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec![]);
    let src = ".foo\n  color: red";
    let result = runner.lint_source_with_rules(
      src,
      "test.sass",
      Syntax::Sass,
      &["block-no-empty".to_string()],
      &HashMap::new(),
      &HashMap::new(),
    );

    let has_parse_error = result
      .diagnostics
      .iter()
      .any(|d| d.rule_name == "parse-error");
    assert!(
      !has_parse_error,
      "Sass should now parse successfully via with_rules, but got parse-error diagnostic"
    );
  }

  #[test]
  fn sass_spans_point_into_the_original_file() {
    // Multibyte text before the problem used to push spans (computed on the
    // converted SCSS) into the middle of characters of the Sass source.
    let src =
      "// héllo wörld ünïcödé\n.foo\n  content: \"→ ✓ ★\"\n  color: red\n\n.baz\n  color: #FFF\n";
    let mut options = HashMap::new();
    options.insert("color-hex-case".to_string(), serde_json::json!("lower"));
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec!["color-hex-case".to_string()],
      options,
    );
    let result = runner.lint_source(src, "test.sass", Syntax::Sass);

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
    let registry = RuleRegistry::default();
    let all: Vec<String> = registry
      .all()
      .iter()
      .map(|r| r.name().to_string())
      .collect();
    let runner = LintRunner::new(registry, all);
    let result = runner.lint_source(src, "test.sass", Syntax::Sass);
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
    let registry = RuleRegistry::default();
    let runner = LintRunner::new(registry, vec!["block-no-empty".to_string()]);
    let src = ".foo { content: \"unclosed; }";
    let result = runner.lint_source(src, "test.scss", Syntax::Scss);

    // Verify file_path is set regardless of parse outcome.
    assert_eq!(result.file_path, "test.scss");
    // The result should not be empty — either rules detected violations
    // from the fallback parse, or a parse-error diagnostic was emitted.
    // (If both parsers happen to recover and produce a clean AST with no
    // violations, that is also acceptable — but the code path is correct.)
  }

  // -- Secondary options every rule accepts --

  fn runner_with_options(rule: &str, options: serde_json::Value) -> LintRunner {
    let mut opts = HashMap::new();
    opts.insert(rule.to_string(), options);
    LintRunner::with_options(RuleRegistry::default(), vec![rule.to_string()], opts)
  }

  #[test]
  fn message_option_replaces_the_warning_text() {
    let runner = runner_with_options(
      "block-no-empty",
      serde_json::json!([true, { "message": "No empty blocks please" }]),
    );
    let result = runner.lint_source("a {}", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].message, "No empty blocks please");
    assert_eq!(result.diagnostics[0].rule_name, "block-no-empty");
  }

  #[test]
  fn url_option_is_attached_to_every_warning_of_the_rule() {
    let runner = runner_with_options(
      "block-no-empty",
      serde_json::json!([true, { "url": "https://example.com/empty" }]),
    );
    let result = runner.lint_source("a {}\nb {}", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 2);
    for d in &result.diagnostics {
      assert_eq!(d.url.as_deref(), Some("https://example.com/empty"));
    }
  }

  #[test]
  fn disable_fix_option_keeps_the_report_but_drops_the_fix() {
    let fixable = runner_with_options("color-hex-case", serde_json::json!(["lower"]));
    let with_fix = fixable.lint_source("a { color: #FFF; }", "test.css", Syntax::Css);
    assert!(
      with_fix.diagnostics[0].fix.is_some(),
      "precondition: rule is fixable"
    );

    let runner = runner_with_options(
      "color-hex-case",
      serde_json::json!(["lower", { "disableFix": true }]),
    );
    let result = runner.lint_source("a { color: #FFF; }", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert!(result.diagnostics[0].fix.is_none());
  }

  #[test]
  fn secondary_options_apply_through_per_file_rule_sets() {
    let runner = LintRunner::new(RuleRegistry::default(), vec![]);
    let enabled = vec!["block-no-empty".to_string()];
    let mut opts = HashMap::new();
    opts.insert(
      "block-no-empty".to_string(),
      serde_json::json!([true, { "message": "Custom", "url": "https://x.y" }]),
    );
    let result = runner.lint_source_with_rules(
      "a {}",
      "test.css",
      Syntax::Css,
      &enabled,
      &opts,
      &HashMap::new(),
    );
    assert_eq!(result.diagnostics[0].message, "Custom");
    assert_eq!(result.diagnostics[0].url.as_deref(), Some("https://x.y"));
  }

  // -- Disable-comment reports --

  fn runner_for(rules: &[&str]) -> LintRunner {
    LintRunner::new(
      RuleRegistry::default(),
      rules.iter().map(|r| r.to_string()).collect(),
    )
  }

  #[test]
  fn invalid_scope_disable_is_reported_for_unconfigured_rule() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_invalid_scope_disables(true);
    let src = "/* stylelint-disable color-named */\na { color: red; }\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "--report-invalid-scope-disables");
    assert_eq!(d.message, "Rule \"color-named\" isn't enabled");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span.offset, 0);
  }

  #[test]
  fn invalid_scope_ignores_configured_and_blanket_disables() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_invalid_scope_disables(true);
    for src in [
      "/* stylelint-disable block-no-empty */\na { color: red; }\n",
      "/* stylelint-disable */\na { color: red; }\n",
    ] {
      let result = runner.lint_source(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn invalid_scope_counts_plugin_rules_from_the_config() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_invalid_scope_disables(true);
    runner.set_configured_rules(vec!["some-plugin/rule".to_string()]);
    let src = "/* stylelint-disable some-plugin/rule */\na { color: red; }\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty());
  }

  #[test]
  fn invalid_scope_reports_once_per_comment_and_rule() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_invalid_scope_disables(true);
    // An inline disable creates two ranges for the same comment.
    let src = "a { color: red; } /* stylelint-disable color-named */\nb { color: red; }\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
  }

  #[test]
  fn descriptionless_disable_is_reported_once_per_comment() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_descriptionless_disables(true);
    let src = "/* stylelint-disable block-no-empty, color-named */\na {}\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
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
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_descriptionless_disables(true);
    let src = "/* stylelint-disable */\na {}\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
      result.diagnostics[0].message,
      "Disable for \"all\" is missing a description"
    );
  }

  #[test]
  fn described_disables_are_not_reported() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_descriptionless_disables(true);
    for src in [
      "/* stylelint-disable block-no-empty -- legacy markup */\na {}\n",
      "/* stylelint-disable -- legacy markup */\na {}\n",
      "/* stylelint-disable-next-line block-no-empty -- why */\na {}\n",
    ] {
      let result = runner.lint_source(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn descriptionless_next_line_disable_is_reported() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_descriptionless_disables(true);
    let src = "/* stylelint-disable-next-line block-no-empty */\na {}\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
      result.diagnostics[0].rule_name,
      "--report-descriptionless-disables"
    );
  }

  #[test]
  fn unscoped_disable_is_reported_once_per_comment() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_unscoped_disables(true);
    // Inline placement creates two ranges for one comment.
    let src = "a { color: red; } /* stylelint-disable */\nb {}\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "--report-unscoped-disables");
    assert_eq!(d.message, "Configuration comment must be scoped");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span.offset, 18);
  }

  #[test]
  fn scoped_disables_are_not_unscoped() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_unscoped_disables(true);
    for src in [
      "/* stylelint-disable block-no-empty */\na {}\n",
      "/* stylelint-disable-next-line block-no-empty */\na {}\n",
    ] {
      let result = runner.lint_source(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn report_disables_option_flags_a_disable_of_that_rule() {
    let runner = runner_with_options(
      "block-no-empty",
      serde_json::json!([true, { "reportDisables": true }]),
    );
    for src in [
      "/* stylelint-disable block-no-empty */\na {}\n",
      "/* stylelint-disable-next-line block-no-empty */\na {}\n",
    ] {
      let result = runner.lint_source(src, "test.css", Syntax::Css);
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
    let runner = runner_with_options(
      "block-no-empty",
      serde_json::json!([true, { "reportDisables": true }]),
    );
    for src in [
      "/* stylelint-disable color-named */\na { color: red; }\n",
      "/* stylelint-disable */\na {}\n",
    ] {
      let result = runner.lint_source(src, "test.css", Syntax::Css);
      assert!(result.diagnostics.is_empty(), "{src}");
    }
  }

  #[test]
  fn disable_reports_use_the_default_severity() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_report_descriptionless_disables(true);
    runner.set_default_severity(Some(Severity::Warning));
    let src = "/* stylelint-disable block-no-empty */\na {}\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics[0].severity, Severity::Warning);
  }

  #[test]
  fn reports_still_run_under_ignore_disables() {
    let mut runner = runner_for(&["block-no-empty"]);
    runner.set_ignore_disables(true);
    runner.set_report_descriptionless_disables(true);
    let src = "/* stylelint-disable block-no-empty */\na {}\n";
    let result = runner.lint_source(src, "test.css", Syntax::Css);
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
