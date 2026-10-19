use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "block-no-redundant-nested-style-rules";

declare_rule! {
  /// Disallow redundant nested style rules that could be combined with their parent.
  ///
  /// Specifically, this rule flags nested rules whose selector is just `&` (a bare
  /// ampersand), since all declarations inside such a rule could be placed directly
  /// in the parent block.
  ///
  /// Equivalent to Stylelint's `block-no-redundant-nested-style-rules` rule.
  pub struct BlockNoRedundantNestedStyleRules => BlockNoRedundantNestedStyleRulesCheck {
    name: NAME,
    description: "Disallow redundant nested style rules that could be combined with their parent",
    default_severity: Warning,
  }
}

/// `block-no-redundant-nested-style-rules`, which takes no options.
pub struct BlockNoRedundantNestedStyleRulesCheck;

impl BlockNoRedundantNestedStyleRulesCheck {
  /// Stylelint's option: `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for BlockNoRedundantNestedStyleRulesCheck {
  /// Flags nested rules whose selector is a bare `&`, since their declarations
  /// could live in the parent block.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diags = Vec::new();

    for child in &rule.children {
      let selector_trimmed = child.selector.trim();

      // A nested rule whose selector is just `&` is always redundant —
      // its declarations could live in the parent.
      if selector_trimmed == "&" {
        diags.push(
          Diagnostic::new(
            NAME,
            "Unexpected redundant nested style rule with selector \"&\"; \
                         declarations can be placed directly in the parent rule"
              .to_string(),
          )
          .span(child.span),
        );
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule};
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options};

  fn style_with_children(selector: &str, children: Vec<StyleRule>) -> CssNode {
    CssNode::Style(StyleRule {
      selector: selector.to_string(),
      declarations: vec![],
      children,
      span: Span::new(0, 50),
      ..Default::default()
    })
  }

  fn child_rule(selector: &str) -> StyleRule {
    StyleRule {
      selector: selector.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(0, 0),
        important: false,
      }],
      span: Span::new(10, 30),
      ..Default::default()
    }
  }

  #[test]
  fn flags_bare_ampersand_nested_rule() {
    let rule = configured(&BlockNoRedundantNestedStyleRules, None);
    let node = style_with_children(".a", vec![child_rule("&")]);
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("redundant"));
    assert!(diags[0].message.contains("&"));
  }

  #[test]
  fn flags_ampersand_with_whitespace() {
    let rule = configured(&BlockNoRedundantNestedStyleRules, None);
    let node = style_with_children(".a", vec![child_rule("  &  ")]);
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn allows_meaningful_nested_selector() {
    let rule = configured(&BlockNoRedundantNestedStyleRules, None);
    let node = style_with_children(".a", vec![child_rule(".b")]);
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_ampersand_with_suffix() {
    let rule = configured(&BlockNoRedundantNestedStyleRules, None);
    // `&:hover` is not redundant — it creates `.a:hover`
    let node = style_with_children(".a", vec![child_rule("&:hover")]);
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_ampersand_with_class() {
    let rule = configured(&BlockNoRedundantNestedStyleRules, None);
    // `&.active` is not redundant — it creates `.a.active`
    let node = style_with_children(".a", vec![child_rule("&.active")]);
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn flags_multiple_redundant_children() {
    let rule = configured(&BlockNoRedundantNestedStyleRules, None);
    let node = style_with_children(".a", vec![child_rule("&"), child_rule("&")]);
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 2);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"block-no-redundant-nested-style-rules\""]
    );
  }
}
