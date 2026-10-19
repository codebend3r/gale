use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "material/no-prefixes";

declare_rule! {
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
  pub struct MaterialNoPrefixes => MaterialNoPrefixesCheck {
    name: NAME,
    description: "Disallow vendor prefixes in Angular Material components (stub — always passes)",
    default_severity: Warning,
  }
}

/// `material/no-prefixes`, which ignores its options.
pub struct MaterialNoPrefixesCheck;

impl MaterialNoPrefixesCheck {
  /// The plugin validates none of its options (`browsers`, `filePattern`),
  /// so neither does the stub.
  fn from_setting(_setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    Ok(Self)
  }
}

impl Check for MaterialNoPrefixesCheck {
  /// Never fires; see the type docs for why this stub exists.
  fn check(&self, _node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }

  /// Never fires; see the type docs for why this stub exists.
  fn check_root(&self, _nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx, invalid_options};

  #[test]
  fn test_always_passes() {
    let rule = configured(&MaterialNoPrefixes, None);
    assert!(rule.check_root(&[], &ctx()).is_empty());
  }

  #[test]
  fn options_are_not_validated_like_the_plugin() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "browsers": ["last 2 versions"], "filePattern": "**/*.scss" }])
      )
      .is_empty()
    );
  }
}
