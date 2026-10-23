use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-attribute-name-disallowed-list";

declare_rule! {
  /// Disallow the attribute names listed in attribute selectors.
  ///
  /// Options: an attribute name or `/regex/`, or a list of them, matched
  /// exactly as written against the name with any namespace (`ns|name`).
  ///
  /// Equivalent to Stylelint's `selector-attribute-name-disallowed-list` rule.
  pub struct SelectorAttributeNameDisallowedList => ListCheck<SelectorAttributeNameDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed attribute names",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorAttributeNameDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::attribute_names(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected attribute name \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_attribute_names() {
    check_css(
      NAME,
      &[
        (
          json!(["class", "id"]),
          "[class~=\"x\"], a[ id ] {}",
          &[
            "1:2 Unexpected attribute name \"class\"",
            "1:18 Unexpected attribute name \"id\"",
          ],
        ),
        (
          json!(["/^data-/"]),
          "a:not([data-x]) {}",
          &["1:8 Unexpected attribute name \"data-x\""],
        ),
        (json!(["class"]), "[CLASS] {}", &[]),
        (
          json!(["svg|href"]),
          "[svg|href] {}",
          &["1:6 Unexpected attribute name \"svg|href\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!(["class"])).is_empty());
  }
}
