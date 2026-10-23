use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{KeyedCheck, KeyedFound, KeyedListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "declaration-property-value-disallowed-list";

declare_rule! {
  /// Disallow the values listed for a property.
  ///
  /// Options: an object mapping a property name or `/regex/` to the values
  /// disallowed for it, each a string that must equal the whole value or a
  /// `/regex/` that need only match part of it.  Keys are matched against
  /// the property as written; a declaration is reported when any key that
  /// matches it lists its value.
  ///
  /// Equivalent to Stylelint's `declaration-property-value-disallowed-list` rule.
  pub struct DeclarationPropertyValueDisallowedList => KeyedCheck<DeclarationPropertyValueDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed property and value pairs within declarations",
    default_severity: Warning,
    primary_option_object: true,
  }
}

impl KeyedListRule for DeclarationPropertyValueDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
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
  use crate::list_rules::testing::{check, check_css};
  use crate::testing::invalid_options;

  #[test]
  fn reports_values_listed_for_the_property() {
    check_css(
      NAME,
      &[
        (
          json!({ "/^border/": ["none"], "/^transition/": ["/all/"] }),
          "a { border-top: none; transition: all 1s; border: 0; }",
          &[
            "1:17 Unexpected value \"none\" for property \"border-top\"",
            "1:35 Unexpected value \"all 1s\" for property \"transition\"",
          ],
        ),
        (
          json!({ "display": ["none"] }),
          "a { display: none !important; }",
          &["1:14 Unexpected value \"none\" for property \"display\""],
        ),
        (json!({ "display": ["none"] }), "a { DISPLAY: none; }", &[]),
        (
          json!({ "display": "none" }),
          "a:has(b:hover) { color: red; }",
          &[],
        ),
      ],
    );
    check(
      NAME,
      gale_css_parser::Syntax::Scss,
      &[(
        json!({ "color": ["red"] }),
        "a {\n  &:hover { color: red; }\n}",
        &["2:20 Unexpected value \"red\" for property \"color\""],
      )],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!({ "/^border/": ["none"] })).is_empty());
  }
}
