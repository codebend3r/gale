use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specified combinators in selectors.
///
/// Equivalent to Stylelint's `selector-combinator-disallowed-list` rule.
pub struct SelectorCombinatorDisallowedList;

impl Rule for SelectorCombinatorDisallowedList {
  fn name(&self) -> &'static str {
    "selector-combinator-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed combinators"
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
    for combinator in extract_combinators(&rule.selector) {
      if disallowed.iter().any(|d| d == &combinator) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected combinator \"{combinator}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract combinators from a selector string.
/// Recognised combinators: `>`, `+`, `~`, `||`, and ` ` (descendant).
fn extract_combinators(selector: &str) -> Vec<String> {
  let mut combinators = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_brackets = 0i32;

  while i < len {
    // Skip attribute selectors and pseudo-function arguments
    if chars[i] == '[' || chars[i] == '(' {
      in_brackets += 1;
      i += 1;
      continue;
    }
    if chars[i] == ']' || chars[i] == ')' {
      in_brackets -= 1;
      i += 1;
      continue;
    }
    if in_brackets > 0 {
      i += 1;
      continue;
    }

    // Column combinator ||
    if i + 1 < len && chars[i] == '|' && chars[i + 1] == '|' {
      combinators.push("||".to_string());
      i += 2;
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      continue;
    }

    // Explicit combinators
    if chars[i] == '>' || chars[i] == '+' || chars[i] == '~' {
      combinators.push(chars[i].to_string());
      i += 1;
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      continue;
    }

    // Descendant combinator (whitespace between simple selectors)
    if chars[i].is_ascii_whitespace() {
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if i < len
        && chars[i] != '>'
        && chars[i] != '+'
        && chars[i] != '~'
        && chars[i] != ','
        && !(i + 1 < len && chars[i] == '|' && chars[i + 1] == '|')
      {
        combinators.push(" ".to_string());
      }
      continue;
    }

    i += 1;
  }

  combinators
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
  fn reports_disallowed_child_combinator() {
    let ctx = ctx_with_options(serde_json::json!([">"]));
    let d = SelectorCombinatorDisallowedList.check(&style_with_selector("a > .foo"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(">"));
  }

  #[test]
  fn reports_disallowed_descendant_combinator() {
    let ctx = ctx_with_options(serde_json::json!([" "]));
    let d = SelectorCombinatorDisallowedList.check(&style_with_selector("a .foo"), &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_combinator_not_in_list() {
    let ctx = ctx_with_options(serde_json::json!([">"]));
    let d = SelectorCombinatorDisallowedList.check(&style_with_selector("a + .foo"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = SelectorCombinatorDisallowedList.check(&style_with_selector("a > .foo"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      SelectorCombinatorDisallowedList.name(),
      "selector-combinator-disallowed-list"
    );
  }
}
