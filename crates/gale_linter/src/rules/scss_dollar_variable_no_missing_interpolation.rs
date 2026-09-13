use gale_diagnostics::Severity;

use crate::rule::Rule;

/// Disallow `$variables` in strings where interpolation `#{$var}` should be
/// used.
///
/// Stub — always passes. Full implementation deferred.
pub struct ScssDollarVariableNoMissingInterpolation;

impl Rule for ScssDollarVariableNoMissingInterpolation {
  fn name(&self) -> &'static str {
    "scss/dollar-variable-no-missing-interpolation"
  }

  fn description(&self) -> &'static str {
    "Disallow $variables without #{} interpolation in strings"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }
}
