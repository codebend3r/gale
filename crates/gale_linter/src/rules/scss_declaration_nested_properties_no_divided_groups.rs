use gale_diagnostics::Severity;

use crate::rule::Rule;

/// Disallow divided groups in nested property declarations.
///
/// Stub — always passes. Full implementation deferred.
pub struct ScssDeclarationNestedPropertiesNoDividedGroups;

impl Rule for ScssDeclarationNestedPropertiesNoDividedGroups {
  fn name(&self) -> &'static str {
    "scss/declaration-nested-properties-no-divided-groups"
  }

  fn description(&self) -> &'static str {
    "Disallow divided groups in nested property declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }
}
