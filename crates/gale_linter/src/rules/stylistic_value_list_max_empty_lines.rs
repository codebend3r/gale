use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of adjacent empty lines within value lists.
///
/// Primary option: integer (default 0).
pub struct StylisticValueListMaxEmptyLines;

impl Rule for StylisticValueListMaxEmptyLines {
  fn name(&self) -> &'static str {
    "@stylistic/value-list-max-empty-lines"
  }

  fn description(&self) -> &'static str {
    "Limit the number of adjacent empty lines within value lists"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = ctx.primary_option().and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diagnostics = Vec::new();

    for decl in &rule.declarations {
      let value = &decl.value;
      let mut consecutive_empty = 0usize;
      let mut max_found = 0usize;

      for line in value.lines() {
        if line.trim().is_empty() {
          consecutive_empty += 1;
          if consecutive_empty > max_found {
            max_found = consecutive_empty;
          }
        } else {
          consecutive_empty = 0;
        }
      }

      if max_found > max {
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!("Expected no more than {max} empty line(s) in value list"),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn check_value(value: &str, max: u64) -> Vec<Diagnostic> {
    let rule = StylisticValueListMaxEmptyLines;
    let opts = serde_json::json!(max);
    let ctx = RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: value.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    });
    rule.check(&node, &ctx)
  }

  #[test]
  fn allows_no_empty_lines_in_value() {
    let d = check_value("Arial,\nsans-serif", 0);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_empty_line_in_value_when_max_zero() {
    let d = check_value("Arial,\n\nsans-serif", 0);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 0"));
  }

  #[test]
  fn allows_one_empty_line_when_max_one() {
    let d = check_value("Arial,\n\nsans-serif", 1);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_two_empty_lines_when_max_one() {
    let d = check_value("Arial,\n\n\nsans-serif", 1);
    assert_eq!(d.len(), 1);
  }
}
