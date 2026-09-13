use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_at_rule_for_syntax;
use crate::rule::{Rule, RuleContext};

pub struct AtRuleNoUnknown;

impl Rule for AtRuleNoUnknown {
  fn name(&self) -> &'static str {
    "at-rule-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
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

  fn ctx() -> RuleContext<'static> {
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
  fn reports_unknown() {
    let d = AtRuleNoUnknown.check(&at("tailwind"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@tailwind"));
  }

  #[test]
  fn allows_known() {
    assert!(AtRuleNoUnknown.check(&at("media"), &ctx()).is_empty());
    assert!(AtRuleNoUnknown.check(&at("keyframes"), &ctx()).is_empty());
  }

  #[test]
  fn skips_vendor_prefixed() {
    assert!(
      AtRuleNoUnknown
        .check(&at("-webkit-keyframes"), &ctx())
        .is_empty()
    );
  }

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn less_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.less",
      source: "",
      syntax: Syntax::Less,
      options: None,
    }
  }

  #[test]
  fn allows_scss_at_rules_in_scss() {
    let ctx = scss_ctx();
    assert!(AtRuleNoUnknown.check(&at("mixin"), &ctx).is_empty());
    assert!(AtRuleNoUnknown.check(&at("include"), &ctx).is_empty());
    assert!(AtRuleNoUnknown.check(&at("if"), &ctx).is_empty());
    assert!(AtRuleNoUnknown.check(&at("each"), &ctx).is_empty());
    assert!(AtRuleNoUnknown.check(&at("extend"), &ctx).is_empty());
    assert!(AtRuleNoUnknown.check(&at("use"), &ctx).is_empty());
    assert!(AtRuleNoUnknown.check(&at("forward"), &ctx).is_empty());
    assert!(AtRuleNoUnknown.check(&at("at-root"), &ctx).is_empty());
  }

  #[test]
  fn reports_scss_at_rules_in_css() {
    let ctx = self::ctx();
    assert_eq!(AtRuleNoUnknown.check(&at("mixin"), &ctx).len(), 1);
    assert_eq!(AtRuleNoUnknown.check(&at("include"), &ctx).len(), 1);
    assert_eq!(AtRuleNoUnknown.check(&at("if"), &ctx).len(), 1);
  }

  #[test]
  fn allows_less_at_rules_in_less() {
    let ctx = less_ctx();
    assert!(AtRuleNoUnknown.check(&at("plugin"), &ctx).is_empty());
    assert!(
      AtRuleNoUnknown
        .check(&at("detached-ruleset"), &ctx)
        .is_empty()
    );
  }

  #[test]
  fn reports_less_at_rules_in_css() {
    let ctx = self::ctx();
    assert_eq!(AtRuleNoUnknown.check(&at("plugin"), &ctx).len(), 1);
  }
}
