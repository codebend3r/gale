use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "at-rule-allowed-list";

declare_rule! {
  /// Only allow the at-rules listed.
  ///
  /// Options: an at-rule name (without the `@`) or `/regex/`, or a list of
  /// them.  Names are matched exactly as written, as Stylelint 17 does, so
  /// `@MEDIA` and `@-webkit-keyframes` need entries of their own.  At-rules
  /// that are not standard syntax (`@charset`, Less mixins, SCSS
  /// `@content`) are not checked.
  ///
  /// Equivalent to Stylelint's `at-rule-allowed-list` rule.
  pub struct AtRuleAllowedList => ListCheck<AtRuleAllowedList> {
    name: NAME,
    description: "Specify a list of allowed at-rules",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for AtRuleAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::at_rule_names(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected at-rule \"@{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::{AtRuleAllowedList, NAME};
  use crate::list_rules::testing::check_css;
  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn reports_at_rules_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["media"]),
          "@supports (display: grid) {}\n@media screen {}",
          &["1:1 Unexpected at-rule \"@supports\""],
        ),
        (json!("media"), "@media screen {}", &[]),
        (json!(["/^media$/i"]), "@MEDIA screen {}", &[]),
        (
          json!(["media"]),
          "@MEDIA screen {}",
          &["1:1 Unexpected at-rule \"@MEDIA\""],
        ),
        (
          json!(["keyframes"]),
          "@-webkit-keyframes a {}",
          &["1:1 Unexpected at-rule \"@-webkit-keyframes\""],
        ),
        (json!(["media"]), "@charset \"utf-8\";", &[]),
        (
          json!([["media"], { "severity": "warning" }]),
          "a { @import \"x\"; }",
          &["1:5 Unexpected at-rule \"@import\""],
        ),
      ],
    );
  }

  #[test]
  fn reports_nothing_without_a_list() {
    let check = configured(&AtRuleAllowedList, None);
    let ctx = ctx_with_source("@supports (display: grid) {}");
    assert!(check.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([1])),
      ["Invalid option value \"1\" for rule \"at-rule-allowed-list\""]
    );
    assert!(invalid_options(NAME, json!(["media", "/^font/"])).is_empty());
  }
}
