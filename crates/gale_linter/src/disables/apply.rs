//! Applying the disabled ranges to a file's problems, and Stylelint's
//! reports about the configuration comments themselves.

use std::collections::{HashMap, HashSet};

use gale_diagnostics::{
  DESCRIPTIONLESS_DISABLES, Diagnostic, INVALID_SCOPE_DISABLES, NEEDLESS_DISABLES, REPORT_DISABLES,
  Severity, UNSCOPED_DISABLES,
};

use super::{DisablePolicy, DisabledRanges, same_rule};

/// Drop the problems in `diagnostics` that `ranges` disable (unless
/// `policy` ignores disables), and add the reports about the comments that
/// `policy` asks for, filed under `file_path`.
///
/// `line_of` gives the line of a problem's offset, in the coordinates the
/// ranges were collected in.
pub(super) fn apply(
  diagnostics: &mut Vec<Diagnostic>,
  ranges: &DisabledRanges,
  line_of: &dyn Fn(usize) -> usize,
  policy: &DisablePolicy,
  file_path: &str,
) {
  if ranges.is_empty() {
    return;
  }
  // Stylelint records the problems a range disabled even under
  // `ignoreDisables`; only dropping them is switched off.
  let mut disabled_warnings: Vec<(String, usize)> = Vec::new();
  diagnostics.retain(|d| {
    if d.is_comment_problem()
      || d.is_unknown_rule()
      || d.is_invalid_option()
      || d.is_internal_error()
    {
      return true;
    }
    let line = line_of(d.span.offset);
    if !ranges.covers(&d.rule_name, line) {
      return true;
    }
    disabled_warnings.push((d.rule_name.clone(), line));
    // Kept only under `ignoreDisables`.
    policy.options.ignore
  });

  let options = policy.options;
  let severity = policy.report_severity();
  let mut report = |rule: &str, message: String, severity: Severity, node: usize| {
    diagnostics.push(
      Diagnostic::new(rule, message)
        .severity(severity)
        .span(ranges.nodes[node].span())
        .file_path(file_path),
    );
  };
  if options.report_needless {
    needless_disables(ranges, &disabled_warnings, policy, |rule, node| {
      report(
        NEEDLESS_DISABLES,
        format!("Needless disable for \"{rule}\""),
        severity,
        node,
      );
    });
  }
  if options.report_invalid_scope {
    for (rule, list) in &ranges.rules[1..] {
      if policy.is_configured(rule) {
        continue;
      }
      for range in list {
        if range.strict_start || range.strict_end == Some(true) {
          report(
            INVALID_SCOPE_DISABLES,
            format!("Rule \"{rule}\" isn't enabled"),
            severity,
            range.node,
          );
        }
      }
    }
  }
  if options.report_descriptionless {
    let mut reported: HashSet<usize> = HashSet::new();
    for (rule, list) in &ranges.rules {
      for range in list {
        if range.described || !reported.insert(range.node) {
          continue;
        }
        report(
          DESCRIPTIONLESS_DISABLES,
          format!("Disable for \"{rule}\" is missing a description"),
          severity,
          range.node,
        );
      }
    }
  }
  if options.report_unscoped {
    for range in ranges.all() {
      report(
        UNSCOPED_DISABLES,
        "Configuration comment must be scoped".to_string(),
        severity,
        range.node,
      );
    }
  }
  for (rule, list) in &ranges.rules[1..] {
    if !policy.forbids_disable(rule) {
      continue;
    }
    for range in list {
      report(
        REPORT_DISABLES,
        format!("Rule \"{rule}\" may not be disabled"),
        Severity::Error,
        range.node,
      );
    }
  }
}

/// Stylelint's `reportNeedlessDisables`, narrowed to what gale can prove:
/// `report(rule, node)` for each range of a rule that never reports
/// anything (see [`DisablePolicy::never_reports`]) whose comment disabled
/// no problem of that rule.
///
/// Blanket disables and disables of the rules gale runs are never called
/// needless: gale may not report every problem Stylelint (or a plugin it
/// lacks) would, so the comment may well be needed there.
fn needless_disables(
  ranges: &DisabledRanges,
  disabled_warnings: &[(String, usize)],
  policy: &DisablePolicy,
  mut report: impl FnMut(&str, usize),
) {
  // The rules each comment usefully disabled.
  let mut useful: HashMap<usize, Vec<&str>> = HashMap::new();
  for (rule, line) in disabled_warnings {
    let named = ranges.rules[1..]
      .iter()
      .filter(|(name, _)| name == rule)
      .flat_map(|(_, list)| list);
    for range in named.chain(ranges.all()) {
      if range.covers(*line) {
        useful.entry(range.node).or_default().push(rule);
      }
    }
  }
  let blanket_nodes: HashSet<usize> = ranges.all().iter().map(|range| range.node).collect();
  for (rule, list) in &ranges.rules[1..] {
    if !policy.never_reports(rule) {
      continue;
    }
    for range in list {
      if blanket_nodes.contains(&range.node) {
        continue;
      }
      let used = useful
        .get(&range.node)
        .is_some_and(|rules| rules.iter().any(|r| same_rule(r, rule)));
      if !used {
        report(rule, range.node);
      }
    }
  }
}
