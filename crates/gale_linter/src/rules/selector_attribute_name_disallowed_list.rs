use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specified attribute names in selectors.
///
/// Equivalent to Stylelint's `selector-attribute-name-disallowed-list` rule.
pub struct SelectorAttributeNameDisallowedList;

impl Rule for SelectorAttributeNameDisallowedList {
  fn name(&self) -> &'static str {
    "selector-attribute-name-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed attribute names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let disallowed: Vec<String> = ctx
      .options
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|item| item.as_str().map(|s| s.to_string()))
          .collect()
      })
      .unwrap_or_default();

    if disallowed.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();
    for attr_name in extract_attribute_names(&rule.selector) {
      if disallowed.iter().any(|d| d == &attr_name) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected attribute name \"{attr_name}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract attribute names from attribute selectors `[name...]`.
fn extract_attribute_names(selector: &str) -> Vec<String> {
  let mut names = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '[' {
      i += 1;
      // Skip whitespace
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      // Collect attribute name
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        names.push(name);
      }
      // Skip to closing bracket
      while i < len && chars[i] != ']' {
        i += 1;
      }
    }
    i += 1;
  }

  names
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_options(options: serde_json::Value) -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(Box::leak(Box::new(options))),
    }
  }

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, sel.len()),
      ..Default::default()
    })
  }

  #[test]
  fn reports_disallowed_attribute_name() {
    let ctx = ctx_with_options(serde_json::json!(["class", "id"]));
    let d =
      SelectorAttributeNameDisallowedList.check(&style_with_selector("a[class=\"foo\"]"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("class"));
  }

  #[test]
  fn allows_attribute_not_in_list() {
    let ctx = ctx_with_options(serde_json::json!(["class"]));
    let d = SelectorAttributeNameDisallowedList.check(&style_with_selector("a[href=\"/\"]"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let d =
      SelectorAttributeNameDisallowedList.check(&style_with_selector("a[class=\"foo\"]"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      SelectorAttributeNameDisallowedList.name(),
      "selector-attribute-name-disallowed-list"
    );
  }
}
