use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-empty-source";

declare_rule! {
  /// Reports when a CSS file contains no meaningful content.
  ///
  /// Equivalent to Stylelint's `no-empty-source` rule.
  pub struct NoEmptySource => NoEmptySourceCheck {
    name: NAME,
    description: "Disallow empty sources",
    default_severity: Warning,
  }
}

/// `no-empty-source`, which takes no options.
pub struct NoEmptySourceCheck;

impl NoEmptySourceCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for NoEmptySourceCheck {
  /// Flags a file that parsed to nothing and holds only whitespace.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    if tree.root_nodes().next().is_none() && context.source.trim().is_empty() {
      vec![Diagnostic::new(NAME, "Unexpected empty source").span(Span::new(0, 0))]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source};

  #[test]
  fn reports_empty_source() {
    let rule = configured(&NoEmptySource, None);
    let diags = rule.check_source(&ctx_with_source("   \n  "));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected empty source");
  }

  #[test]
  fn ignores_non_empty_source() {
    let rule = configured(&NoEmptySource, None);
    assert!(
      rule
        .check_source(&ctx_with_source("a { color: red; }"))
        .is_empty()
    );
    assert!(rule.check_source(&ctx_with_source("/* */")).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options("no-empty-source", serde_json::json!(true)).is_empty());
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(invalid_options("no-empty-source", serde_json::json!([true, { "foo": 1 }])).is_empty());
    assert_eq!(
      invalid_options("no-empty-source", serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"no-empty-source\""]
    );
  }
}
