use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports when a CSS file contains no meaningful content.
///
/// Equivalent to Stylelint's `no-empty-source` rule.
pub struct NoEmptySource;

impl Rule for NoEmptySource {
  fn name(&self) -> &'static str {
    "no-empty-source"
  }

  fn description(&self) -> &'static str {
    "Disallow empty sources"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    if nodes.is_empty() && context.source.trim().is_empty() {
      vec![
        Diagnostic::new(self.name(), "Unexpected empty source")
          .severity(self.default_severity())
          .span(Span::new(0, 0)),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  #[test]
  fn reports_empty_source() {
    let rule = NoEmptySource;
    let context = RuleContext {
      file_path: "test.css",
      source: "   \n  ",
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected empty source");
  }

  #[test]
  fn ignores_non_empty_source() {
    let rule = NoEmptySource;
    let context = RuleContext {
      file_path: "test.css",
      source: "a { color: red; }",
      syntax: Syntax::Css,
      options: None,
    };
    let nodes = vec![CssNode::Style(gale_css_parser::StyleRule {
      selector: "a".to_string(),
      declarations: vec![gale_css_parser::Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: gale_css_parser::Span::new(4, 10),
        important: false,
      }],
      span: gale_css_parser::Span::new(0, 17),
      ..Default::default()
    })];
    let diags = rule.check_root(&nodes, &context);
    assert!(diags.is_empty());
  }
}
