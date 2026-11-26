//! One configuration-comment policy for standalone sheets and host documents.

use std::collections::HashSet;

use gale_diagnostics::{Diagnostic, Severity};

use super::{Collector, apply};
use crate::registry::canonical_name;
use crate::runner::DisableOptions;

#[derive(Debug, Clone, Default)]
pub struct DisablePolicy {
  pub options: DisableOptions,
  pub default_severity: Option<Severity>,
  pub configured: HashSet<String>,
  pub forbids_disable: HashSet<String>,
  /// The registered rules that never report a problem (their meta says
  /// [`crate::rule::Rule::never_reports`]), whose disables
  /// `reportNeedlessDisables` can prove needless.
  pub never_reports: HashSet<&'static str>,
}

impl DisablePolicy {
  /// Collect the ranges of the comments `scan` feeds the collector, with
  /// `line_of` giving the line of an offset, and apply them to
  /// `diagnostics`.
  ///
  /// Returns Stylelint's `CssSyntaxError` for a comment it rejects (an
  /// `enable` with nothing disabled, say), which replaces every other
  /// problem in the file, `ignoreDisables` or not.
  pub(crate) fn run(
    &self,
    diagnostics: &mut Vec<Diagnostic>,
    line_of: &dyn Fn(usize) -> usize,
    scan: impl FnOnce(&mut Collector<'_>),
    file_path: &str,
  ) -> Option<Diagnostic> {
    let mut collector = Collector::new(line_of);
    scan(&mut collector);
    let (ranges, rejected) = collector.finish();
    if let Some(rejected) = rejected {
      return Some(rejected.to_diagnostic(file_path));
    }
    apply(diagnostics, &ranges, line_of, self, file_path);
    None
  }

  /// The severity of the reports about disable comments.
  ///
  /// Stylelint's `reportNeedlessDisables`, `reportInvalidScopeDisables`,
  /// `reportDescriptionlessDisables` and `reportUnscopedDisables` all default
  /// to `defaultSeverity`, falling back to error.
  pub(crate) fn report_severity(&self) -> Severity {
    self.default_severity.unwrap_or(Severity::Error)
  }

  /// Whether the config enables `rule`, for `reportInvalidScopeDisables`.
  pub(crate) fn is_configured(&self, rule: &str) -> bool {
    self.configured.contains(canonical_name(rule))
  }

  /// Whether `rule` is a registered rule that never reports a problem, so
  /// a comment disabling it is needless whenever it disabled nothing.
  ///
  /// Every other rule might report problems Stylelint does not (or miss
  /// ones it does), so its disables are never called needless.
  pub(crate) fn never_reports(&self, rule: &str) -> bool {
    self.never_reports.contains(rule)
  }

  /// Whether `rule` has `reportDisables: true`.
  pub(crate) fn forbids_disable(&self, rule: &str) -> bool {
    self.forbids_disable.contains(rule) || self.forbids_disable.contains(canonical_name(rule))
  }
}
