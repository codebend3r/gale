use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require specified at-rules to be nested within style rules.
///
/// Options: an array of at-rule names (without `@`) that must appear nested
/// inside a style rule, not at the top level.
///
/// Example config:
/// ```json
/// ["media", "supports"]
/// ```
///
/// Equivalent to Stylelint's hypothetical `rule-nesting-at-rule-required-list` rule.
pub struct RuleNestingAtRuleRequiredList;

impl Rule for RuleNestingAtRuleRequiredList {
  fn name(&self) -> &'static str {
    "rule-nesting-at-rule-required-list"
  }

  fn description(&self) -> &'static str {
    "Require specified at-rules to be nested within style rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let required = match parse_options(ctx.options) {
      Some(r) => r,
      None => return vec![],
    };

    let mut diags = Vec::new();

    // Only inspect top-level nodes. At-rules found here that are in the
    // required list should have been nested inside a style rule instead.
    for node in nodes {
      if let CssNode::AtRule(at_rule) = node {
        let name_lower = at_rule.name.to_ascii_lowercase();
        if required.contains(&name_lower) {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Expected at-rule \"@{name}\" to be nested within a style rule",
                name = at_rule.name,
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(at_rule.span.offset, at_rule.span.length)),
          );
        }
      }
    }

    diags
  }
}

/// Parse the rule options as `["at-rule-name", ...]`.
fn parse_options(options: Option<&serde_json::Value>) -> Option<Vec<String>> {
  let arr = options?.as_array()?;
  let names: Vec<String> = arr
    .iter()
    .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
    .collect();
  if names.is_empty() { None } else { Some(names) }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn top_level_at_rule(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    })
  }

  fn style_rule_node() -> CssNode {
    CssNode::Style(StyleRule {
      selector: ".foo".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    })
  }

  #[test]
  fn reports_top_level_at_rule_that_should_be_nested() {
    let opts = serde_json::json!(["media", "supports"]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let nodes = vec![top_level_at_rule("media"), style_rule_node()];
    let d = RuleNestingAtRuleRequiredList.check_root(&nodes, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@media"));
  }

  #[test]
  fn allows_at_rules_not_in_list() {
    let opts = serde_json::json!(["media"]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let nodes = vec![top_level_at_rule("keyframes"), style_rule_node()];
    let d = RuleNestingAtRuleRequiredList.check_root(&nodes, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_style_rules_at_top_level() {
    let opts = serde_json::json!(["media"]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let nodes = vec![style_rule_node()];
    let d = RuleNestingAtRuleRequiredList.check_root(&nodes, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn returns_empty_when_no_options() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let nodes = vec![top_level_at_rule("media")];
    let d = RuleNestingAtRuleRequiredList.check_root(&nodes, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn case_insensitive_matching() {
    let opts = serde_json::json!(["media"]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let nodes = vec![top_level_at_rule("Media")];
    let d = RuleNestingAtRuleRequiredList.check_root(&nodes, &ctx);
    assert_eq!(d.len(), 1);
  }
}
