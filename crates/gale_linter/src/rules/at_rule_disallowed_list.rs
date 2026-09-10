use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specified at-rules.
///
/// Options: an array of at-rule names (without the `@` prefix) that are disallowed.
/// By default, no at-rules are disallowed.
///
/// Equivalent to Stylelint's `at-rule-disallowed-list` rule.
pub struct AtRuleDisallowedList;

impl Rule for AtRuleDisallowedList {
  fn name(&self) -> &'static str {
    "at-rule-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };

    let disallowed: Vec<String> = match ctx.options {
      Some(serde_json::Value::Array(arr)) => arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect(),
      _ => return vec![],
    };

    if disallowed.iter().any(|d| d == &at_rule.name) {
      vec![
        Diagnostic::new(
          self.name(),
          format!("Unexpected at-rule \"@{}\"", at_rule.name),
        )
        .severity(self.default_severity())
        .span(Span::new(at_rule.span.offset, at_rule.span.length)),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn ctx_with_options(options: Option<serde_json::Value>) -> RuleContext<'static> {
    // Leak the value so we get a `'static` reference for tests.
    let opts: Option<&'static serde_json::Value> = options.map(|v| &*Box::leak(Box::new(v)));
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: opts,
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
  fn allows_all_when_no_options() {
    let ctx = ctx_with_options(None);
    let d = AtRuleDisallowedList.check(&at_rule_node("media"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_disallowed_at_rule() {
    let ctx = ctx_with_options(Some(serde_json::json!(["extend", "debug"])));
    let d = AtRuleDisallowedList.check(&at_rule_node("extend"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@extend"));
  }

  #[test]
  fn allows_at_rule_not_in_list() {
    let ctx = ctx_with_options(Some(serde_json::json!(["extend"])));
    let d = AtRuleDisallowedList.check(&at_rule_node("media"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn case_sensitive() {
    let ctx = ctx_with_options(Some(serde_json::json!(["extend"])));
    let d = AtRuleDisallowedList.check(&at_rule_node("Extend"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn vendor_prefixed_not_matched_by_unprefixed() {
    let ctx = ctx_with_options(Some(serde_json::json!(["keyframes"])));
    let d = AtRuleDisallowedList.check(&at_rule_node("-webkit-keyframes"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(AtRuleDisallowedList.name(), "at-rule-disallowed-list");
  }
}
