use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-pseudo-class-disallowed-list";

declare_rule! {
  /// Disallow the pseudo-classes listed.
  ///
  /// Options: a pseudo-class name (without the colon) or `/regex/`, or a
  /// list of them, matched exactly as written, as Stylelint 17 does.
  /// Selectors in `@page` are checked too.
  ///
  /// Equivalent to Stylelint's `selector-pseudo-class-disallowed-list` rule.
  pub struct SelectorPseudoClassDisallowedList => ListCheck<SelectorPseudoClassDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed pseudo-class selectors",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorPseudoClassDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::pseudo_classes(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected pseudo-class \":{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_pseudo_classes() {
    check_css(
      NAME,
      &[
        (
          json!(["hover"]),
          "a:hover, b:focus {}",
          &["1:2 Unexpected pseudo-class \":hover\""],
        ),
        (
          json!(["/^nth-/"]),
          "a:not(:nth-child(2)) {}",
          &["1:7 Unexpected pseudo-class \":nth-child\""],
        ),
        (json!(["before"]), "a::before {}", &[]),
        (json!(["hover"]), "a:HOVER {}", &[]),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!("hover")).is_empty());
  }
}
