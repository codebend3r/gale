use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified CSS properties.
///
/// Options: an array of property names that are allowed. All other properties
/// are flagged.
///
/// Equivalent to Stylelint's `property-allowed-list` rule.
pub struct PropertyAllowedList;

impl Rule for PropertyAllowedList {
  fn name(&self) -> &'static str {
    "property-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let allowed: Vec<String> = match ctx.options {
      Some(serde_json::Value::Array(arr)) => arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect(),
      _ => return vec![],
    };

    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();

    for decl in declarations {
      if !allowed.iter().any(|a| a == &decl.property) {
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

  fn ctx_with_options(options: Option<serde_json::Value>) -> RuleContext<'static> {
    let opts: Option<&'static serde_json::Value> = options.map(|v| &*Box::leak(Box::new(v)));
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: opts,
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
  fn allows_all_when_no_options() {
    let ctx = ctx_with_options(None);
    let d = PropertyAllowedList.check(&style_with_prop("color", "red"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_listed_property() {
    let ctx = ctx_with_options(Some(serde_json::json!(["color", "margin"])));
    let d = PropertyAllowedList.check(&style_with_prop("color", "red"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn flags_unlisted_property() {
    let ctx = ctx_with_options(Some(serde_json::json!(["color"])));
    let d = PropertyAllowedList.check(&style_with_prop("margin", "0"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("margin"));
  }

  #[test]
  fn case_sensitive() {
    let ctx = ctx_with_options(Some(serde_json::json!(["color"])));
    let d = PropertyAllowedList.check(&style_with_prop("Color", "red"), &ctx);
    // "color" does not match "Color" -- strict matching
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn vendor_prefixed_not_matched_by_unprefixed() {
    let ctx = ctx_with_options(Some(serde_json::json!(["transform"])));
    let d = PropertyAllowedList.check(&style_with_prop("-webkit-transform", "rotate(45deg)"), &ctx);
    // "-webkit-transform" is not "transform" -- strict matching
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(PropertyAllowedList.name(), "property-allowed-list");
  }
}
