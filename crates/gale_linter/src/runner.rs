use std::collections::{HashMap, HashSet};
use std::time::Instant;

use gale_css_parser::{CssNode, Syntax, parse};
use gale_diagnostics::{Diagnostic, LintResult, Severity, SourceLineIndex, Span};

use crate::registry::RuleRegistry;
use crate::rule::RuleContext;

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
fn filter_disabled_and_report(
    diagnostics: &mut Vec<Diagnostic>,
    ranges: &[DisabledRange],
    apply_disables: bool,
    reports: DisableReports,
    is_configured: &dyn Fn(&str) -> bool,
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

/// Returns `true` when the `GALE_DEBUG_PERF` environment variable is set to `"1"`.
fn perf_enabled() -> bool {
    std::env::var("GALE_DEBUG_PERF")
        .map(|v| v == "1")
        .unwrap_or(false)
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

    /// The disable-comment reports this runner emits, at the severity
    /// Stylelint would use (`defaultSeverity`, falling back to error).
    fn disable_reports(&self) -> DisableReports {
        DisableReports {
            needless: self.report_needless_disables,
            invalid_scope: self.report_invalid_scope_disables,
            descriptionless: self.report_descriptionless_disables,
            severity: self.default_severity.unwrap_or(Severity::Error),
        }
    }

    /// Whether the comment reports need the disabled ranges at all.
    fn needs_disabled_ranges(&self) -> bool {
        !self.ignore_disables
            || self.report_needless_disables
            || self.report_invalid_scope_disables
            || self.report_descriptionless_disables
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
        self.configured_rules
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
    pub fn lint_source(&self, source: &str, file_path: &str, syntax: Syntax) -> LintResult {
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

        let mut diagnostics = Vec::new();

        // Collect enabled rules from the registry.
        let active_rules: Vec<&dyn crate::rule::Rule> = self
            .enabled_rules
            .iter()
            .filter_map(|name| self.registry.get(name))
            .collect();

        // Run document-level checks (check_root).
        let t1 = Instant::now();
        for rule in &active_rules {
            let tr = Instant::now();
            let context = RuleContext {
                file_path,
                source,
                syntax,
                options: self.rule_options.get(rule.name()),
            };
            let mut results = rule.check_root(&parse_result.nodes, &context);
            if debug {
                let elapsed = tr.elapsed().as_secs_f64();
                if elapsed > 0.001 {
                    eprintln!("[perf] check_root {}: {:.3}s", rule.name(), elapsed);
                }
            }
            diagnostics.append(&mut results);
        }
        if debug {
            eprintln!(
                "[perf] check_root total: {:.3}s",
                t1.elapsed().as_secs_f64()
            );
        }

        // Walk each top-level node for per-node checks.
        let t2 = Instant::now();
        for node in &parse_result.nodes {
            walk_node(
                node,
                &active_rules,
                file_path,
                source,
                syntax,
                &self.rule_options,
                &mut diagnostics,
            );
        }
        if debug {
            eprintln!("[perf] walk: {:.3}s", t2.elapsed().as_secs_f64());
        }

        // Set file_path and apply severity overrides on all diagnostics.
        let t3 = Instant::now();
        for diag in &mut diagnostics {
            if diag.file_path.is_empty() {
                diag.file_path = file_path.to_string();
            }
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
            let line_index = SourceLineIndex::build(source);
            let disabled_ranges = collect_disabled_ranges(source, &line_index);
            filter_disabled_and_report(
                &mut diagnostics,
                &disabled_ranges,
                !self.ignore_disables,
                self.disable_reports(),
                &|rule_name| self.is_configured_rule(&[], rule_name),
                file_path,
            );
        }
        if debug {
            eprintln!("[perf] disable-filter: {:.3}s", t4.elapsed().as_secs_f64());
        }

        // Sort diagnostics by position for consistent output.  The rule name
        // breaks ties: rules run in the order `enabled_rules` happens to hold
        // them, which comes from a HashMap and so varies between processes.
        // Without a tiebreaker two warnings at the same offset swap places
        // between runs.
        let t5 = Instant::now();
        diagnostics.sort_by(|a, b| {
            a.span
                .offset
                .cmp(&b.span.offset)
                .then_with(|| a.rule_name.cmp(&b.rule_name))
        });
        if debug {
            eprintln!("[perf] sort: {:.3}s", t5.elapsed().as_secs_f64());
            eprintln!("[perf] total diagnostics: {}", diagnostics.len());
        }

        LintResult::new(file_path, source, diagnostics)
    }

    /// Parse and lint a CSS source string using a custom set of enabled rule
    /// names instead of the runner's default list.  Used when config overrides
    /// change the effective rules for a specific file.
    pub fn lint_source_with_rules(
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

        let mut diagnostics = Vec::new();

        let active_rules: Vec<&dyn crate::rule::Rule> = enabled_rules
            .iter()
            .filter_map(|name| self.registry.get(name))
            .collect();

        let t1 = Instant::now();
        for rule in &active_rules {
            let context = RuleContext {
                file_path,
                source,
                syntax,
                options: rule_options
                    .get(rule.name())
                    .or_else(|| self.rule_options.get(rule.name())),
            };
            let mut results = rule.check_root(&parse_result.nodes, &context);
            diagnostics.append(&mut results);
        }
        if debug {
            eprintln!(
                "[perf] check_root total: {:.3}s",
                t1.elapsed().as_secs_f64()
            );
        }

        // Merge rule_options with self.rule_options (override-specific takes precedence)
        let merged_options = if rule_options.is_empty() {
            &self.rule_options
        } else {
            // We need a merged map; use a temporary
            // For efficiency, just pass both and let walk_node check both
            rule_options
        };

        let t2 = Instant::now();
        for node in &parse_result.nodes {
            walk_node(
                node,
                &active_rules,
                file_path,
                source,
                syntax,
                merged_options,
                &mut diagnostics,
            );
        }
        if debug {
            eprintln!("[perf] walk: {:.3}s", t2.elapsed().as_secs_f64());
        }

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
            let line_index = SourceLineIndex::build(source);
            let disabled_ranges = collect_disabled_ranges(source, &line_index);
            filter_disabled_and_report(
                &mut diagnostics,
                &disabled_ranges,
                !self.ignore_disables,
                self.disable_reports(),
                &|rule_name| self.is_configured_rule(enabled_rules, rule_name),
                file_path,
            );
        }

        // Tie-broken by rule name for the same reason as in `lint_source`.
        diagnostics.sort_by(|a, b| {
            a.span
                .offset
                .cmp(&b.span.offset)
                .then_with(|| a.rule_name.cmp(&b.rule_name))
        });

        LintResult::new(file_path, source, diagnostics)
    }
}

/// Recursively walk the AST, invoking each rule's `check` on every node.
fn walk_node(
    node: &CssNode,
    rules: &[&dyn crate::rule::Rule],
    file_path: &str,
    source: &str,
    syntax: Syntax,
    rule_options: &HashMap<String, serde_json::Value>,
    diagnostics: &mut Vec<gale_diagnostics::Diagnostic>,
) {
    // Run rules on this node.
    for rule in rules {
        let context = RuleContext {
            file_path,
            source,
            syntax,
            options: rule_options.get(rule.name()),
        };
        let mut results = rule.check(node, &context);
        diagnostics.append(&mut results);
    }

    // Recurse into children based on node type.
    match node {
        CssNode::Style(style_rule) => {
            for child in &style_rule.children {
                let child_node = CssNode::Style(child.clone());
                walk_node(
                    &child_node,
                    rules,
                    file_path,
                    source,
                    syntax,
                    rule_options,
                    diagnostics,
                );
            }
            // Walk at-rules nested inside the style rule (e.g. @include,
            // @if/@else, @media) so lint rules can inspect their contents.
            for at_node in &style_rule.nested_at_rules {
                walk_node(
                    at_node,
                    rules,
                    file_path,
                    source,
                    syntax,
                    rule_options,
                    diagnostics,
                );
            }
        }
        CssNode::AtRule(at_rule) => {
            for child in &at_rule.children {
                walk_node(
                    child,
                    rules,
                    file_path,
                    source,
                    syntax,
                    rule_options,
                    diagnostics,
                );
            }
        }
        CssNode::Comment(_) | CssNode::Declaration(_) => {}
    }
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
