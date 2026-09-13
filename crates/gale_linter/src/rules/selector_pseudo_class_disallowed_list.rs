use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specified pseudo-class selectors.
///
/// Equivalent to Stylelint's `selector-pseudo-class-disallowed-list` rule.
pub struct SelectorPseudoClassDisallowedList;

impl Rule for SelectorPseudoClassDisallowedList {
  fn name(&self) -> &'static str {
    "selector-pseudo-class-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed pseudo-class selectors"
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
    for name in extract_pseudo_classes(&rule.selector) {
      if disallowed.iter().any(|d| d == &name) {
        diags.push(
          Diagnostic::new(self.name(), format!("Unexpected pseudo-class \":{name}\""))
            .severity(self.default_severity())
            .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract pseudo-class names from a selector string.
fn extract_pseudo_classes(selector: &str) -> Vec<String> {
  let mut classes = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '[' {
      let mut depth = 1;
      i += 1;
      while i < len && depth > 0 {
        match chars[i] {
          '[' => depth += 1,
          ']' => depth -= 1,
          '"' | '\'' => {
            let quote = chars[i];
            i += 1;
            while i < len && chars[i] != quote {
              if chars[i] == '\\' {
                i += 1;
              }
              i += 1;
            }
          }
          _ => {}
        }
        i += 1;
      }
      continue;
    }
    if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      continue;
    }
    if chars[i] == ':' {
      i += 1;
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        classes.push(name);
      }
      if i < len && chars[i] == '(' {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if chars[i] == '(' {
            depth += 1;
          } else if chars[i] == ')' {
            depth -= 1;
          }
          i += 1;
        }
      }
    } else {
      i += 1;
    }
  }

  classes
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
  fn reports_disallowed_pseudo_class() {
    let ctx = ctx_with_options(serde_json::json!(["hover"]));
    let d = SelectorPseudoClassDisallowedList.check(&style_with_selector("a:hover"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(":hover"));
  }

  #[test]
  fn allows_pseudo_class_not_in_list() {
    let ctx = ctx_with_options(serde_json::json!(["hover"]));
    let d = SelectorPseudoClassDisallowedList.check(&style_with_selector("a:focus"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = SelectorPseudoClassDisallowedList.check(&style_with_selector("a:hover"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn case_sensitive() {
    let ctx = ctx_with_options(serde_json::json!(["hover"]));
    let d = SelectorPseudoClassDisallowedList.check(&style_with_selector("a:Hover"), &ctx);
    // "hover" does not match "Hover" -- strict matching
    assert!(d.is_empty());
  }

  #[test]
  fn vendor_prefixed_not_matched_by_unprefixed() {
    let ctx = ctx_with_options(serde_json::json!(["placeholder"]));
    let d = SelectorPseudoClassDisallowedList
      .check(&style_with_selector("input:-webkit-placeholder"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      SelectorPseudoClassDisallowedList.name(),
      "selector-pseudo-class-disallowed-list"
    );
  }
}
