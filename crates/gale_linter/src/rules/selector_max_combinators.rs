use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of combinators in a selector.
///
/// Equivalent to Stylelint's `selector-max-combinators` rule.
/// Default maximum: 2. Counts combinators (>, +, ~, space) between compound selectors.
pub struct SelectorMaxCombinators;

const DEFAULT_MAX: usize = 2;

impl Rule for SelectorMaxCombinators {
  fn name(&self) -> &'static str {
    "selector-max-combinators"
  }

  fn description(&self) -> &'static str {
    "Limit the number of combinators in a selector"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let max = ctx
      .options
      .and_then(|v| v.as_u64())
      .map(|n| n as usize)
      .unwrap_or(DEFAULT_MAX);

    let mut diags = Vec::new();
    for sel in rule.selector.split(',') {
      let sel = sel.trim();
      let count = count_combinators(sel);
      if count > max {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Expected selector \"{sel}\" to have no more than {max} combinator(s), found {count}"
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Count combinators in a selector (>, +, ~, descendant whitespace).
/// Skips content inside attribute selectors `[...]` but NOT inside
/// pseudo-class function arguments — matching Stylelint v17 behavior
/// where selectors are evaluated as-written.
fn count_combinators(selector: &str) -> usize {
  if selector.is_empty() {
    return 0;
  }

  let mut count = 0usize;
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_brackets = 0i32; // only tracks [ ]

  while i < len {
    // Skip SCSS line comments
    if chars[i] == '/' && i + 1 < len && chars[i + 1] == '/' {
      while i < len && chars[i] != '\n' {
        i += 1;
      }
      continue;
    }

    if chars[i] == '[' {
      in_brackets += 1;
      i += 1;
      continue;
    }
    if chars[i] == ']' {
      in_brackets -= 1;
      i += 1;
      continue;
    }
    if in_brackets > 0 {
      i += 1;
      continue;
    }

    // Explicit combinators
    if chars[i] == '>' || chars[i] == '+' || chars[i] == '~' {
      count += 1;
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
      if i < len && chars[i] != '>' && chars[i] != '+' && chars[i] != '~' && chars[i] != ')' {
        count += 1;
      }
      continue;
    }

    i += 1;
  }

  count
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

  fn ctx_with_options(options: serde_json::Value) -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(Box::leak(Box::new(options))),
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
  fn reports_too_many_combinators() {
    // 3 combinators: .a .b > .c + .d
    let d = SelectorMaxCombinators.check(&style_with_selector(".a .b > .c + .d"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 3"));
  }

  #[test]
  fn allows_within_default_limit() {
    // 2 combinators: .a .b > .c
    let d = SelectorMaxCombinators.check(&style_with_selector(".a .b > .c"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn respects_configured_max() {
    let ctx = ctx_with_options(serde_json::json!(1));
    let d = SelectorMaxCombinators.check(&style_with_selector(".a .b > .c"), &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn single_selector_no_combinators() {
    let d = SelectorMaxCombinators.check(&style_with_selector(".foo"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(SelectorMaxCombinators.name(), "selector-max-combinators");
  }

  #[test]
  fn counts_combinators_inside_is() {
    // Stylelint v17: combinators inside :is() ARE counted.
    // `:is(.a .b)` has 1 descendant combinator.
    let ctx = ctx_with_options(serde_json::json!(0));
    let d = SelectorMaxCombinators.check(&style_with_selector(":is(.a .b)"), &ctx);
    assert_eq!(
      d.len(),
      1,
      "expected 1 diagnostic for combinator inside :is()"
    );
  }

  #[test]
  fn counts_combinators_inside_has() {
    // Stylelint v17: combinators inside :has() ARE counted.
    // `.foo:has(.bar > .baz)` has 1 child combinator inside :has().
    let ctx = ctx_with_options(serde_json::json!(0));
    let d = SelectorMaxCombinators.check(&style_with_selector(".foo:has(.bar > .baz)"), &ctx);
    assert_eq!(d.len(), 1);
  }
}
