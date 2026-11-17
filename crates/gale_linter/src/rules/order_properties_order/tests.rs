use super::*;
use gale_css_parser::Syntax;
use serde_json::json;

use crate::testing::{CheckSource, configured, ctx_with_source, fix, invalid_options};

const RULE: &str = "order/properties-order";

/// The messages for `source` with `options`.
fn messages(source: &str, options: serde_json::Value) -> Vec<String> {
  let ctx = ctx_with_source(source);
  configured(&OrderPropertiesOrder, options)
    .check_source(&ctx)
    .into_iter()
    .map(|d| d.message)
    .collect()
}

#[test]
fn reports_properties_out_of_order() {
  let order = json!([[
    "my",
    "transform",
    "font-smoothing",
    "top",
    "transition",
    "border",
    "color"
  ]]);
  assert_eq!(
    messages("a { color: pink; top: 0; }", order.clone()),
    vec!["Expected \"top\" to come before \"color\""]
  );
  assert!(messages("a { top: 0; color: pink; }", order.clone()).is_empty());
  assert!(
    messages(
      "a { -webkit-transform: none; transform: none; }",
      order.clone()
    )
    .is_empty()
  );
  assert_eq!(
    messages("a { transform: none; -webkit-transform: none; }", order).len(),
    1
  );
  let grouped =
    json!([[{ "groupName": "font", "properties": ["font-size", "font-weight"] }, "height"]]);
  assert_eq!(
    messages("a { height: 1px; font-size: 2px; }", grouped),
    vec!["Expected \"font-size\" to come before \"height\" in group \"font\""]
  );
}

#[test]
fn an_order_naming_no_property_checks_nothing() {
  assert!(messages("a { top: 0; color: pink; }", json!([[]])).is_empty());
  assert!(invalid_options(RULE, json!([[]])).is_empty());
}

#[test]
fn options_are_validated_like_stylelint() {
  assert!(
    invalid_options(
      RULE,
      json!([["color"], {
        "unspecified": "bottomAlphabetical",
        "emptyLineBeforeUnspecified": "threshold",
        "emptyLineMinimumPropertyThreshold": 4
      }])
    )
    .is_empty()
  );
  assert_eq!(
    invalid_options(RULE, json!([[{ "properties": "color" }]])),
    ["Invalid option \"[{\"properties\":\"color\"}]\" for rule \"order/properties-order\""]
  );
  assert_eq!(
    invalid_options(
      RULE,
      json!([["color"], { "emptyLineMinimumPropertyThreshold": "4" }])
    ),
    [
      "Invalid value \"4\" for option \"emptyLineMinimumPropertyThreshold\" of rule \"order/properties-order\""
    ]
  );
}

#[test]
fn fix_sorts_and_keeps_comments_with_their_declarations() {
  let order = json!([["height", "width", "color"]]);
  assert_eq!(
    fix(
      RULE,
      order.clone(),
      "a { color: pink; width: 1px; height: 2px }",
      Syntax::Css
    ),
    "a { height: 2px; width: 1px; color: pink }"
  );
  assert_eq!(
    fix(
      RULE,
      order.clone(),
      "a {\n  /* c */\n  color: pink;\n  width: 1px; /* w */\n  top: 0;\n}",
      Syntax::Css
    ),
    "a {\n  width: 1px; /* w */\n  /* c */\n  color: pink;\n  top: 0;\n}"
  );
  assert_eq!(
    fix(
      RULE,
      json!([["transform"]]),
      "a { -moz-transform: none; transform: none; -webkit-transform: none; }",
      Syntax::Css
    ),
    "a { -moz-transform: none; -webkit-transform: none; transform: none; }"
  );
  // Sass control blocks are reported but never sorted.
  let ctx_source = "@if $a { color: pink; height: 1px; }";
  assert_eq!(
    fix("order/properties-order", order, ctx_source, Syntax::Scss),
    ctx_source
  );
}

#[test]
fn fix_sorts_then_fixes_empty_lines_between_groups() {
  let groups = json!([[
    { "emptyLineBefore": "always", "properties": ["width", "height"] },
    { "emptyLineBefore": "always", "properties": ["font-size", "font-family"] }
  ]]);
  assert_eq!(
    fix(
      RULE,
      groups,
      "a {\n  width: 1px;\n  font-size: 2px;\n  height: 3px;\n}",
      Syntax::Css
    ),
    "a {\n  width: 1px;\n  height: 3px;\n\n  font-size: 2px;\n}"
  );
  let unspecified = json!([["height", "width"], { "unspecified": "bottom", "emptyLineBeforeUnspecified": "always" }]);
  assert_eq!(
    fix(
      RULE,
      unspecified,
      "a {\r\n  height: 1px;\r\n  color: red;\r\n}",
      Syntax::Css
    ),
    "a {\r\n  height: 1px;\r\n\r\n  color: red;\r\n}"
  );
}

#[test]
fn unspecified_positions() {
  assert_eq!(
    fix(
      RULE,
      json!([["height"], { "unspecified": "top" }]),
      "a { height: 1px; top: 0; }",
      Syntax::Css
    ),
    "a { top: 0; height: 1px; }"
  );
  assert_eq!(
    fix(
      RULE,
      json!([["height"], { "unspecified": "bottom" }]),
      "a { bottom: 0; height: 1px; }",
      Syntax::Css
    ),
    "a { height: 1px; bottom: 0; }"
  );
  assert_eq!(
    fix(
      RULE,
      json!([["all", "compose"], { "unspecified": "bottomAlphabetical" }]),
      "a { compose: b; top: 0; bottom: 0; }",
      Syntax::Css
    ),
    "a { compose: b; bottom: 0; top: 0; }"
  );
}

#[test]
fn empty_line_helpers_match_stylelint_order() {
  assert_eq!(add_empty_line_before(" ", "\n"), "\n\n ");
  assert_eq!(add_empty_line_before("\n  ", "\n"), "\n\n  ");
  assert_eq!(add_empty_line_before(";\n", "\n"), ";\n\n");
  assert_eq!(add_empty_line_before(";\n  ", "\n"), ";\n\n  ");
  assert_eq!(remove_empty_lines("\n\n\n  ", "\n"), "\n  ");
  assert_eq!(remove_empty_lines("\r\n \r\n\r\n  ", "\r\n"), "\r\n  ");
  assert!(has_order_empty_line("\n \n"));
  assert!(!has_order_empty_line("\n  "));
}
