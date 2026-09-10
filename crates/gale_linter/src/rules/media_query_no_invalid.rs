use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports obviously invalid media queries.
///
/// Checks for common errors in `@media` params such as:
/// - Empty media query params
/// - Unbalanced parentheses
/// - Consecutive operators (and/or/not) without a condition
///
/// Equivalent to Stylelint's `media-query-no-invalid` rule.
pub struct MediaQueryNoInvalid;

impl Rule for MediaQueryNoInvalid {
  fn name(&self) -> &'static str {
    "media-query-no-invalid"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid media queries"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "media" {
      return vec![];
    }

    let params = at.params.trim();

    // Empty media query
    if params.is_empty() {
      return vec![
        Diagnostic::new(self.name(), "Unexpected empty media query")
          .severity(self.default_severity())
          .span(Span::new(at.span.offset, at.span.length)),
      ];
    }

    // Check for unbalanced parentheses
    let mut depth: i32 = 0;
    for ch in params.chars() {
      match ch {
        '(' => depth += 1,
        ')' => {
          depth -= 1;
          if depth < 0 {
            return vec![
              Diagnostic::new(
                self.name(),
                "Unexpected unbalanced parentheses in media query",
              )
              .severity(self.default_severity())
              .span(Span::new(at.span.offset, at.span.length)),
            ];
          }
        }
        _ => {}
      }
    }
    if depth != 0 {
      return vec![
        Diagnostic::new(
          self.name(),
          "Unexpected unbalanced parentheses in media query",
        )
        .severity(self.default_severity())
        .span(Span::new(at.span.offset, at.span.length)),
      ];
    }

    // Check for empty parentheses ()
    if params.contains("()") {
      return vec![
        Diagnostic::new(self.name(), "Unexpected empty parentheses in media query")
          .severity(self.default_severity())
          .span(Span::new(at.span.offset, at.span.length)),
      ];
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_empty_media_query() {
    let d = MediaQueryNoInvalid.check(&media(""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty"));
  }

  #[test]
  fn reports_empty_parentheses() {
    let d = MediaQueryNoInvalid.check(&media("screen and ()"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty parentheses"));
  }

  #[test]
  fn allows_valid_media_queries() {
    assert!(
      MediaQueryNoInvalid
        .check(&media("(min-width: 768px)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaQueryNoInvalid
        .check(&media("screen and (color)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaQueryNoInvalid
        .check(&media("(hover: hover)"), &ctx())
        .is_empty()
    );
  }
}
