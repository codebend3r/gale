use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Entries, Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-attribute-operator-allowed-list";

declare_rule! {
  /// Only allow the attribute selector operators listed.
  ///
  /// Options: an operator (`=`, `~=`, `|=`, `^=`, `$=`, `*=`) or a list of
  /// them; entries are compared literally.
  ///
  /// Equivalent to Stylelint's `selector-attribute-operator-allowed-list` rule.
  pub struct SelectorAttributeOperatorAllowedList => ListCheck<SelectorAttributeOperatorAllowedList> {
    name: NAME,
    description: "Specify a list of allowed attribute operators",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorAttributeOperatorAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
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
  fn reports_operators_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["="]),
          "[a=\"x\"], [b*=\"y\"], [c] {}",
          &["1:12 Unexpected operator \"*=\""],
        ),
        (json!("~="), "[class~=x] {}", &[]),
        (
          json!(["="]),
          "[ lang |= en ] {}",
          &["1:8 Unexpected operator \"|=\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([["=", 1]])),
      ["Invalid option value \"1\" for rule \"selector-attribute-operator-allowed-list\""]
    );
  }
}
