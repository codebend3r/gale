use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "at-rule-disallowed-list";

declare_rule! {
  /// Disallow the at-rules listed.
  ///
  /// Options: an at-rule name (without the `@`) or `/regex/`, or a list of
  /// them, matched exactly as written, as Stylelint 17 does.  At-rules that
  /// are not standard syntax (`@charset`, Less mixins, SCSS `@content`)
  /// are not checked.
  ///
  /// Equivalent to Stylelint's `at-rule-disallowed-list` rule.
  pub struct AtRuleDisallowedList => ListCheck<AtRuleDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed at-rules",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for AtRuleDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
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

  use super::NAME;
  use crate::list_rules::testing::{check, check_css};
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_at_rules() {
    check_css(
      NAME,
      &[
        (
          json!(["extend", "media"]),
          "@media print {}\n@supports (a: b) {}",
          &["1:1 Unexpected at-rule \"@media\""],
        ),
        (json!(["media"]), "@MEDIA print {}", &[]),
        (json!(["keyframes"]), "@-webkit-keyframes a {}", &[]),
        (
          json!("/keyframes$/"),
          "@-webkit-keyframes a {}",
          &["1:1 Unexpected at-rule \"@-webkit-keyframes\""],
        ),
      ],
    );
    check(
      NAME,
      gale_css_parser::Syntax::Scss,
      &[(
        json!(["extend"]),
        "a {\n  @extend %b;\n}",
        &["2:3 Unexpected at-rule \"@extend\""],
      )],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([["extend", false]])),
      ["Invalid option value \"false\" for rule \"at-rule-disallowed-list\""]
    );
    assert!(invalid_options(NAME, json!("extend")).is_empty());
  }
}
