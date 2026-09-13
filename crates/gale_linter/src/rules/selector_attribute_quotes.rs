use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require quotes for attribute values in attribute selectors.
///
/// In "always" mode (the default), attribute selectors with a value must use
/// quotes around that value. For example, `[type=text]` should be `[type="text"]`.
/// Attribute selectors without a value (e.g., `[disabled]`) are ignored.
///
/// Equivalent to Stylelint's `selector-attribute-quotes` rule.
pub struct SelectorAttributeQuotes;

/// Operators that can appear in CSS attribute selectors before the value.
const ATTR_OPERATORS: &[&str] = &["~=", "|=", "^=", "$=", "*=", "="];

impl Rule for SelectorAttributeQuotes {
  fn name(&self) -> &'static str {
    "selector-attribute-quotes"
  }

  fn description(&self) -> &'static str {
    "Require or disallow quotes for attribute values in attribute selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Stylelint's isStandardSyntaxRule returns false for selectors with
    // SCSS/Less interpolation. Skip those to match Stylelint's behavior.
    if rule.selector.contains("#{") || rule.selector.contains("@{") {
      return vec![];
    }

    let mut diags = Vec::new();
    check_selector_for_unquoted_attrs(self, &rule.selector, rule.span.offset, &mut diags);
    diags
  }
}

/// Scan a selector string for attribute selectors with unquoted values.
fn check_selector_for_unquoted_attrs(
  rule: &SelectorAttributeQuotes,
  selector: &str,
  base_offset: usize,
  diags: &mut Vec<Diagnostic>,
) {
  let bytes = selector.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Find the start of an attribute selector.
    if bytes[i] != b'[' {
      i += 1;
      continue;
    }

    let bracket_start = i;
    i += 1;

    // Find the closing bracket.
    let mut bracket_end = None;
    let mut depth = 1;
    let mut j = i;
    while j < len {
      if bytes[j] == b'[' {
        depth += 1;
      } else if bytes[j] == b']' {
        depth -= 1;
        if depth == 0 {
          bracket_end = Some(j);
          break;
        }
      }
      j += 1;
    }

    let Some(end) = bracket_end else {
      break;
    };

    let attr_content = &selector[i..end];

    // Find which operator is used, if any.
    let mut found_operator = false;
    for op in ATTR_OPERATORS {
      if let Some(op_pos) = attr_content.find(op) {
        found_operator = true;
        let value_start = op_pos + op.len();
        let value_part = attr_content[value_start..].trim();

        if value_part.is_empty() {
          break;
        }

        let first_char = value_part.as_bytes()[0];
        if first_char != b'"' && first_char != b'\'' {
          // Unquoted attribute value — flag it.
          // Strip trailing `]`-adjacent flags like `i` or `s` (case sensitivity).
          let value_clean = value_part.trim_end_matches(|c: char| c.is_ascii_whitespace());
          // Calculate the offset of the value within the selector.
          // `i` is the position after `[`, `value_start` is relative
          // to `attr_content` (which starts at `i`), and we need to
          // account for any leading whitespace that `trim()` removed.
          let value_raw = &attr_content[value_start..];
          let leading_ws = value_raw.len() - value_raw.trim_start().len();
          let value_offset_in_selector = i + value_start + leading_ws;
          diags.push(
            Diagnostic::new(
              rule.name(),
              format!("Expected quotes around \"{}\"", value_clean,),
            )
            .severity(rule.default_severity())
            .span(Span::new(
              base_offset + value_offset_in_selector,
              value_clean.len(),
            )),
          );
        }
        break;
      }
    }

    if !found_operator {
      // No operator means no value (e.g., `[disabled]`) — skip.
    }

    i = end + 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_selector(selector: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: selector.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, selector.len() + 20),
      ..Default::default()
    })
  }

  #[test]
  fn flags_unquoted_attribute_value() {
    let node = style_with_selector("[type=text]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("text"));
  }

  #[test]
  fn allows_quoted_attribute_value() {
    let node = style_with_selector("[type=\"text\"]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_single_quoted_attribute_value() {
    let node = style_with_selector("[type='text']");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_attribute_without_value() {
    let node = style_with_selector("[disabled]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn flags_unquoted_with_tilde_operator() {
    let node = style_with_selector("[class~=foo]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("foo"));
  }

  #[test]
  fn flags_unquoted_with_caret_operator() {
    let node = style_with_selector("[href^=https]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("https"));
  }

  #[test]
  fn flags_unquoted_with_dollar_operator() {
    let node = style_with_selector("[src$=png]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn flags_unquoted_with_star_operator() {
    let node = style_with_selector("[data-value*=test]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn allows_quoted_with_various_operators() {
    for selector in &[
      "[class~=\"foo\"]",
      "[href^=\"https\"]",
      "[src$=\"png\"]",
      "[data-value*=\"test\"]",
      "[lang|=\"en\"]",
    ] {
      let node = style_with_selector(selector);
      let diags = SelectorAttributeQuotes.check(&node, &ctx());
      assert!(diags.is_empty(), "Expected no diags for {}", selector);
    }
  }

  #[test]
  fn flags_multiple_unquoted_attrs_in_one_selector() {
    let node = style_with_selector("[type=text][name=email]");
    let diags = SelectorAttributeQuotes.check(&node, &ctx());
    assert_eq!(diags.len(), 2);
  }
}
