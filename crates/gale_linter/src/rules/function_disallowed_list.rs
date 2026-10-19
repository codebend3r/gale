use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "function-disallowed-list";

declare_rule! {
  /// Disallow the functions listed.
  ///
  /// Options: a function name or `/regex/`, or a list of them, matched
  /// exactly as written, as Stylelint 17 does.
  ///
  /// Equivalent to Stylelint's `function-disallowed-list` rule.
  pub struct FunctionDisallowedList => ListCheck<FunctionDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed functions",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for FunctionDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::function_names(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected function \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_functions() {
    check_css(
      NAME,
      &[
        (
          json!(["rgba"]),
          "a { color: rgba(0, 0, 0, 0); }",
          &["1:12 Unexpected function \"rgba\""],
        ),
        (
          json!("/^translate/"),
          "a { transform: scale(2) translateX(1px); }",
          &["1:25 Unexpected function \"translateX\""],
        ),
        (
          json!(["var"]),
          "a { width: calc(var(--a) * 2); }",
          &["1:17 Unexpected function \"var\""],
        ),
        (json!(["rgba"]), "a { color: RGBA(0, 0, 0, 0); }", &[]),
        (json!(["rgba"]), "a { color: red; }", &[]),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([["rgba", null]])),
      ["Invalid option value \"null\" for rule \"function-disallowed-list\""]
    );
    assert!(invalid_options(NAME, json!([["rgba"], { "anything": 1 }])).is_empty());
  }
}
