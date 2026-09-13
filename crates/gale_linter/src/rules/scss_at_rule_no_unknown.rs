use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_at_rule_for_syntax;
use crate::rule::{Rule, RuleContext};

/// SCSS replacement for `at-rule-no-unknown`.
///
/// Like the core rule but recognises SCSS-specific at-rules (`@mixin`,
/// `@include`, `@if`, `@each`, `@extend`, `@use`, `@forward`, etc.).
/// Only active when syntax is SCSS or Sass.
pub struct ScssAtRuleNoUnknown;

impl Rule for ScssAtRuleNoUnknown {
  fn name(&self) -> &'static str {
    "scss/at-rule-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown at-rules (SCSS-aware)"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    // Stylelint runs scss/at-rule-no-unknown on ALL file types (including
    // plain CSS) when the rule is enabled — typically via
    // stylelint-config-standard-scss.  Do not filter by syntax.

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    // Skip vendor-prefixed
    if at.name.starts_with('-') {
      return vec![];
    }

    if !is_known_at_rule_for_syntax(&at.name, ctx.syntax) {
      vec![
        Diagnostic::new(
          self.name(),
          format!("Unexpected unknown at-rule \"@{}\"", at.name),
        )
        .severity(self.default_severity())
        .span(Span::new(at.span.offset, at.span.length)),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Span, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn css_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn at(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_unknown_in_css() {
    // Stylelint runs scss/at-rule-no-unknown on CSS files too when enabled
    // (typically via stylelint-config-standard-scss).
    assert_eq!(
      ScssAtRuleNoUnknown.check(&at("tailwind"), &css_ctx()).len(),
      1
    );
  }

  #[test]
  fn allows_scss_at_rules() {
    let ctx = scss_ctx();
    assert!(ScssAtRuleNoUnknown.check(&at("mixin"), &ctx).is_empty());
    assert!(ScssAtRuleNoUnknown.check(&at("include"), &ctx).is_empty());
    assert!(ScssAtRuleNoUnknown.check(&at("if"), &ctx).is_empty());
    assert!(ScssAtRuleNoUnknown.check(&at("each"), &ctx).is_empty());
    assert!(ScssAtRuleNoUnknown.check(&at("extend"), &ctx).is_empty());
    assert!(ScssAtRuleNoUnknown.check(&at("use"), &ctx).is_empty());
    assert!(ScssAtRuleNoUnknown.check(&at("forward"), &ctx).is_empty());
    assert!(ScssAtRuleNoUnknown.check(&at("media"), &ctx).is_empty());
  }

  #[test]
  fn reports_unknown_in_scss() {
    let ctx = scss_ctx();
    let d = ScssAtRuleNoUnknown.check(&at("tailwind"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@tailwind"));
  }

  #[test]
  fn skips_vendor_prefixed() {
    assert!(
      ScssAtRuleNoUnknown
        .check(&at("-webkit-keyframes"), &scss_ctx())
        .is_empty()
    );
  }
}
