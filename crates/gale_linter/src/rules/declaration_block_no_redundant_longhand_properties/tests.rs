use gale_css_parser::Syntax;
use serde_json::json;

use crate::testing::{fix, invalid_options, lint};

const RULE: &str = "declaration-block-no-redundant-longhand-properties";

#[test]
fn combines_longhands_into_the_first_one() {
  assert_eq!(
    fix(
      RULE,
      json!([true]),
      "a {\n\tmargin-left: 40px;\n\tmargin-right: 10px;\n\tmargin-top: 20px;\n\tcolor: blue;\n\tmargin-bottom: 30px;\n}",
      Syntax::Css
    ),
    "a {\n\tmargin: 20px 10px 30px 40px;\n\tcolor: blue;\n}"
  );
  assert_eq!(
    fix(
      RULE,
      json!([true]),
      "a { top: 1px !important; right: 2px !important; bottom: 3px !important; left: 4px !important; }",
      Syntax::Css
    ),
    "a { inset: 1px 2px 3px 4px !important; }"
  );
}

#[test]
fn drops_the_semicolon_when_the_last_declaration_had_none() {
  assert_eq!(
    fix(
      RULE,
      json!([true]),
      "a { overflow-x: hidden; color: red; overflow-y: auto }",
      Syntax::Css
    ),
    "a { overflow: hidden auto; color: red }"
  );
}

#[test]
fn overlapping_shorthands_take_their_longhands_away() {
  assert_eq!(
    fix(
      RULE,
      json!([true]),
      "a { border-top-width: 1px; border-top-style: dotted; border-top-color: red; border-right-width: 2px; border-bottom-width: 3px; border-left-width: 4px; border-right-style: solid; border-bottom-style: dashed; border-left-style: double; border-right-color: green; border-bottom-color: blue; border-left-color: yellow; }",
      Syntax::Css
    ),
    "a { border-top: 1px dotted red; border-right: 2px solid green; border-bottom: 3px dashed blue; border-left: 4px double yellow; }"
  );
}

#[test]
fn custom_resolvers() {
  assert_eq!(
    fix(
      RULE,
      json!([true]),
      "a { transition-delay: 500ms,    1s; transition-duration: 250ms,2s; transition-timing-function: ease-in-out; transition-property: transform, visibility; }",
      Syntax::Css
    ),
    "a { transition: transform 250ms ease-in-out 500ms, visibility 2s ease-in-out 1s; }"
  );
  assert_eq!(
    fix(
      RULE,
      json!([true]),
      "a { grid-row-start: 1; grid-row-end: 2; }",
      Syntax::Css
    ),
    "a { grid-row: 1 / 2; }"
  );
  assert_eq!(
    fix(
      RULE,
      json!([true]),
      "a { font-synthesis-weight: auto; font-synthesis-style: none; font-synthesis-small-caps: auto; }",
      Syntax::Css
    ),
    "a { font-synthesis: weight small-caps; }"
  );
}

#[test]
fn removes_longhands_equal_to_their_shorthand() {
  let source = "a { margin: 1px; margin-left: 2px; margin-right: 1px; }";
  let diags = lint(RULE, json!([true]), source, Syntax::Css);
  assert_eq!(diags.len(), 1);
  assert_eq!(
    diags[0].message,
    "Redundant longhand property \"margin-right\" after shorthand property \"margin\""
  );
  assert_eq!(
    fix(RULE, json!([true]), source, Syntax::Css),
    "a { margin: 1px; margin-left: 2px; }"
  );
}

#[test]
fn unfixable_problems_are_left_alone() {
  let source = "a { transition-delay: ; transition-duration: 1s; transition-timing-function: ease; transition-property: top; }";
  assert_eq!(lint(RULE, json!([true]), source, Syntax::Css).len(), 1);
  assert_eq!(fix(RULE, json!([true]), source, Syntax::Css), source);
}

#[test]
fn ignore_options() {
  let options = json!([true, { "ignoreShorthands": ["/border/", "padding"] }]);
  assert_eq!(
    lint(
      RULE,
      options,
      "a { padding-left: 1px; padding-right: 1px; padding-top: 1px; padding-bottom: 1px; }",
      Syntax::Css
    )
    .len(),
    0
  );
  let options = json!([true, { "ignoreLonghands": "text-decoration-thickness" }]);
  assert_eq!(
    fix(
      RULE,
      options,
      "a { text-decoration-line: underline; text-decoration-style: solid; text-decoration-color: purple; }",
      Syntax::Css
    ),
    "a { text-decoration: underline solid purple; }"
  );
}

#[test]
fn disable_fix_keeps_the_warning_but_not_the_fix() {
  let options = json!([true, { "disableFix": true }]);
  let source = "a { overflow-x: hidden; overflow-y: auto; }";
  assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
  assert_eq!(lint(RULE, options, source, Syntax::Css).len(), 1);
}

#[test]
fn options_are_validated_like_stylelint() {
  assert!(
    invalid_options(
      RULE,
      json!([true, { "ignoreShorthands": ["/^margin/"], "ignoreLonghands": "padding-top" }])
    )
    .is_empty()
  );
  assert_eq!(
    invalid_options(RULE, json!("always")),
    [
      "Unexpected option value \"always\" for rule \"declaration-block-no-redundant-longhand-properties\""
    ]
  );
  assert_eq!(
    invalid_options(RULE, json!([true, { "ignoreLonghands": [1] }])),
    [
      "Invalid value \"1\" for option \"ignoreLonghands\" of rule \"declaration-block-no-redundant-longhand-properties\""
    ]
  );
}
