use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Entries, Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-attribute-operator-disallowed-list";

declare_rule! {
  /// Disallow the attribute selector operators listed.
  ///
  /// Options: an operator (`=`, `~=`, `|=`, `^=`, `$=`, `*=`) or a list of
  /// them; entries are compared literally.
  ///
  /// Equivalent to Stylelint's `selector-attribute-operator-disallowed-list` rule.
  pub struct SelectorAttributeOperatorDisallowedList => ListCheck<SelectorAttributeOperatorDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed attribute operators",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorAttributeOperatorDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  const ENTRIES: Entries = Entries::Names;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::attribute_operators(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected operator \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_operators() {
    check_css(
      NAME,
      &[
        (
          json!(["*="]),
          "[a=\"x\"], [b*=\"y\"] {}",
          &["1:12 Unexpected operator \"*=\""],
        ),
        (json!(["/=/"]), "[a=x] {}", &[]),
        (json!(["*="]), "a {}", &[]),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!("*=")).is_empty());
  }
}
