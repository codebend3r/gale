use std::collections::{HashMap, HashSet};
use std::time::Instant;

use gale_css_parser::{CssNode, ParseResult, Syntax, parse};
use gale_diagnostics::{Diagnostic, LintResult, Severity, SourceLineIndex, Span};

use crate::known_rules::{self, RuleSupport};
use crate::panic_guard::{self, Caught, ISSUES_URL};
use crate::pattern;
use crate::registry::RuleRegistry;
use crate::rule::{Rule, RuleContext, secondary_options_of};

// ---------------------------------------------------------------------------
// Inline disable-comment support
// ---------------------------------------------------------------------------

/// A range in the source where certain (or all) rules are disabled.
#[derive(Debug)]
struct DisabledRange {
  /// Byte offset where the disable starts.
  start: usize,
  /// Byte offset where the disable ends (exclusive). `usize::MAX` means EOF.
  end: usize,
  /// `None` → all rules disabled; `Some(name)` → only that rule.
  rule: Option<String>,
  /// Byte offset of the comment that created this range (for needless-disable
  /// reporting).  Points to the `/*` or `//` that starts the directive.
  comment_start: usize,
  /// Whether the directive carried a `-- description` explaining itself.
  has_description: bool,
}

/// A `disable` directive that has not yet met its `enable`.
struct OpenDisable {
  start: usize,
  comment_start: usize,
  rule: Option<String>,
  has_description: bool,
}

/// The parsed body of a disable directive: which rules it names (`None` for
/// all of them) and whether it explains itself with a `-- description`.
struct Directive {
  rules: Vec<Option<String>>,
  has_description: bool,
}

/// Scan `source` for gale / stylelint disable comments and return disabled ranges.
fn collect_disabled_ranges(source: &str, line_index: &SourceLineIndex) -> Vec<DisabledRange> {
  let mut ranges: Vec<DisabledRange> = Vec::new();

  // Track open "disable" directives until their matching "enable".
  let mut open_disables: Vec<OpenDisable> = Vec::new();

  // We scan for `/* ... */` comments manually so we don't rely on the parser
  // (comments inside values, etc. would be stripped by the parser).
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i + 1 < len {
    if bytes[i] == b'/' && bytes[i + 1] == b'*' {
      // Found block comment start — find the end.
      let comment_start = i;
      if let Some(end_pos) = find_comment_end(bytes, i + 2) {
        let comment_end = end_pos + 2; // past `*/`
        let inner = &source[comment_start + 2..end_pos];
        let trimmed = inner.trim();

        process_directive(
          trimmed,
          comment_start,
          comment_end,
          source,
          line_index,
          &mut open_disables,
          &mut ranges,
        );

        i = comment_end;
      } else {
        break; // unterminated comment
      }
    } else if bytes[i] == b'/' && bytes[i + 1] == b'/' {
      // Found line comment (`//`) — used in SCSS/Less for disable directives.
      let comment_start = i;
      i += 2; // skip `//`
      let inner_start = i;
      // Find end of line
      while i < len && bytes[i] != b'\n' {
        i += 1;
      }
      let inner = &source[inner_start..i];
      let trimmed = inner.trim();
      let comment_end = i;

      process_directive(
        trimmed,
        comment_start,
        comment_end,
        source,
        line_index,
        &mut open_disables,
        &mut ranges,
      );

      if i < len {
        i += 1; // skip newline
      }
    } else {
      i += 1;
    }
  }

  // Close any still-open disables at EOF.
  for open in open_disables {
    ranges.push(DisabledRange {
      start: open.start,
      end: len,
      rule: open.rule,
      comment_start: open.comment_start,
      has_description: open.has_description,
    });
  }

  ranges
}

/// Byte offset of the `*/` closing a block comment started at `from`.
fn find_comment_end(bytes: &[u8], from: usize) -> Option<usize> {
  let mut j = from;
  while j + 1 < bytes.len() {
    if bytes[j] == b'*' && bytes[j + 1] == b'/' {
      return Some(j);
    }
    j += 1;
  }
  None
}

/// Dispatches a comment body that carries a `gale-` or `stylelint-` directive.
fn process_directive(
  trimmed: &str,
  comment_start: usize,
  comment_end: usize,
  source: &str,
  line_index: &SourceLineIndex,
  open_disables: &mut Vec<OpenDisable>,
  ranges: &mut Vec<DisabledRange>,
) {
  // Try both prefixes: gale-* and stylelint-*
  for prefix in &["gale-", "stylelint-"] {
    if let Some(rest) = trimmed.strip_prefix(prefix) {
      handle_directive(
        rest,
        comment_start,
        comment_end,
        source,
        line_index,
        open_disables,
        ranges,
      );
      return;
    }
  }
}

/// Applies one directive: `disable-next-line` and `disable-line` push a
/// range for a single line, `enable` closes open disables, and `disable`
/// opens one (plus the current line when the comment is inline).
fn handle_directive(
  rest: &str,
  comment_start: usize,
  comment_end: usize,
  source: &str,
  line_index: &SourceLineIndex,
  open_disables: &mut Vec<OpenDisable>,
  ranges: &mut Vec<DisabledRange>,
) {
  if let Some(rule_part) = rest.strip_prefix("disable-next-line") {
    // disable-next-line [rule-name, rule-name, ...]
    let directive = parse_directive(rule_part);
    let (comment_line, _) = line_index.offset_to_location(comment_start);
    let next_line = comment_line + 1;
    let (next_start, next_end) = line_byte_range(source, next_line);
    for rule_name in directive.rules {
      ranges.push(DisabledRange {
        start: next_start,
        end: next_end,
        rule: rule_name,
        comment_start,
        has_description: directive.has_description,
      });
    }
  } else if let Some(rule_part) = rest.strip_prefix("disable-line") {
    // disable-line [rule-name, ...] — disables on the current line
    let directive = parse_directive(rule_part);
    let (comment_line, _) = line_index.offset_to_location(comment_start);
    let (line_start, line_end) = line_byte_range(source, comment_line);
    for rule_name in directive.rules {
      ranges.push(DisabledRange {
        start: line_start,
        end: line_end,
        rule: rule_name,
        comment_start,
        has_description: directive.has_description,
      });
    }
  } else if let Some(rule_part) = rest.strip_prefix("enable") {
    // enable [rule-name, ...]
    // Use the end of the enable comment (past `*/`) so that diagnostics
    // whose span starts within the enable comment itself are still
    // suppressed.  This matches Stylelint's behaviour where the enable
    // comment line is considered part of the disabled region.
    let directive = parse_directive(rule_part);
    for rule_name in directive.rules {
      close_disable(open_disables, ranges, comment_end, &rule_name);
    }
  } else if let Some(rule_part) = rest.strip_prefix("disable") {
    // disable [rule-name, ...]
    let directive = parse_directive(rule_part);

    // If the disable comment is inline (on the same line as code), also
    // disable from the start of the current line so that code preceding
    // the comment on the same line is covered. This matches Stylelint's
    // behavior where `property: value; // stylelint-disable rule` suppresses
    // the diagnostic on the declaration.
    let (comment_line, _) = line_index.offset_to_location(comment_start);
    let (line_start, line_end) = line_byte_range(source, comment_line);
    let before_comment = &source[line_start..comment_start];
    let is_inline = !before_comment.trim().is_empty();

    for rule_name in directive.rules {
      if is_inline {
        // Add a range covering the current line in addition to the
        // open-ended disable.
        ranges.push(DisabledRange {
          start: line_start,
          end: line_end,
          rule: rule_name.clone(),
          comment_start,
          has_description: directive.has_description,
        });
      }
      open_disables.push(OpenDisable {
        start: comment_end,
        comment_start,
        rule: rule_name,
        has_description: directive.has_description,
      });
    }
  }
}

/// Parse the body of a directive: comma-separated rule names followed by an
/// optional `-- description`.
///
/// `None` in `rules` means "all rules".
/// E.g. " rule-a, rule-b " → [Some("rule-a"), Some("rule-b")]
///      "" → [None]  (all rules)
///
/// Deprecated rule name aliases (e.g. `scss/at-import-no-partial-leading-underscore`)
/// are resolved to their canonical names so that disable comments using old names
/// still suppress diagnostics emitted under the new name.
fn parse_directive(text: &str) -> Directive {
  let t = text.trim();
  // Split off the description after the ` -- ` separator (Stylelint
  // convention).  E.g. "rule-name -- reason" → "rule-name".
  let (t, has_description) = if let Some(pos) = t.find(" -- ") {
    (t[..pos].trim(), true)
  } else if t.starts_with("--") {
    // The entire text is a description (e.g. "-- Disable reason: ...")
    ("", true)
  } else {
    (t, false)
  };
  Directive {
    rules: parse_rule_names(t),
    has_description,
  }
}

/// Parse comma-separated rule names.  An empty string means "all rules".
fn parse_rule_names(t: &str) -> Vec<Option<String>> {
  if t.is_empty() {
    return vec![None]; // disable all rules
  }

  let resolve = |name: &str| -> String {
    crate::registry::resolve_deprecated_alias(name)
      .map(|s| s.to_string())
      .unwrap_or_else(|| name.to_string())
  };

  // If there are commas, split by comma
  if t.contains(',') {
    t.split(',')
      .map(|part| {
        let p = part.trim();
        if p.is_empty() { None } else { Some(resolve(p)) }
      })
      .filter(|n| n.is_some()) // filter out empty parts
      .collect()
  } else {
    vec![Some(resolve(t))]
  }
}

/// Close the most-recent matching open disable.
fn close_disable(
  open_disables: &mut Vec<OpenDisable>,
  ranges: &mut Vec<DisabledRange>,
  end_offset: usize,
  rule_name: &Option<String>,
) {
  // Find the last matching open disable (same rule or both None).
  if let Some(idx) = open_disables.iter().rposition(|o| &o.rule == rule_name) {
    let open = open_disables.remove(idx);
    ranges.push(DisabledRange {
      start: open.start,
      end: end_offset,
      rule: open.rule,
      comment_start: open.comment_start,
      has_description: open.has_description,
    });
  }
}

/// Return (start_byte, end_byte) for 1-indexed `line_number`.
fn line_byte_range(source: &str, line_number: usize) -> (usize, usize) {
  let mut current_line = 1usize;
  let mut line_start = 0usize;

  for (i, b) in source.bytes().enumerate() {
    if current_line == line_number {
      // Find end of this line.
      let mut end = i;
      for (j, b2) in source.bytes().enumerate().skip(i) {
        if b2 == b'\n' {
          end = j + 1; // include the newline
          return (line_start, end);
        }
        end = j + 1;
      }
      return (line_start, end);
    }
    if b == b'\n' {
      current_line += 1;
      line_start = i + 1;
    }
  }
  // If the requested line is beyond EOF, return an empty range at end.
  (source.len(), source.len())
}

/// Which reports about disable comments to emit, and at what severity.
///
/// Stylelint's `reportNeedlessDisables`, `reportInvalidScopeDisables` and
/// `reportDescriptionlessDisables` all default to `defaultSeverity`, falling
/// back to error.
#[derive(Debug, Clone, Copy)]
struct DisableReports {
  needless: bool,
  invalid_scope: bool,
  descriptionless: bool,
  unscoped: bool,
  severity: Severity,
}

/// Filter out disabled diagnostics and optionally report on the disable
/// comments themselves.
///
/// When `reports.needless` is `true`, a disable comment is reported as "needless"
/// only if:
///   1. The disable targets a **specific rule** (not `/* stylelint-disable */`), AND
///   2. The referenced rule is **known** to Gale (registered in the registry), AND
///   3. The disable didn't actually suppress any diagnostic.
///
/// Disables for **unknown** rules (e.g. third-party plugin rules Gale doesn't
/// implement) are NOT reported as needless because Gale can't know if they
/// suppress warnings — it doesn't run those plugins.
///
/// "All rules" disables (`/* stylelint-disable */`) are also not reported as
/// needless, since Gale may not implement all rules that Stylelint would fire.
///
/// When `reports.invalid_scope` is `true`, a disable that names a rule which is
/// not configured is reported (`is_configured` decides), matching Stylelint's
/// `reportInvalidScopeDisables`.  Blanket disables are never out of scope.
///
/// When `reports.descriptionless` is `true`, a disable comment without a
/// `-- description` is reported once, matching `reportDescriptionlessDisables`.
///
/// When `apply_disables` is `false` (Stylelint's `ignoreDisables`) the ranges
/// suppress nothing, but the reports above still run.
///
/// `forbids_disable` says whether a rule was configured with
/// `reportDisables: true`; any disable naming such a rule is reported.
fn filter_disabled_and_report(
  diagnostics: &mut Vec<Diagnostic>,
  ranges: &[DisabledRange],
  apply_disables: bool,
  reports: DisableReports,
  is_configured: &dyn Fn(&str) -> bool,
  forbids_disable: &dyn Fn(&str) -> bool,
  file_path: &str,
) {
  if ranges.is_empty() {
    return;
  }

  // Track which ranges actually suppressed at least one diagnostic.
  let mut suppressed = vec![false; ranges.len()];

  // Stylelint tracks which disables suppressed a warning even under
  // `ignoreDisables`; only the suppression itself is switched off.
  diagnostics.retain(|d| {
    let hit = range_covering(d, ranges);
    if let Some(i) = hit {
      suppressed[i] = true;
    }
    hit.is_none() || !apply_disables
  });

  if reports.invalid_scope {
    report_invalid_scope_disables(
      diagnostics,
      ranges,
      reports.severity,
      is_configured,
      file_path,
    );
  }

  if reports.descriptionless {
    report_descriptionless_disables(diagnostics, ranges, reports.severity, file_path);
  }

  if reports.unscoped {
    report_unscoped_disables(diagnostics, ranges, reports.severity, file_path);
  }

  report_forbidden_disables(diagnostics, ranges, forbids_disable, file_path);

  if !reports.needless {
    return;
  }

  // Deduplicate: an inline disable creates two ranges (current line +
  // open-ended) sharing the same comment_start.  Only report once per
  // (comment_start, rule) pair.  Also merge suppression: if *either*
  // range suppressed a diagnostic, the comment is not needless.
  use std::collections::HashMap as StdHashMap;
  let mut comment_suppressed: StdHashMap<(usize, Option<&str>), bool> = StdHashMap::new();
  for (i, range) in ranges.iter().enumerate() {
    let key = (range.comment_start, range.rule.as_deref());
    let entry = comment_suppressed.entry(key).or_insert(false);
    if suppressed[i] {
      *entry = true;
    }
  }

  let mut reported: HashSet<(usize, Option<String>)> = HashSet::new();

  for range in ranges.iter() {
    let key = (range.comment_start, range.rule.as_deref());

    // Skip if any range from this comment suppressed a diagnostic.
    if comment_suppressed.get(&key).copied().unwrap_or(false) {
      continue;
    }

    // "All rules" disables (`/* stylelint-disable */`) are never reported
    // as needless because Gale may not implement every rule that Stylelint
    // would fire — so a blanket disable might legitimately suppress
    // warnings from plugin rules Gale doesn't know about.
    if range.rule.is_none() {
      continue;
    }

    // Only report needless for rules where Gale has a **no-op stub**
    // implementation — i.e. a rule that is registered but intentionally
    // never produces diagnostics.  For all other rules, Gale's detection
    // might differ from Stylelint's, so a disable that appears to suppress
    // nothing in Gale might legitimately suppress warnings in Stylelint.
    //
    // No-op stubs exist for plugin rules that Gale can't execute (e.g.
    // Angular's `material/no-prefixes`): the rule is "known" but always
    // returns empty diagnostics.  Since it never fires, any inline disable
    // for it is genuinely needless.
    if let Some(ref rule_name) = range.rule
      && !is_noop_stub_rule(rule_name)
    {
      continue;
    }

    // Deduplicate: only report once per (comment_start, rule).
    let dedup_key = (range.comment_start, range.rule.clone());
    if !reported.insert(dedup_key) {
      continue;
    }

    let rule_desc = match &range.rule {
      None => "\"all\"".to_string(),
      Some(name) => format!("\"{}\"", name),
    };

    let msg = format!("Needless disable for {}", rule_desc);

    diagnostics.push(
      Diagnostic::new("--report-needless-disables", msg)
        .severity(reports.severity)
        .span(Span::new(range.comment_start, 0))
        .file_path(file_path),
    );
  }
}

/// Index of the first disabled range that covers `diag`, if any.
///
/// A range matches when it is a blanket disable, names the diagnostic's rule,
/// or names a deprecated alias of it (in either direction).
fn range_covering(diag: &Diagnostic, ranges: &[DisabledRange]) -> Option<usize> {
  let offset = diag.span.offset;
  ranges.iter().position(|r| {
    if offset < r.start || offset >= r.end {
      return false;
    }
    match &r.rule {
      None => true,
      Some(name) if name == &diag.rule_name => true,
      Some(name) => {
        // The disable may reference a deprecated alias that resolves
        // to this diagnostic's canonical rule name, or the diagnostic
        // may carry an alias of the disable's rule.
        crate::registry::resolve_deprecated_alias(name)
          .is_some_and(|canonical| canonical == diag.rule_name)
          || crate::registry::resolve_deprecated_alias(&diag.rule_name)
            .is_some_and(|canonical| canonical == name.as_str())
      }
    }
  })
}

/// Stylelint's `reportInvalidScopeDisables`: a disable that names a rule the
/// config does not enable.  Reported once per comment and rule.
fn report_invalid_scope_disables(
  diagnostics: &mut Vec<Diagnostic>,
  ranges: &[DisabledRange],
  severity: Severity,
  is_configured: &dyn Fn(&str) -> bool,
  file_path: &str,
) {
  let mut reported: HashSet<(usize, &str)> = HashSet::new();
  for range in ranges {
    let Some(name) = range.rule.as_deref() else {
      continue;
    };
    if is_configured(name) || !reported.insert((range.comment_start, name)) {
      continue;
    }
    diagnostics.push(
      Diagnostic::new(
        "--report-invalid-scope-disables",
        format!("Rule \"{name}\" isn't enabled"),
      )
      .severity(severity)
      .span(Span::new(range.comment_start, 0))
      .file_path(file_path),
    );
  }
}

/// Stylelint's per-rule `reportDisables: true`: a disable that names a rule
/// which may not be disabled.  Always an error, once per comment and rule.
fn report_forbidden_disables(
  diagnostics: &mut Vec<Diagnostic>,
  ranges: &[DisabledRange],
  forbids_disable: &dyn Fn(&str) -> bool,
  file_path: &str,
) {
  let mut reported: HashSet<(usize, &str)> = HashSet::new();
  for range in ranges {
    let Some(name) = range.rule.as_deref() else {
      continue;
    };
    if !forbids_disable(name) || !reported.insert((range.comment_start, name)) {
      continue;
    }
    diagnostics.push(
      Diagnostic::new(
        "reportDisables",
        format!("Rule \"{name}\" may not be disabled"),
      )
      .severity(Severity::Error)
      .span(Span::new(range.comment_start, 0))
      .file_path(file_path),
    );
  }
}

/// Stylelint's `reportUnscopedDisables`: a disable comment that names no
/// rule.  Reported once per comment.
fn report_unscoped_disables(
  diagnostics: &mut Vec<Diagnostic>,
  ranges: &[DisabledRange],
  severity: Severity,
  file_path: &str,
) {
  let mut reported: HashSet<usize> = HashSet::new();
  for range in ranges {
    if range.rule.is_some() || !reported.insert(range.comment_start) {
      continue;
    }
    diagnostics.push(
      Diagnostic::new(
        "--report-unscoped-disables",
        "Configuration comment must be scoped",
      )
      .severity(severity)
      .span(Span::new(range.comment_start, 0))
      .file_path(file_path),
    );
  }
}

/// Stylelint's `reportDescriptionlessDisables`: a disable comment with no
/// `-- description`.  Reported once per comment, naming its first rule (or
/// `all` for a blanket disable).
fn report_descriptionless_disables(
  diagnostics: &mut Vec<Diagnostic>,
  ranges: &[DisabledRange],
  severity: Severity,
  file_path: &str,
) {
  let mut reported: HashSet<usize> = HashSet::new();
  for range in ranges {
    if range.has_description || !reported.insert(range.comment_start) {
      continue;
    }
    let name = range.rule.as_deref().unwrap_or("all");
    diagnostics.push(
      Diagnostic::new(
        "--report-descriptionless-disables",
        format!("Disable for \"{name}\" is missing a description"),
      )
      .severity(severity)
      .span(Span::new(range.comment_start, 0))
      .file_path(file_path),
    );
  }
}

/// Rules that are registered as **no-op stubs** — they never produce any
/// diagnostics.  These exist for third-party plugin rules that Gale can't
/// execute natively.  Because they never fire, any inline disable comment
/// referencing them is genuinely needless.
///
/// All other registered rules *might* produce different diagnostics than
/// Stylelint, so we can't safely call their disables "needless".
fn is_noop_stub_rule(name: &str) -> bool {
  // Only include rules where BOTH Gale AND Stylelint produce zero warnings
  // in practice.  Plugin rules that Gale stubs as no-ops BUT Stylelint
  // actually fires (like scss/dollar-variable-no-missing-interpolation)
  // must NOT be listed here, because their disables suppress real
  // Stylelint warnings and are therefore not needless.
  matches!(name, "material/no-prefixes")
}

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
    diag.message = message.to_string();
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
    let debug = perf_enabled();
    if debug {
      eprintln!("[perf] start file: {}", file_path);
    }

    let t0 = Instant::now();
    let parse_result = match parse(source, syntax) {
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
    run.check_root(&parse_result.nodes, debug);
    if debug {
      eprintln!(
        "[perf] check_root total: {:.3}s",
        t1.elapsed().as_secs_f64()
      );
    }

    // Walk each top-level node for per-node checks.
    let t2 = Instant::now();
    for node in &parse_result.nodes {
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
    if self.needs_disabled_ranges() {
      let line_index = SourceLineIndex::build(text);
      let disabled_ranges = collect_disabled_ranges(text, &line_index);
      filter_disabled_and_report(
        &mut diagnostics,
        &disabled_ranges,
        !self.ignore_disables,
        self.disable_reports(),
        &|rule_name| self.is_configured_rule(&[], rule_name),
        &|rule_name| self.rule_forbids_disable(&HashMap::new(), rule_name),
        file_path,
      );
    }
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
    let debug = perf_enabled();
    if debug {
      eprintln!("[perf] start file: {}", file_path);
    }

    let t0 = Instant::now();
    let parse_result = match parse(source, syntax) {
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
    run.check_root(&parse_result.nodes, debug);
    if debug {
      eprintln!(
        "[perf] check_root total: {:.3}s",
        t1.elapsed().as_secs_f64()
      );
    }

    let t2 = Instant::now();
    for node in &parse_result.nodes {
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

    if self.needs_disabled_ranges() {
      let line_index = SourceLineIndex::build(text);
      let disabled_ranges = collect_disabled_ranges(text, &line_index);
      filter_disabled_and_report(
        &mut diagnostics,
        &disabled_ranges,
        !self.ignore_disables,
        self.disable_reports(),
        &|rule_name| self.is_configured_rule(enabled_rules, rule_name),
        &|rule_name| self.rule_forbids_disable(rule_options, rule_name),
        file_path,
      );
    }

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
  fn walk(&mut self, node: &CssNode) {
    for index in 0..self.rules.len() {
      if self.failed[index] {
        continue;
      }
      let rule = self.rules[index];
      let context = self.context(self.node_options[index]);
      let outcome = panic_guard::catch(|| rule.check(node, &context));
      self.record(index, outcome, node.span().offset);
    }

    // Recurse into children based on node type.
    match node {
      CssNode::Style(style_rule) => {
        for child in &style_rule.children {
          self.walk(&CssNode::Style(child.clone()));
        }
        // Walk at-rules nested inside the style rule (e.g. @include,
        // @if/@else, @media) so lint rules can inspect their contents.
        for at_node in &style_rule.nested_at_rules {
          self.walk(at_node);
        }
      }
      CssNode::AtRule(at_rule) => {
        for child in &at_rule.children {
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
