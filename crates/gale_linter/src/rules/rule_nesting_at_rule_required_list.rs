use gale_css_parser::postcss_tree::{NodeRef, PostcssTree};
use gale_diagnostics::Diagnostic;

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
  /// Flags a listed at-rule that sits at the top level rather than nested
  /// inside a style rule.  Names are read as written, vendor prefix
  /// included, in every syntax.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    if self.required.is_empty() {
      return vec![];
    }
    tree
      .root_nodes()
      .filter_map(NodeRef::as_at_rule)
      .filter(|at| self.is_required(at.name()))
      .map(|at| {
        Diagnostic::new(
          NAME,
          format!(
            "Expected at-rule \"@{name}\" to be nested within a style rule",
            name = at.name(),
          ),
        )
        .span(at.span())
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source` with `options`: each message and
  /// the text it points at.
  fn check(source: &str, options: Option<serde_json::Value>) -> Vec<(String, &str)> {
    configured(&RuleNestingAtRuleRequiredList, options)
      .check_source(&context(source, Syntax::Scss))
      .into_iter()
      .map(|d| {
        (
          d.message,
          &source[d.span.offset..d.span.offset + d.span.length],
        )
      })
      .collect()
  }

  #[test]
  fn reports_top_level_at_rule_that_should_be_nested() {
    let options = serde_json::json!(["media", "supports"]);
    let source = "@media print { a {} }\n.foo { color: red; @supports (x: y) {} }";
    assert_eq!(
      check(source, Some(options)),
      [(
        "Expected at-rule \"@media\" to be nested within a style rule".to_string(),
        "@media print { a {} }"
      )]
    );
  }

  #[test]
  fn reports_blockless_at_rules_by_their_own_text() {
    let options = serde_json::json!(["import"]);
    let source = "@import \"x\";\na { b: c }";
    assert_eq!(
      check(source, Some(options)),
      [(
        "Expected at-rule \"@import\" to be nested within a style rule".to_string(),
        "@import \"x\";"
      )]
    );
  }

  #[test]
  fn allows_at_rules_not_in_list() {
    let options = serde_json::json!(["media"]);
    assert!(check("@keyframes k {} .foo { color: red }", Some(options)).is_empty());
  }

  #[test]
  fn allows_style_rules_at_top_level() {
    assert!(check(".foo { color: red }", Some(serde_json::json!(["media"]))).is_empty());
  }

  #[test]
  fn returns_empty_when_no_options() {
    assert!(check("@media print {}", None).is_empty());
  }

  #[test]
  fn case_insensitive_matching() {
    assert_eq!(
      check("@Media print {}", Some(serde_json::json!(["media"]))).len(),
      1
    );
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
    let source = "@supports (x: y) {} .foo { color: red }";
    assert_eq!(check(source, Some(serde_json::json!("supports"))).len(), 1);
    assert_eq!(check(source, Some(serde_json::json!(["/^supp/"]))).len(), 1);
  }
}
