use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify a pattern for the selectors of rules nested within rules.
///
/// Equivalent to Stylelint's `selector-nested-pattern` rule.
/// The primary option is a regex pattern string that nested selectors must match.
pub struct SelectorNestedPattern;

impl Rule for SelectorNestedPattern {
  fn name(&self) -> &'static str {
    "selector-nested-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for the selectors of rules nested within rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read the pattern from options. If no pattern configured, skip.
    let pattern_str = match ctx.primary_option_str() {
      Some(p) => p,
      None => return vec![],
    };

    // Use fancy_regex to support lookaheads/lookbehinds (e.g. "^(?!.*&[-_])")
    let re = match fancy_regex::Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    let mut diags = Vec::new();
    check_nested_selectors(self, rule, &re, pattern_str, &mut diags);
    diags
  }
}

fn check_nested_selectors(
  rule: &SelectorNestedPattern,
  style: &gale_css_parser::StyleRule,
  re: &fancy_regex::Regex,
  pattern_str: &str,
  diags: &mut Vec<Diagnostic>,
) {
  for child in &style.children {
    let selector = child.selector.trim();
    // Check each selector in a selector list (split by comma)
    for individual in selector.split(',') {
      let sel = individual.trim();
      if sel.is_empty() {
        continue;
      }
      if !re.is_match(sel).unwrap_or(false) {
        diags.push(
          Diagnostic::new(
            rule.name(),
            format!("Expected nested selector \"{sel}\" to match pattern \"{pattern_str}\""),
          )
          .severity(rule.default_severity())
          .span(Span::new(child.span.offset, child.span.length)),
        );
        // Only report once per selector list, not per individual selector
        break;
      }
    }
    // Recurse into deeper nesting
    check_nested_selectors(rule, child, re, pattern_str, diags);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_pattern(pattern: &str) -> (serde_json::Value, RuleContext<'static>) {
    // Leak the string so we get 'static lifetime for tests
    let opts = serde_json::json!(pattern);
    (
      opts,
      RuleContext {
        file_path: "t.css",
        source: "",
        syntax: Syntax::Css,
        options: None,
      },
    )
  }

  fn make_decl() -> Declaration {
    Declaration {
      property: "color".to_string(),
      value: "red".to_string(),
      span: ParserSpan::new(0, 0),
      important: false,
    }
  }

  #[test]
  fn reports_nested_without_ampersand() {
    let opts = serde_json::json!("^&");
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      children: vec![StyleRule {
        selector: ".child".to_string(),
        declarations: vec![make_decl()],
        span: ParserSpan::new(20, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 40),
      nested_at_rules: Vec::new(),
    });
    let d = SelectorNestedPattern.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".child"));
  }

  #[test]
  fn allows_nested_with_ampersand() {
    let opts = serde_json::json!("^&");
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      children: vec![StyleRule {
        selector: "&:hover".to_string(),
        declarations: vec![make_decl()],
        span: ParserSpan::new(20, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 40),
      nested_at_rules: Vec::new(),
    });
    let d = SelectorNestedPattern.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn no_diagnostics_for_no_children() {
    let opts = serde_json::json!("^&");
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      span: ParserSpan::new(0, 20),
      ..Default::default()
    });
    let d = SelectorNestedPattern.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn no_diagnostics_without_options() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let node = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      children: vec![StyleRule {
        selector: ".child".to_string(),
        declarations: vec![make_decl()],
        span: ParserSpan::new(20, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 40),
      nested_at_rules: Vec::new(),
    });
    let d = SelectorNestedPattern.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn negative_lookahead_pattern() {
    // Pattern like patternfly uses: "^(?!.*&[-_])"
    // This pattern rejects selectors containing &- or &_
    let opts = serde_json::json!("^(?!.*&[-_])");
    let ctx = RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: Some(&opts),
    };
    // &__child should be rejected (contains &_)
    let node = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      children: vec![StyleRule {
        selector: "&__child".to_string(),
        declarations: vec![make_decl()],
        span: ParserSpan::new(20, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 40),
      nested_at_rules: Vec::new(),
    });
    let d = SelectorNestedPattern.check(&node, &ctx);
    assert_eq!(d.len(), 1);

    // &:hover should be accepted (no &- or &_)
    let node2 = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      children: vec![StyleRule {
        selector: "&:hover".to_string(),
        declarations: vec![make_decl()],
        span: ParserSpan::new(20, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 40),
      nested_at_rules: Vec::new(),
    });
    let d2 = SelectorNestedPattern.check(&node2, &ctx);
    assert!(d2.is_empty());

    // .child without & should also be accepted (no &-)
    let node3 = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      children: vec![StyleRule {
        selector: ".child".to_string(),
        declarations: vec![make_decl()],
        span: ParserSpan::new(20, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 40),
      nested_at_rules: Vec::new(),
    });
    let d3 = SelectorNestedPattern.check(&node3, &ctx);
    assert!(d3.is_empty());
  }
}
