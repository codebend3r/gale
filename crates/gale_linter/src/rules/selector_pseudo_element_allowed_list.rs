use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified pseudo-element selectors.
///
/// Equivalent to Stylelint's `selector-pseudo-element-allowed-list` rule.
pub struct SelectorPseudoElementAllowedList;

impl Rule for SelectorPseudoElementAllowedList {
  fn name(&self) -> &'static str {
    "selector-pseudo-element-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed pseudo-element selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let allowed: Vec<String> = ctx
      .options
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|item| item.as_str().map(|s| s.to_string()))
          .collect()
      })
      .unwrap_or_default();

    if allowed.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();
    for name in extract_pseudo_elements(&rule.selector) {
      if !allowed.iter().any(|a| a == &name) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected pseudo-element \"::{name}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract pseudo-element names from a selector string (::name patterns).
fn extract_pseudo_elements(selector: &str) -> Vec<String> {
  let mut elements = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
      i += 2;
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        elements.push(name);
      }
    } else {
      i += 1;
    }
  }

  elements
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
  fn reports_pseudo_element_not_in_allowed_list() {
    let ctx = ctx_with_options(serde_json::json!(["before"]));
    let d = SelectorPseudoElementAllowedList.check(&style_with_selector("a::after"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("::after"));
  }

  #[test]
  fn allows_pseudo_element_in_list() {
    let ctx = ctx_with_options(serde_json::json!(["before", "after"]));
    let d = SelectorPseudoElementAllowedList.check(&style_with_selector("a::before"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = SelectorPseudoElementAllowedList.check(&style_with_selector("a::before"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn case_sensitive() {
    let ctx = ctx_with_options(serde_json::json!(["before"]));
    let d = SelectorPseudoElementAllowedList.check(&style_with_selector("a::Before"), &ctx);
    // "before" does not match "Before" -- strict matching
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn vendor_prefixed_not_matched_by_unprefixed() {
    let ctx = ctx_with_options(serde_json::json!(["placeholder"]));
    let d = SelectorPseudoElementAllowedList
      .check(&style_with_selector("input::-webkit-placeholder"), &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      SelectorPseudoElementAllowedList.name(),
      "selector-pseudo-element-allowed-list"
    );
  }
}
