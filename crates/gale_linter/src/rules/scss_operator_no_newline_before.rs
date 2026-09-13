use gale_diagnostics::Severity;

use crate::rule::Rule;

/// Disallow newlines before operators (+, -, *, /).
///
/// Stub — always passes. Full implementation requires source-text scanning.
pub struct ScssOperatorNoNewlineBefore;

impl Rule for ScssOperatorNoNewlineBefore {
  fn name(&self) -> &'static str {
    "scss/operator-no-newline-before"
  }

  fn description(&self) -> &'static str {
    "Disallow newlines before Sass operators"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }
}
