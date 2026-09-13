use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified at-rules.
///
/// By default, allows the most common standard at-rules:
/// `@media`, `@import`, `@keyframes`, `@font-face`, `@supports`, `@layer`,
/// `@charset`, `@namespace`, `@page`, `@property`, `@container`.
///
/// Equivalent to Stylelint's `at-rule-allowed-list` rule.
pub struct AtRuleAllowedList;

const ALLOWED: &[&str] = &[
  "charset",
  "container",
  "font-face",
  "import",
  "keyframes",
  "layer",
  "media",
  "namespace",
  "page",
  "property",
  "supports",
];

impl Rule for AtRuleAllowedList {
  fn name(&self) -> &'static str {
    "at-rule-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };
    if ALLOWED.contains(&at_rule.name.as_str()) {
      vec![]
    } else {
      vec![
        Diagnostic::new(
          self.name(),
          format!("Unexpected at-rule \"@{name}\"", name = at_rule.name),
        )
        .severity(self.default_severity())
        .span(Span::new(at_rule.span.offset, at_rule.span.length)),
      ]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn at_rule_node(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn allows_standard_at_rules() {
    assert!(
      AtRuleAllowedList
        .check(&at_rule_node("media"), &ctx())
        .is_empty()
    );
    assert!(
      AtRuleAllowedList
        .check(&at_rule_node("import"), &ctx())
        .is_empty()
    );
    assert!(
      AtRuleAllowedList
        .check(&at_rule_node("keyframes"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_disallowed_at_rule() {
    let d = AtRuleAllowedList.check(&at_rule_node("apply"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@apply"));
  }

  #[test]
  fn case_sensitive() {
    let d = AtRuleAllowedList.check(&at_rule_node("Media"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@Media"));
  }

  #[test]
  fn vendor_prefixed_not_matched_by_unprefixed() {
    let d = AtRuleAllowedList.check(&at_rule_node("-webkit-keyframes"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@-webkit-keyframes"));
  }
}
