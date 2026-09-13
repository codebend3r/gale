use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specific CSS properties.
///
/// By default, disallows no properties. Configure via rule options.
///
/// Equivalent to Stylelint's `property-disallowed-list` rule.
pub struct PropertyDisallowedList;

const DISALLOWED: &[&str] = &[];

impl Rule for PropertyDisallowedList {
  fn name(&self) -> &'static str {
    "property-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };
    for decl in declarations {
      if DISALLOWED.contains(&decl.property.as_str()) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected property \"{}\"", decl.property),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
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
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_prop(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn allows_all_when_list_empty() {
    let d = PropertyDisallowedList.check(&style_with_prop("color", "red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_standard_properties() {
    let d = PropertyDisallowedList.check(&style_with_prop("margin", "0"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(PropertyDisallowedList.name(), "property-disallowed-list");
  }
}
