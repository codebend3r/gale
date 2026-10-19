use std::borrow::Cow;

use crate::list_rules::{Entries, KeyedCheck, KeyedFound, KeyedListRule, Polarity, subjects};
use crate::options::Options;
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "declaration-property-unit-allowed-list";

declare_rule! {
  /// Only allow the units listed for a property.
  ///
  /// Options: an object mapping a property name or `/regex/` to the units
  /// allowed in its value; the first key that matches the property applies.
  /// Units are compared lowercased; those inside `url()`, and with
  /// `ignore: ["inside-function"]` those inside any function, are not
  /// checked.
  ///
  /// Equivalent to Stylelint's `declaration-property-unit-allowed-list` rule.
  pub struct DeclarationPropertyUnitAllowedList => KeyedCheck<DeclarationPropertyUnitAllowedList> {
    name: NAME,
    description: "Specify a list of allowed property and unit pairs within declarations",
    default_severity: Warning,
    primary_option_object: true,
  }
}

impl KeyedListRule for DeclarationPropertyUnitAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  const ENTRIES: Entries = Entries::Names;
  /// `ignore: ["inside-function"]`.
  type Options = bool;

  fn options(options: &mut Options<'_>) -> bool {
    !options.keywords("ignore", &["inside-function"]).is_empty()
  }

  fn find(
    tree: &PostcssTree,
    _: &RuleContext,
    inside_function: &bool,
    found: &mut Vec<KeyedFound>,
  ) {
    subjects::declaration_units(tree, *inside_function, found);
  }

  fn normalize(unit: &str) -> Cow<'_, str> {
    Cow::Owned(unit.to_ascii_lowercase())
  }

  fn message(found: &KeyedFound) -> String {
    format!(
      "Unexpected unit \"{}\" for property \"{}\"",
      found.name, found.key
    )
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_units_missing_from_the_property_list() {
    check_css(
      NAME,
      &[
        (
          json!({ "font-size": ["rem", "em"] }),
          "a { font-size: 12px; margin: 1px; }",
          &["1:16 Unexpected unit \"px\" for property \"font-size\""],
        ),
        (
          json!({ "/^margin/": ["PX"] }),
          "a { margin-top: 1PX; }",
          &["1:17 Unexpected unit \"PX\" for property \"margin-top\""],
        ),
        (
          json!({ "/^margin/": "px" }),
          "a { margin-top: 1PX 0; }",
          &[],
        ),
        (
          json!({ "width": ["px"] }),
          "a { width: calc(100% - 1em); background: url(a-1em.png); }",
          &[
            "1:17 Unexpected unit \"%\" for property \"width\"",
            "1:24 Unexpected unit \"em\" for property \"width\"",
          ],
        ),
        (
          json!([{ "width": ["px"] }, { "ignore": ["inside-function"] }]),
          "a { width: calc(100% - 1em); }",
          &[],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!({ "width": [1] })),
      [
        "Invalid option value \"{\"width\":[1]}\" for rule \"declaration-property-unit-allowed-list\""
      ]
    );
    assert_eq!(
      invalid_options(
        NAME,
        json!([{ "width": ["px"] }, { "ignore": ["inside-url"] }])
      ),
      [
        "Invalid value \"inside-url\" for option \"ignore\" of rule \"declaration-property-unit-allowed-list\""
      ]
    );
  }
}
