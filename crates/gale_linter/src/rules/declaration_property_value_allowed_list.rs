use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{KeyedCheck, KeyedFound, KeyedListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "declaration-property-value-allowed-list";

declare_rule! {
  /// Only allow the values listed for a property.
  ///
  /// Options: an object mapping a property name or `/regex/` to the values
  /// allowed for it, each a string that must equal the whole value or a
  /// `/regex/` that need only match part of it.  Keys are matched against
  /// the property as written; a declaration is reported when every key
  /// that matches it rejects its value.
  ///
  /// Equivalent to Stylelint's `declaration-property-value-allowed-list` rule.
  pub struct DeclarationPropertyValueAllowedList => KeyedCheck<DeclarationPropertyValueAllowedList> {
    name: NAME,
    description: "Specify a list of allowed property and value pairs within declarations",
    default_severity: Warning,
    primary_option_object: true,
  }
}

impl KeyedListRule for DeclarationPropertyValueAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  const EVERY_KEY: bool = true;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<KeyedFound>) {
    subjects::declaration_values(tree, found);
  }

  fn message(found: &KeyedFound) -> String {
    format!(
      "Unexpected value \"{}\" for property \"{}\"",
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
  fn reports_values_missing_from_the_property_lists() {
    check_css(
      NAME,
      &[
        (
          json!({ "display": ["block", "flex"] }),
          "a { display: grid; color: red; }\nb { display: flex; }",
          &["1:14 Unexpected value \"grid\" for property \"display\""],
        ),
        (
          json!({ "/^border/": ["none"], "border-color": ["/var\\(/"] }),
          "a { border-color: var(--c); border-top: 0; }",
          &["1:41 Unexpected value \"0\" for property \"border-top\""],
        ),
        (
          json!({ "color": ["/^#/"] }),
          "a { color: #FFF !important; }",
          &[],
        ),
        (
          json!({ "display": ["flex"] }),
          "a { -webkit-display: box; }",
          &[],
        ),
        (
          json!({ "color": "red" }),
          "a { color: /* x */ blue; }",
          &["1:20 Unexpected value \"blue\" for property \"color\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!({ "display": [{}] })),
      [
        "Invalid option value \"{\"display\":[{}]}\" for rule \"declaration-property-value-allowed-list\""
      ]
    );
    assert!(invalid_options(NAME, json!({ "display": ["/^flex/", "block"] })).is_empty());
  }
}
