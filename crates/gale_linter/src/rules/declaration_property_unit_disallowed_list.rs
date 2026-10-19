use std::borrow::Cow;

use crate::list_rules::{Entries, KeyedCheck, KeyedFound, KeyedListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "declaration-property-unit-disallowed-list";

declare_rule! {
  /// Disallow the units listed for a property.
  ///
  /// Options: an object mapping a property name or `/regex/` to the units
  /// disallowed in its value; the first key that matches the property
  /// applies.  Units are compared lowercased; those inside `url()` are not
  /// checked.
  ///
  /// Equivalent to Stylelint's `declaration-property-unit-disallowed-list` rule.
  pub struct DeclarationPropertyUnitDisallowedList => KeyedCheck<DeclarationPropertyUnitDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed property and unit pairs within declarations",
    default_severity: Warning,
    primary_option_object: true,
  }
}

impl KeyedListRule for DeclarationPropertyUnitDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  const ENTRIES: Entries = Entries::Names;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<KeyedFound>) {
    subjects::declaration_units(tree, false, found);
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
  fn reports_units_listed_for_the_property() {
    check_css(
      NAME,
      &[
        (
          json!({ "font-size": ["px"], "/^animation/": ["s"] }),
          "a { font-size: 12PX; animation: x 2s 300ms; }",
          &[
            "1:16 Unexpected unit \"PX\" for property \"font-size\"",
            "1:35 Unexpected unit \"s\" for property \"animation\"",
          ],
        ),
        (
          json!({ "width": ["em"] }),
          "a { width: 1px; height: 1em; }",
          &[],
        ),
        (
          json!({ "background": ["px"] }),
          "a { background: url(a-1px.png) 2px; }",
          &["1:32 Unexpected unit \"px\" for property \"background\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!(["width"])),
      ["Invalid option value \"width\" for rule \"declaration-property-unit-disallowed-list\""]
    );
    assert!(invalid_options(NAME, json!([{ "width": "px" }, { "anything": true }])).is_empty());
  }
}
