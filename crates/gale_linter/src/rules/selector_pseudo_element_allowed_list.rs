use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-pseudo-element-allowed-list";

declare_rule! {
  /// Only allow the pseudo-elements listed.
  ///
  /// Options: a pseudo-element name (without the colons) or `/regex/`, or
  /// a list of them, matched exactly as written.  Only the double-colon
  /// form is checked: `a:before` is a pseudo-class to Stylelint.
  ///
  /// Equivalent to Stylelint's `selector-pseudo-element-allowed-list` rule.
  pub struct SelectorPseudoElementAllowedList => ListCheck<SelectorPseudoElementAllowedList> {
    name: NAME,
    description: "Specify a list of allowed pseudo-element selectors",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorPseudoElementAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
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
  fn reports_pseudo_elements_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["before"]),
          "a::before, a::after {}",
          &["1:13 Unexpected pseudo-element \"::after\""],
        ),
        (json!(["before"]), "a:after {}", &[]),
        (json!(["/^-moz-/"]), "input::-moz-placeholder {}", &[]),
        (
          json!(["before"]),
          "a::BEFORE {}",
          &["1:2 Unexpected pseudo-element \"::BEFORE\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!(["before"])).is_empty());
  }
}
