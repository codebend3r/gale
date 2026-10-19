use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::pattern;
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "rule-nesting-at-rule-required-list";

declare_rule! {
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
  pub struct RuleNestingAtRuleRequiredList => RuleNestingAtRuleRequiredListCheck {
    name: NAME,
    description: "Require specified at-rules to be nested within style rules",
    default_severity: Warning,
    primary_option_array: true,
  }
}

/// `rule-nesting-at-rule-required-list` as configured.
pub struct RuleNestingAtRuleRequiredListCheck {
  /// The at-rule names (lowercase) and `/regex/` entries; the rule checks
  /// nothing without any.
  required: Vec<String>,
}

impl RuleNestingAtRuleRequiredListCheck {
  /// Stylelint's options: a name or `/regex/` entry, or a list of them.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let required = options.primary_strings().unwrap_or_default();
    options.finish()?;
    Ok(Self {
      required: required
        .into_iter()
        .map(|entry| {
          if pattern::split_literal(&entry).is_some() {
            entry
          } else {
            entry.to_ascii_lowercase()
          }
        })
        .collect(),
    })
  }

  /// Whether the at-rule `name` is required to be nested: a `/regex/` entry
  /// matches by search, a name in any case.
  fn is_required(&self, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    self
      .required
      .iter()
      .any(|entry| pattern::match_regex_entry(entry, name).unwrap_or_else(|| *entry == lower))
  }
}

impl Check for RuleNestingAtRuleRequiredListCheck {
  /// Flags a listed at-rule that sits at the top level rather than nested inside
  /// a style rule.
  fn check_root(&self, nodes: &[CssNode], _ctx: &RuleContext) -> Vec<Diagnostic> {
    if self.required.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();

    // Only inspect top-level nodes. At-rules found here that are in the
    // required list should have been nested inside a style rule instead.
    for node in nodes {
      if let CssNode::AtRule(at_rule) = node {
        if self.is_required(&at_rule.name) {
          diags.push(
            Diagnostic::new(
              NAME,
              format!(
                "Expected at-rule \"@{name}\" to be nested within a style rule",
                name = at_rule.name,
              ),
            )
            .span(Span::new(at_rule.span.offset, at_rule.span.length)),
          );
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx};

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
    let rule = configured(&RuleNestingAtRuleRequiredList, opts.clone());
    let ctx = ctx();
    let nodes = vec![top_level_at_rule("media"), style_rule_node()];
    let d = rule.check_root(&nodes, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@media"));
  }

  #[test]
  fn allows_at_rules_not_in_list() {
    let opts = serde_json::json!(["media"]);
    let rule = configured(&RuleNestingAtRuleRequiredList, opts.clone());
    let ctx = ctx();
    let nodes = vec![top_level_at_rule("keyframes"), style_rule_node()];
    let d = rule.check_root(&nodes, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_style_rules_at_top_level() {
    let opts = serde_json::json!(["media"]);
    let rule = configured(&RuleNestingAtRuleRequiredList, opts.clone());
    let ctx = ctx();
    let nodes = vec![style_rule_node()];
    let d = rule.check_root(&nodes, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn returns_empty_when_no_options() {
    let rule = configured(&RuleNestingAtRuleRequiredList, None);
    let ctx = ctx();
    let nodes = vec![top_level_at_rule("media")];
    let d = rule.check_root(&nodes, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn case_insensitive_matching() {
    let opts = serde_json::json!(["media"]);
    let rule = configured(&RuleNestingAtRuleRequiredList, opts.clone());
    let ctx = ctx();
    let nodes = vec![top_level_at_rule("Media")];
    let d = rule.check_root(&nodes, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options(NAME, serde_json::json!(["media", "/^supp/"])).is_empty());
    assert!(invalid_options(NAME, serde_json::json!([["media"], { "foo": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["media", 1])),
      ["Invalid option value \"1\" for rule \"rule-nesting-at-rule-required-list\""]
    );
  }

  #[test]
  fn takes_a_single_name_and_regexes() {
    let nodes = vec![top_level_at_rule("supports"), style_rule_node()];
    let single = configured(
      &RuleNestingAtRuleRequiredList,
      serde_json::json!("supports"),
    );
    assert_eq!(single.check_root(&nodes, &ctx()).len(), 1);
    let regex = configured(
      &RuleNestingAtRuleRequiredList,
      serde_json::json!(["/^supp/"]),
    );
    assert_eq!(regex.check_root(&nodes, &ctx()).len(), 1);
  }
}
