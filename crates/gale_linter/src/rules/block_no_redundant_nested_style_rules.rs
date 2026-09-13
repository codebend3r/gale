use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow redundant nested style rules that could be combined with their parent.
///
/// Specifically, this rule flags nested rules whose selector is just `&` (a bare
/// ampersand), since all declarations inside such a rule could be placed directly
/// in the parent block.
///
/// Equivalent to Stylelint's `block-no-redundant-nested-style-rules` rule.
pub struct BlockNoRedundantNestedStyleRules;

impl Rule for BlockNoRedundantNestedStyleRules {
  fn name(&self) -> &'static str {
    "block-no-redundant-nested-style-rules"
  }

  fn description(&self) -> &'static str {
    "Disallow redundant nested style rules that could be combined with their parent"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

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
            self.name(),
            "Unexpected redundant nested style rule with selector \"&\"; \
                         declarations can be placed directly in the parent rule"
              .to_string(),
          )
          .severity(self.default_severity())
          .span(Span::new(child.span.offset, child.span.length)),
        );
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_children(selector: &str, children: Vec<StyleRule>) -> CssNode {
    CssNode::Style(StyleRule {
      selector: selector.to_string(),
      declarations: vec![],
      children,
      span: ParserSpan::new(0, 50),
      ..Default::default()
    })
  }

  fn child_rule(selector: &str) -> StyleRule {
    StyleRule {
      selector: selector.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(10, 30),
      ..Default::default()
    }
  }

  #[test]
  fn flags_bare_ampersand_nested_rule() {
    let node = style_with_children(".a", vec![child_rule("&")]);
    let diags = BlockNoRedundantNestedStyleRules.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("redundant"));
    assert!(diags[0].message.contains("&"));
  }

  #[test]
  fn flags_ampersand_with_whitespace() {
    let node = style_with_children(".a", vec![child_rule("  &  ")]);
    let diags = BlockNoRedundantNestedStyleRules.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn allows_meaningful_nested_selector() {
    let node = style_with_children(".a", vec![child_rule(".b")]);
    let diags = BlockNoRedundantNestedStyleRules.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_ampersand_with_suffix() {
    // `&:hover` is not redundant — it creates `.a:hover`
    let node = style_with_children(".a", vec![child_rule("&:hover")]);
    let diags = BlockNoRedundantNestedStyleRules.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_ampersand_with_class() {
    // `&.active` is not redundant — it creates `.a.active`
    let node = style_with_children(".a", vec![child_rule("&.active")]);
    let diags = BlockNoRedundantNestedStyleRules.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn flags_multiple_redundant_children() {
    let node = style_with_children(".a", vec![child_rule("&"), child_rule("&")]);
    let diags = BlockNoRedundantNestedStyleRules.check(&node, &ctx());
    assert_eq!(diags.len(), 2);
  }
}
