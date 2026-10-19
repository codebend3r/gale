use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-nested-pattern";

declare_rule! {
  /// Specify a pattern for the selectors of rules nested within rules.
  ///
  /// Equivalent to Stylelint's `selector-nested-pattern` rule.
  /// The primary option is a regex pattern string that nested selectors must match.
  pub struct SelectorNestedPattern => SelectorNestedPatternCheck {
    name: NAME,
    description: "Specify a pattern for the selectors of rules nested within rules",
    default_severity: Warning,
  }
}

/// `selector-nested-pattern` as configured.
pub struct SelectorNestedPatternCheck {
  /// The pattern nested selectors must match; with none configured the
  /// rule reports nothing.
  pattern: Option<Pattern>,
}

impl SelectorNestedPatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`,
  /// then `splitList`, which gale accepts but does not yet apply (it always
  /// tests each selector of a list).  Lookarounds such as `^(?!.*&[-_])`
  /// work, and a pattern that does not compile is an invalid option.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.flag("splitList");
    options.finish()?;
    Ok(Self { pattern })
  }
}

impl Check for SelectorNestedPatternCheck {
  /// Flags nested selectors that do not match the configured pattern. Uses
  /// `fancy_regex` so lookarounds in the pattern work.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let Some(pattern) = &self.pattern else {
      return vec![];
    };

    let mut diags = Vec::new();
    check_nested_selectors(rule, pattern, &mut diags);
    diags
  }
}

/// Reports each nested selector, split on commas, that fails the pattern.
fn check_nested_selectors(
  style: &gale_css_parser::StyleRule,
  pattern: &Pattern,
  diags: &mut Vec<Diagnostic>,
) {
  let pattern_str = &pattern.source;
  for child in &style.children {
    let selector = child.selector.trim();
    // Check each selector in a selector list (split by comma)
    for individual in selector.split(',') {
      let sel = individual.trim();
      if sel.is_empty() {
        continue;
      }
      if !pattern.is_match(sel) {
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Expected nested selector \"{sel}\" to match pattern \"{pattern_str}\""),
          )
          .span(Span::new(child.span.offset, child.span.length)),
        );
        // Only report once per selector list, not per individual selector
        break;
      }
    }
    // Recurse into deeper nesting
    check_nested_selectors(child, pattern, diags);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

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
    let rule = configured(&SelectorNestedPattern, opts.clone());
    let ctx = ctx();
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
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".child"));
  }

  #[test]
  fn allows_nested_with_ampersand() {
    let opts = serde_json::json!("^&");
    let rule = configured(&SelectorNestedPattern, opts.clone());
    let ctx = ctx();
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
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn no_diagnostics_for_no_children() {
    let opts = serde_json::json!("^&");
    let rule = configured(&SelectorNestedPattern, opts.clone());
    let ctx = ctx();
    let node = CssNode::Style(StyleRule {
      selector: ".parent".to_string(),
      declarations: vec![make_decl()],
      span: ParserSpan::new(0, 20),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn no_diagnostics_without_options() {
    let rule = configured(&SelectorNestedPattern, None);
    let ctx = ctx();
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
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn negative_lookahead_pattern() {
    // Pattern like patternfly uses: "^(?!.*&[-_])"
    // This pattern rejects selectors containing &- or &_
    let opts = serde_json::json!("^(?!.*&[-_])");
    let rule = configured(&SelectorNestedPattern, opts.clone());
    let ctx = scss_ctx();
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
    let d = rule.check(&node, &ctx);
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
    let d2 = rule.check(&node2, &ctx);
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
    let d3 = rule.check(&node3, &ctx);
    assert!(d3.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(["^&", { "splitList": true }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"selector-nested-pattern\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["^&", { "splitList": "yes" }])),
      ["Invalid value \"yes\" for option \"splitList\" of rule \"selector-nested-pattern\""]
    );
  }
}
