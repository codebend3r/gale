use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-pseudo-element-disallowed-list";

declare_rule! {
  /// Disallow the pseudo-elements listed.
  ///
  /// Options: a pseudo-element name (without the colons) or `/regex/`, or
  /// a list of them, matched exactly as written.  Only the double-colon
  /// form is checked: `a:before` is a pseudo-class to Stylelint.
  ///
  /// Equivalent to Stylelint's `selector-pseudo-element-disallowed-list` rule.
  pub struct SelectorPseudoElementDisallowedList => ListCheck<SelectorPseudoElementDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed pseudo-element selectors",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorPseudoElementDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::pseudo_elements(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected pseudo-element \"::{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_pseudo_elements() {
    check_css(
      NAME,
      &[
        (
          json!(["before"]),
          "a::before, a::after {}",
          &["1:2 Unexpected pseudo-element \"::before\""],
        ),
        (
          json!(["/^-webkit-/"]),
          "::-webkit-scrollbar {}",
          &["1:1 Unexpected pseudo-element \"::-webkit-scrollbar\""],
        ),
        (json!(["before"]), "a:before {}", &[]),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!("before")).is_empty());
  }
}
