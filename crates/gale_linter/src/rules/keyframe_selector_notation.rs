use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require keyword or percentage notation for keyframe selectors.
///
/// In "percentage-unless-within-keyword-only-block" mode (default), prefers
/// `0%` over `from` and `100%` over `to`, unless all selectors in the keyframe
/// block use only keywords.
///
/// Equivalent to Stylelint's `keyframe-selector-notation` rule.
pub struct KeyframeSelectorNotation;

impl Rule for KeyframeSelectorNotation {
  fn name(&self) -> &'static str {
    "keyframe-selector-notation"
  }

  fn description(&self) -> &'static str {
    "Specify keyword or percentage notation for keyframe selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };
    if !at_rule.name.eq_ignore_ascii_case("keyframes") {
      return vec![];
    }

    let mut diags = Vec::new();

    // Collect all keyframe selectors to determine if this is a keyword-only block.
    let selectors: Vec<&str> = at_rule
      .children
      .iter()
      .filter_map(|child| {
        if let CssNode::Style(rule) = child {
          Some(rule.selector.as_str())
        } else {
          None
        }
      })
      .collect();

    // Check if all selectors are keywords only (from/to).
    let all_keywords = selectors.iter().all(|sel| {
      sel
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .all(|s| s == "from" || s == "to")
    });

    // If all selectors are keywords, this is a keyword-only block — allow it.
    if all_keywords {
      return vec![];
    }

    // Otherwise, flag `from` and `to` usage, suggesting `0%` and `100%`.
    for child in &at_rule.children {
      if let CssNode::Style(rule) = child {
        for part in rule.selector.split(',') {
          let trimmed = part.trim().to_ascii_lowercase();
          if trimmed == "from" {
            diags.push(
              Diagnostic::new(
                self.name(),
                "Expected \"0%\" instead of \"from\"".to_string(),
              )
              .severity(self.default_severity())
              .span(Span::new(rule.span.offset, rule.span.length)),
            );
          } else if trimmed == "to" {
            diags.push(
              Diagnostic::new(
                self.name(),
                "Expected \"100%\" instead of \"to\"".to_string(),
              )
              .severity(self.default_severity())
              .span(Span::new(rule.span.offset, rule.span.length)),
            );
          }
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule as CssAtRule, Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn keyframes_with_selectors(selectors: &[&str]) -> CssNode {
    let children: Vec<CssNode> = selectors
      .iter()
      .map(|sel| {
        CssNode::Style(StyleRule {
          selector: sel.to_string(),
          declarations: vec![Declaration {
            property: "opacity".to_string(),
            value: "1".to_string(),
            span: ParserSpan::new(0, 0),
            important: false,
          }],
          span: ParserSpan::new(0, 0),
          ..Default::default()
        })
      })
      .collect();
    CssNode::AtRule(CssAtRule {
      name: "keyframes".to_string(),
      params: "fadeIn".to_string(),
      span: ParserSpan::new(0, 0),
      children,
    })
  }

  #[test]
  fn reports_from_when_mixed_with_percentage() {
    let node = keyframes_with_selectors(&["from", "50%", "100%"]);
    let d = KeyframeSelectorNotation.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("0%"));
    assert!(d[0].message.contains("from"));
  }

  #[test]
  fn reports_to_when_mixed_with_percentage() {
    let node = keyframes_with_selectors(&["0%", "50%", "to"]);
    let d = KeyframeSelectorNotation.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("100%"));
    assert!(d[0].message.contains("to"));
  }

  #[test]
  fn allows_keyword_only_block() {
    let node = keyframes_with_selectors(&["from", "to"]);
    let d = KeyframeSelectorNotation.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_percentage_only_block() {
    let node = keyframes_with_selectors(&["0%", "50%", "100%"]);
    let d = KeyframeSelectorNotation.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_both_from_and_to_when_mixed() {
    let node = keyframes_with_selectors(&["from", "50%", "to"]);
    let d = KeyframeSelectorNotation.check(&node, &ctx());
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn ignores_non_keyframes_at_rule() {
    let node = CssNode::AtRule(CssAtRule {
      name: "media".to_string(),
      params: "screen".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    let d = KeyframeSelectorNotation.check(&node, &ctx());
    assert!(d.is_empty());
  }
}
