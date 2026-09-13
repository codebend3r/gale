use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified attribute operators in selectors.
///
/// Equivalent to Stylelint's `selector-attribute-operator-allowed-list` rule.
pub struct SelectorAttributeOperatorAllowedList;

impl Rule for SelectorAttributeOperatorAllowedList {
  fn name(&self) -> &'static str {
    "selector-attribute-operator-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed attribute operators"
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
    for op in extract_attribute_operators(&rule.selector) {
      if !allowed.iter().any(|a| a == &op) {
        diags.push(
          Diagnostic::new(self.name(), format!("Unexpected operator \"{op}\""))
            .severity(self.default_severity())
            .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract attribute operators from attribute selectors `[name op value]`.
/// Recognised operators: `=`, `~=`, `|=`, `^=`, `$=`, `*=`.
fn extract_attribute_operators(selector: &str) -> Vec<String> {
  let mut operators = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '[' {
      i += 1;
      // Find the operator inside the attribute selector
      while i < len && chars[i] != ']' {
        // Skip strings inside attribute selector
        if chars[i] == '"' || chars[i] == '\'' {
          let quote = chars[i];
          i += 1;
          while i < len && chars[i] != quote {
            if chars[i] == '\\' {
              i += 1;
            }
            i += 1;
          }
          if i < len {
            i += 1;
          }
          continue;
        }
        // Check for two-char operators: ~=, |=, ^=, $=, *=
        if i + 1 < len && chars[i + 1] == '=' && matches!(chars[i], '~' | '|' | '^' | '$' | '*') {
          let op: String = chars[i..=i + 1].iter().collect();
          operators.push(op);
          i += 2;
          continue;
        }
        // Check for plain `=` (not preceded by ~|^$*)
        if chars[i] == '=' {
          operators.push("=".to_string());
          i += 1;
          continue;
        }
        i += 1;
      }
    }
    i += 1;
  }

  operators
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
  fn reports_disallowed_operator() {
    let ctx = ctx_with_options(serde_json::json!(["="]));
    let d =
      SelectorAttributeOperatorAllowedList.check(&style_with_selector("a[class*=\"foo\"]"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("*="));
  }

  #[test]
  fn allows_operator_in_list() {
    let ctx = ctx_with_options(serde_json::json!(["=", "~="]));
    let d =
      SelectorAttributeOperatorAllowedList.check(&style_with_selector("a[class~=\"foo\"]"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let d =
      SelectorAttributeOperatorAllowedList.check(&style_with_selector("a[class*=\"foo\"]"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      SelectorAttributeOperatorAllowedList.name(),
      "selector-attribute-operator-allowed-list"
    );
  }
}
