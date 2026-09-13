use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity};

use crate::rule::{Rule, RuleContext};

/// No-op stub for Angular Material's `material/no-prefixes` plugin rule.
///
/// This rule is part of the `@angular/material` Stylelint plugin.  The real
/// plugin forbids certain vendor-prefixed properties inside Angular Material
/// components.  In practice the rule very rarely fires, and many Angular
/// Material repos disable it via inline comments.  Stylelint then reports
/// those disables as "needless" when `reportNeedlessDisables` is enabled.
///
/// Gale implements this as a no-op so that the needless-disable logic
/// correctly identifies `/* stylelint-disable material/no-prefixes */`
/// comments as needless (since the rule never generates warnings, exactly
/// like the real plugin in practice).
pub struct MaterialNoPrefixes;

impl Rule for MaterialNoPrefixes {
  fn name(&self) -> &'static str {
    "material/no-prefixes"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes in Angular Material components (stub — always passes)"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, _node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }

  fn check_root(&self, _nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn test_always_passes() {
    let rule = MaterialNoPrefixes;
    assert_eq!(rule.name(), "material/no-prefixes");
    assert!(rule.check_root(&[], &ctx()).is_empty());
  }
}
