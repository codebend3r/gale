use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate selectors within keyframe blocks.
///
/// Equivalent to Stylelint's `keyframe-block-no-duplicate-selectors` rule.
///
/// Checks `@keyframes` at-rules for child `Style` nodes that share the same
/// selector (e.g. two `from` or two `50%` blocks).
pub struct KeyframeBlockNoDuplicateSelectors;

impl Rule for KeyframeBlockNoDuplicateSelectors {
  fn name(&self) -> &'static str {
    "keyframe-block-no-duplicate-selectors"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate selectors within keyframe blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };

    if at_rule.name != "keyframes" {
      return vec![];
    }

    let mut seen = HashSet::new();
    let mut diagnostics = Vec::new();

    for child in &at_rule.children {
      if let CssNode::Style(style_rule) = child {
        let selector = style_rule.selector.trim().to_ascii_lowercase();
        if !seen.insert(selector.clone()) {
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Unexpected duplicate keyframe selector \"{}\"", selector),
            )
            .severity(self.default_severity())
            .span(Span::new(style_rule.span.offset, style_rule.span.length)),
          );
        }
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_duplicate_keyframe_selectors() {
    let rule = KeyframeBlockNoDuplicateSelectors;
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, 80),
      children: vec![
        CssNode::Style(StyleRule {
          selector: "from".to_string(),
          declarations: vec![Declaration {
            property: "opacity".to_string(),
            value: "0".to_string(),
            span: ParserSpan::new(25, 10),
            important: false,
          }],
          span: ParserSpan::new(19, 20),
          ..Default::default()
        }),
        CssNode::Style(StyleRule {
          selector: "to".to_string(),
          declarations: vec![Declaration {
            property: "opacity".to_string(),
            value: "1".to_string(),
            span: ParserSpan::new(48, 10),
            important: false,
          }],
          span: ParserSpan::new(40, 20),
          ..Default::default()
        }),
        CssNode::Style(StyleRule {
          selector: "from".to_string(),
          declarations: vec![Declaration {
            property: "opacity".to_string(),
            value: "0.5".to_string(),
            span: ParserSpan::new(68, 12),
            important: false,
          }],
          span: ParserSpan::new(61, 22),
          ..Default::default()
        }),
      ],
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate keyframe selector \"from\""
    );
  }

  #[test]
  fn ignores_unique_keyframe_selectors() {
    let rule = KeyframeBlockNoDuplicateSelectors;
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, 60),
      children: vec![
        CssNode::Style(StyleRule {
          selector: "from".to_string(),
          declarations: vec![],
          span: ParserSpan::new(19, 15),
          ..Default::default()
        }),
        CssNode::Style(StyleRule {
          selector: "to".to_string(),
          declarations: vec![],
          span: ParserSpan::new(35, 13),
          ..Default::default()
        }),
      ],
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_non_keyframes_at_rules() {
    let rule = KeyframeBlockNoDuplicateSelectors;
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 40),
      children: vec![],
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn detects_duplicates_in_parsed_css_keyframes() {
    let css = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } from { opacity: 0.5; } }";
    let result = gale_css_parser::parse(css, Syntax::Css).expect("should parse CSS");
    let rule = KeyframeBlockNoDuplicateSelectors;
    let ctx = make_context();
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect duplicate 'from' in CSS keyframes"
    );
  }

  #[test]
  fn detects_duplicates_in_parsed_scss_keyframes() {
    let scss = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } from { opacity: 0.5; } }";
    let result = gale_css_parser::parse(scss, Syntax::Scss).expect("should parse SCSS");
    let rule = KeyframeBlockNoDuplicateSelectors;
    let ctx = RuleContext {
      file_path: "test.scss",
      source: scss,
      syntax: Syntax::Scss,
      options: None,
    };
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect duplicate 'from' in SCSS keyframes"
    );
  }
}
