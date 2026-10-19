use gale_css_parser::CssNode;
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
  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    if nodes.is_empty() && context.source.trim().is_empty() {
      vec![Diagnostic::new(NAME, "Unexpected empty source").span(Span::new(0, 0))]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source};

  #[test]
  fn reports_empty_source() {
    let rule = configured(&NoEmptySource, None);
    let context = ctx_with_source("   \n  ");
    let diags = rule.check_root(&[], &context);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected empty source");
  }

  #[test]
  fn ignores_non_empty_source() {
    let rule = configured(&NoEmptySource, None);
    let context = ctx_with_source("a { color: red; }");
    let nodes = vec![CssNode::Style(gale_css_parser::StyleRule {
      selector: "a".to_string(),
      declarations: vec![gale_css_parser::Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: gale_diagnostics::Span::new(4, 10),
        important: false,
      }],
      span: gale_diagnostics::Span::new(0, 17),
      ..Default::default()
    })];
    let diags = rule.check_root(&nodes, &context);
    assert!(diags.is_empty());
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
