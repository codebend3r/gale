use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{KeyedCheck, KeyedFound, KeyedListRule, Polarity, subjects};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "rule-selector-property-disallowed-list";

declare_rule! {
  /// Disallow the properties listed for rules with a selector.
  ///
  /// Options: an object mapping a selector or `/regex/` to the properties
  /// (names or `/regex/`) disallowed in rules with that selector.  The first
  /// key that matches the rule's whole selector applies to the declarations
  /// directly in the rule, not those in rules nested inside it.
  /// `ignore: ["keyframe-selectors"]` skips the selectors in `@keyframes`.
  ///
  /// Equivalent to Stylelint's `rule-selector-property-disallowed-list` rule.
  pub struct RuleSelectorPropertyDisallowedList => KeyedCheck<RuleSelectorPropertyDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed properties for selectors within rules",
    default_severity: Warning,
    primary_option_object: true,
  }
}

impl KeyedListRule for RuleSelectorPropertyDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  /// `ignore: ["keyframe-selectors"]`.
  type Options = bool;

  fn options(options: &mut Options<'_>) -> bool {
    !options
      .keywords("ignore", &["keyframe-selectors"])
      .is_empty()
  }

  fn find(
    tree: &PostcssTree,
    _: &RuleContext,
    ignore_keyframes: &bool,
    found: &mut Vec<KeyedFound>,
  ) {
    subjects::rule_properties(tree, *ignore_keyframes, found);
  }

  fn message(found: &KeyedFound) -> String {
    format!(
      "Unexpected property \"{}\" for selector \"{}\"",
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
  fn reports_properties_listed_for_the_selector() {
    check_css(
      NAME,
      &[
        (
          json!({ "/\\.foo/": ["color", "/^margin/"], "a": ["padding"] }),
          "a.foo { color: red; margin-top: 0; }\na { padding: 0; }",
          &[
            "1:9 Unexpected property \"color\" for selector \"a.foo\"",
            "1:21 Unexpected property \"margin-top\" for selector \"a.foo\"",
            "2:5 Unexpected property \"padding\" for selector \"a\"",
          ],
        ),
        (json!({ "a": ["color"] }), "a { COLOR: red; }", &[]),
        (json!({ "a": ["color"] }), "a, b { color: red; }", &[]),
        (
          json!([{ "from": ["color"] }, { "ignore": ["keyframe-selectors"] }]),
          "@keyframes k { from { color: red; } }",
          &[],
        ),
      ],
    );
    check(
      NAME,
      gale_css_parser::Syntax::Scss,
      &[(
        json!({ "a": ["color"] }),
        "a {\n  @media print { color: red; }\n  b { color: blue; }\n}",
        &["2:18 Unexpected property \"color\" for selector \"a\""],
      )],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([{ "a": ["color"] }, { "ignore": "nested" }])),
      [
        "Invalid value \"nested\" for option \"ignore\" of rule \"rule-selector-property-disallowed-list\""
      ]
    );
  }
}
