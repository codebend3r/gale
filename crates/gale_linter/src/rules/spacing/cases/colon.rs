//! The `colon` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn declaration_colon_space_after_always() {
  let expected_after = "Expected single space after \":\"";
  check(
    "@stylistic/declaration-colon-space-after",
    json!(["always"]),
    Css,
    &[
      ok("a { --a: ; }"),
      ok("a { --a: ; color: red; }"),
      ok("a { color: pink }"),
      ok("a { color : pink }"),
      ok("a { color\n: pink }"),
      ok("a { color\r\n: pink }"),
      ok("$map:(key:value)"),
      ok("$list:('value1', 'value2')"),
      ok("a { background: url(data:application/font-woff;...); }"),
      bad("a { --a:; }")
        .at(1, 9, expected_after)
        .fixed("a { --a: ; }"),
      bad("a { --a:  ; }")
        .at(1, 9, expected_after)
        .fixed("a { --a: ; }"),
      bad("a {\n\t--a:;\n}")
        .at(2, 6, expected_after)
        .fixed("a {\n\t--a: ;\n}"),
      bad("a {\n\t--a:  ;\n}")
        .at(2, 6, expected_after)
        .fixed("a {\n\t--a: ;\n}"),
      bad("a {\n\t--a:\n\t;\n}")
        .at(2, 6, expected_after)
        .fixed("a {\n\t--a: ;\n}"),
      bad("a { color :pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color : pink; }"),
      bad("a { color :  pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color : pink; }"),
      bad("a { color :\tpink; }")
        .at(1, 11, expected_after)
        .fixed("a { color : pink; }"),
      bad("a { color :\npink; }")
        .at(1, 11, expected_after)
        .fixed("a { color : pink; }"),
      bad("a { color :\r\npink; }")
        .at(1, 11, expected_after)
        .fixed("a { color : pink; }"),
      bad("a { color:pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color: pink; }"),
      bad("a { color/*comment*/:/*comment*/pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color/*comment*/: /*comment*/pink; }"),
    ],
  );
}

#[test]
fn declaration_colon_space_after_never() {
  let rejected_after = "Unexpected whitespace after \":\"";
  check(
    "@stylistic/declaration-colon-space-after",
    json!(["never"]),
    Css,
    &[
      ok("a { --a:; color:red; }"),
      ok("a { color:pink }"),
      ok("a { color :pink }"),
      ok("a { color\n:pink }"),
      ok("a { color\r\n:pink }"),
      ok("$map: (key: value)"),
      bad("a { --a: ; color:red; }")
        .at(1, 9, rejected_after)
        .fixed("a { --a:; color:red; }"),
      bad("a { color : pink; }")
        .at(1, 11, rejected_after)
        .fixed("a { color :pink; }"),
      bad("a { color:  pink; }")
        .at(1, 11, rejected_after)
        .fixed("a { color:pink; }"),
      bad("a { color :\tpink; }")
        .at(1, 11, rejected_after)
        .fixed("a { color :pink; }"),
      bad("a { color :\npink; }")
        .at(1, 11, rejected_after)
        .fixed("a { color :pink; }"),
      bad("a { color :\r\npink; }")
        .at(1, 11, rejected_after)
        .fixed("a { color :pink; }"),
      bad("a { color/*comment*/ : /*comment*/pink; }")
        .at(1, 11, rejected_after)
        .fixed("a { color/*comment*/ :/*comment*/pink; }"),
    ],
  );
}

#[test]
fn declaration_colon_space_after_always_single_line() {
  let expected_after_single_line =
    "Expected single space after \":\" with a single-line declaration";
  check(
    "@stylistic/declaration-colon-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { --a: ; color: red; }"),
      ok("a { --a: ; }"),
      ok("a { color: pink }"),
      ok("a { transition: color 1s,\n\twidth 2s; }"),
      ok("a { transition:color 1s,\n\twidth 2s; }"),
      ok("a { transition:color 1s,\r\n\twidth 2s; }"),
      ok("a { transition:\tcolor 1s,\n\twidth 2s; }"),
      bad("a { --a:; }")
        .at(1, 9, expected_after_single_line)
        .fixed("a { --a: ; }"),
      bad("a { --a:  ; }")
        .at(1, 9, expected_after_single_line)
        .fixed("a { --a: ; }"),
      bad("a { --a:  ; color: red; }")
        .at(1, 9, expected_after_single_line)
        .fixed("a { --a: ; color: red; }"),
      bad("a { color :pink; }")
        .at(1, 11, expected_after_single_line)
        .fixed("a { color : pink; }"),
      bad("a { color :  pink; }")
        .at(1, 11, expected_after_single_line)
        .fixed("a { color : pink; }"),
      bad("a { color :\tpink; }")
        .at(1, 11, expected_after_single_line)
        .fixed("a { color : pink; }"),
      bad("a { color :\npink; }")
        .at(1, 11, expected_after_single_line)
        .fixed("a { color : pink; }"),
      bad("a { color :\r\npink; }")
        .at(1, 11, expected_after_single_line)
        .fixed("a { color : pink; }"),
      bad("a { color:pink; }")
        .at(1, 11, expected_after_single_line)
        .fixed("a { color: pink; }"),
      bad("a { color/*comment*/:/*comment*/pink; }")
        .at(1, 11, expected_after_single_line)
        .fixed("a { color/*comment*/: /*comment*/pink; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_colon_space_after_always_gale_cases() {
  check(
    "@stylistic/declaration-colon-space-after",
    json!("always"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { color:red; }")
        .at(1, 11, "Expected single space after \":\"")
        .fixed("a { color: red; }"),
      ok("a:hover { color: red; }"),
      ok(".parent { input:focus { color: red; } }"),
      ok("a::before { color: red; }"),
      ok("a:not(.foo) { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_colon_space_after_never_gale_cases() {
  check(
    "@stylistic/declaration-colon-space-after",
    json!("never"),
    Css,
    &[
      ok("a { color:red; }"),
      bad("a { color: red; }")
        .at(1, 11, "Unexpected whitespace after \":\"")
        .fixed("a { color:red; }"),
    ],
  );
}

#[test]
fn declaration_colon_space_before_always() {
  let expected_before = "Expected single space before \":\"";
  check(
    "@stylistic/declaration-colon-space-before",
    json!(["always"]),
    Css,
    &[
      ok("a { color :pink }"),
      ok("a { color : pink }"),
      ok("a { color :\npink }"),
      ok("a { color :\r\npink }"),
      ok("$map:(key:value)"),
      ok("$list:('value1', 'value2')"),
      ok("a { background : url(data:application/font-woff;...); }"),
      bad("a { color: pink; }")
        .at(1, 11, expected_before)
        .fixed("a { color : pink; }"),
      bad("a { color  : pink; }")
        .at(1, 11, expected_before)
        .fixed("a { color : pink; }"),
      bad("a { color\t: pink; }")
        .at(1, 11, expected_before)
        .fixed("a { color : pink; }"),
      bad("a { color\n: pink; }")
        .at(2, 1, expected_before)
        .fixed("a { color : pink; }"),
      bad("a { color\r\n: pink; }")
        .at(1, 11, expected_before)
        .fixed("a { color : pink; }"),
      bad("a { color/*comment*/:/*comment*/pink; }")
        .at(1, 11, expected_before)
        .fixed("a { color/*comment*/ :/*comment*/pink; }"),
    ],
  );
}

#[test]
fn declaration_colon_space_before_never() {
  let rejected_before = "Unexpected whitespace before \":\"";
  check(
    "@stylistic/declaration-colon-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a { color:pink }"),
      ok("a { color: pink }"),
      ok("a { color:\npink }"),
      ok("a { color:\r\npink }"),
      ok("$map :(key :value)"),
      bad("a { color : pink; }")
        .at(1, 11, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color  : pink; }")
        .at(1, 11, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color\t: pink; }")
        .at(1, 11, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color\n: pink; }")
        .at(2, 1, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color\r\n: pink; }")
        .at(1, 11, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color/*comment*/ :/*comment*/pink; }")
        .at(1, 11, rejected_before)
        .fixed("a { color/*comment*/:/*comment*/pink; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_colon_space_before_never_gale_cases() {
  check(
    "@stylistic/declaration-colon-space-before",
    json!("never"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { color : red; }")
        .at(1, 11, "Unexpected whitespace before \":\"")
        .fixed("a { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_colon_space_before_always_gale_cases() {
  check(
    "@stylistic/declaration-colon-space-before",
    json!("always"),
    Css,
    &[
      ok("a { color : red; }"),
      bad("a { color: red; }")
        .at(1, 11, "Expected single space before \":\"")
        .fixed("a { color : red; }"),
    ],
  );
}

#[test]
fn declaration_colon_newline_after_always() {
  let expected_after = "Expected newline after \":\"";
  check(
    "@stylistic/declaration-colon-newline-after",
    json!(["always"]),
    Css,
    &[
      ok("a {\n  color:\n    pink\n}"),
      ok("a { color :\npink }"),
      ok("a { color\n:\npink }"),
      ok("a { color\r\n:\r\npink }"),
      ok("a { color\n\n:\n\npink }"),
      ok("a { color\r\n\r\n:\r\n\r\npink }"),
      ok("$map: (key: value)"),
      ok("a { background:\n  url(data:application/font-woff;...); }"),
      bad("a { color :pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\npink; }"),
      bad("a { color :  pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\n  pink; }"),
      bad("a { color :\tpink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\n\tpink; }"),
      bad("a { color : pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\n pink; }"),
      bad("a { color : \npink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\npink; }"),
      bad("a { color : \r\npink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\r\npink; }"),
      bad("a { color : \t\n\t\npink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\n\t\npink; }"),
      bad("a { color :/*comment*/pink; }")
        .at(1, 22, expected_after)
        .fixed("a { color :/*comment*/\npink; }"),
      bad("a { color : /*comment*/ pink; }")
        .at(1, 23, expected_after)
        .fixed("a { color : /*comment*/\n pink; }"),
      bad("a { color : \n/*comment*/ pink; }")
        .at(1, 11, expected_after)
        .fixed("a { color :\n/*comment*/ pink; }"),
      bad("a { color        :    pink; }")
        .at(1, 18, expected_after)
        .fixed("a { color        :\n    pink; }"),
    ],
  );
}

#[test]
fn declaration_colon_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \":\" with a multi-line declaration";
  check(
    "@stylistic/declaration-colon-newline-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a {\n  color: pink\n}"),
      ok("a {\n  box-shadow:\n    0 0 0 1px #5b9dd9\n    0 0 2px 1px rgba(30, 140, 190, 0.8);\n}"),
      ok("$map\n: (\nkey: value,\nkey2 :value2)"),
      ok("$list: (\n'value1',\n'value2',\n)"),
      ok("a { color:pink }"),
      ok("a { color :\tpink }"),
      ok("a { color\n: pink }"),
      ok("a { color\r\n:  pink }"),
      bad("a {\n  box-shadow: 0 0 0 1px #5b9dd9\n    0 0 2px 1px rgba(30, 140, 190, 0.8);\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed(
          "a {\n  box-shadow:\n 0 0 0 1px #5b9dd9\n    0 0 2px 1px rgba(30, 140, 190, 0.8);\n}",
        ),
      bad("a {\n  box-shadow:0 0 0 1px #5b9dd9\n    0 0 2px 1px rgba(30, 140, 190, 0.8);\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed(
          "a {\n  box-shadow:\n0 0 0 1px #5b9dd9\n    0 0 2px 1px rgba(30, 140, 190, 0.8);\n}",
        ),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_colon_newline_after_always_multi_line_gale_cases() {
  check(
    "@stylistic/declaration-colon-newline-after",
    json!("always-multi-line"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { background: url()\n    no-repeat; }")
        .at(
          1,
          15,
          "Expected newline after \":\" with a multi-line declaration",
        )
        .fixed("a { background:\n url()\n    no-repeat; }"),
      ok("a { background:\n    url() no-repeat; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_colon_newline_after_always_gale_cases() {
  check(
    "@stylistic/declaration-colon-newline-after",
    json!("always"),
    Css,
    &[
      bad("a { color: red; }")
        .at(1, 10, "Expected newline after \":\"")
        .fixed("a { color:\n red; }"),
      ok("a { color:\n  red; }"),
    ],
  );
}

#[test]
fn media_feature_colon_space_after_always() {
  let expected_after = "Expected single space after \":\"";
  check(
    "@stylistic/media-feature-colon-space-after",
    json!(["always"]),
    Css,
    &[
      ok("@media (max-width: 600px) {}"),
      ok("@mEdIa (max-width: 600px) {}"),
      ok("@MEDIA (max-width: 600px) {}"),
      ok("@media (max-width : 600px) {}"),
      ok("@media (max-width: 600px) and (min-width: 3em) {}"),
      ok("@custom-selector :--enter :hover;"),
      bad("@media (max-width:600px) {}")
        .at(1, 18, expected_after)
        .fixed("@media (max-width: 600px) {}"),
      bad("@mEdIa (max-width:600px) {}")
        .at(1, 18, expected_after)
        .fixed("@mEdIa (max-width: 600px) {}"),
      bad("@MEDIA (max-width:600px) {}")
        .at(1, 18, expected_after)
        .fixed("@MEDIA (max-width: 600px) {}"),
      bad("@media (max-width:  600px) {}")
        .at(1, 18, expected_after)
        .fixed("@media (max-width: 600px) {}"),
      bad("@media (max-width:\t600px) {}")
        .at(1, 18, expected_after)
        .fixed("@media (max-width: 600px) {}"),
      bad("@media (max-width:\n600px) {}")
        .at(1, 18, expected_after)
        .fixed("@media (max-width: 600px) {}"),
      bad("@media (max-width:\r\n600px) {}")
        .at(1, 18, expected_after)
        .fixed("@media (max-width: 600px) {}"),
      bad("@media (max-width:600px) and (min-width: 3em) {}")
        .at(1, 18, expected_after)
        .fixed("@media (max-width: 600px) and (min-width: 3em) {}"),
      bad("@media (max-width: 600px) and (min-width:3em) {}")
        .at(1, 41, expected_after)
        .fixed("@media (max-width: 600px) and (min-width: 3em) {}"),
      bad("@media (max-width:600px) and (min-width:3em) {}")
        .at(1, 18, expected_after)
        .at(1, 40, expected_after)
        .fixed("@media (max-width: 600px) and (min-width: 3em) {}"),
      bad("@media(p:600px) and (prop:600px) {}")
        .at(1, 9, expected_after)
        .at(1, 26, expected_after)
        .fixed("@media(p: 600px) and (prop: 600px) {}"),
      bad("@media (max-width:/*comment*/600px) {}")
        .at(1, 18, expected_after)
        .fixed("@media (max-width: /*comment*/600px) {}"),
    ],
  );
}

#[test]
fn media_feature_colon_space_after_never() {
  let rejected_after = "Unexpected whitespace after \":\"";
  check(
    "@stylistic/media-feature-colon-space-after",
    json!(["never"]),
    Css,
    &[
      ok("@media (max-width:600px) {}"),
      ok("@mEdIa (max-width:600px) {}"),
      ok("@MEDIA (max-width:600px) {}"),
      ok("@media (max-width:600px) and (min-width:3em) {}"),
      ok("@custom-selector : --enter :hover;"),
      bad("@media (max-width: 600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@media (max-width:600px) {}"),
      bad("@mEdIa (max-width: 600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@mEdIa (max-width:600px) {}"),
      bad("@MEDIA (max-width: 600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@MEDIA (max-width:600px) {}"),
      bad("@media (max-width:  600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width:\t600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width:\n600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width:\r\n600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width:600px) and (min-width: 3em) {}")
        .at(1, 40, rejected_after)
        .fixed("@media (max-width:600px) and (min-width:3em) {}"),
      bad("@media (max-width: 600px) and (min-width:3em) {}")
        .at(1, 18, rejected_after)
        .fixed("@media (max-width:600px) and (min-width:3em) {}"),
      bad("@media (max-width: 600px) and (min-width: 3em) {}")
        .at(1, 18, rejected_after)
        .at(1, 41, rejected_after)
        .fixed("@media (max-width:600px) and (min-width:3em) {}"),
      bad("@media (max-width: /*comment*/ 600px) {}")
        .at(1, 18, rejected_after)
        .fixed("@media (max-width:/*comment*/ 600px) {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_colon_space_after_always_gale_cases() {
  check(
    "@stylistic/media-feature-colon-space-after",
    json!("always"),
    Css,
    &[
      ok("@media (min-width: 768px) { }"),
      bad("@media (min-width:768px) { }")
        .at(1, 18, "Expected single space after \":\"")
        .fixed("@media (min-width: 768px) { }"),
      ok("a { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_colon_space_after_never_gale_cases() {
  check(
    "@stylistic/media-feature-colon-space-after",
    json!("never"),
    Css,
    &[
      ok("@media (min-width:768px) { }"),
      bad("@media (min-width: 768px) { }")
        .at(1, 18, "Unexpected whitespace after \":\"")
        .fixed("@media (min-width:768px) { }"),
    ],
  );
}

#[test]
fn media_feature_colon_space_before_always() {
  let expected_before = "Expected single space before \":\"";
  check(
    "@stylistic/media-feature-colon-space-before",
    json!(["always"]),
    Css,
    &[
      ok("@media (max-width :600px) {}"),
      ok("@mEdIa (max-width :600px) {}"),
      ok("@MEDIA (max-width :600px) {}"),
      ok("@media (max-width : 600px) {}"),
      ok("@media (max-width :600px) and (min-width :3em) {}"),
      ok("@custom-selector:--enter :hover;"),
      bad("@media (max-width:600px) {}")
        .at(1, 18, expected_before)
        .fixed("@media (max-width :600px) {}"),
      bad("@mEdIa (max-width:600px) {}")
        .at(1, 18, expected_before)
        .fixed("@mEdIa (max-width :600px) {}"),
      bad("@MEDIA (max-width:600px) {}")
        .at(1, 18, expected_before)
        .fixed("@MEDIA (max-width :600px) {}"),
      bad("@media (max-width  :600px) {}")
        .at(1, 20, expected_before)
        .fixed("@media (max-width :600px) {}"),
      bad("@media (max-width\t:600px) {}")
        .at(1, 19, expected_before)
        .fixed("@media (max-width :600px) {}"),
      bad("@media (max-width\n:600px) {}")
        .at(2, 1, expected_before)
        .fixed("@media (max-width :600px) {}"),
      bad("@media (max-width\r\n:600px) {}")
        .at(2, 1, expected_before)
        .fixed("@media (max-width :600px) {}"),
      bad("@media (max-width:600px) and (min-width :3em) {}")
        .at(1, 18, expected_before)
        .fixed("@media (max-width :600px) and (min-width :3em) {}"),
      bad("@media (max-width :600px) and (min-width:3em) {}")
        .at(1, 41, expected_before)
        .fixed("@media (max-width :600px) and (min-width :3em) {}"),
      bad("@media (max-width:600px) and (min-width:3em) {}")
        .at(1, 18, expected_before)
        .at(1, 40, expected_before)
        .fixed("@media (max-width :600px) and (min-width :3em) {}"),
      bad("@media (max-width/*comment*/:600px) {}")
        .at(1, 29, expected_before)
        .fixed("@media (max-width/*comment*/ :600px) {}"),
    ],
  );
}

#[test]
fn media_feature_colon_space_before_never() {
  let rejected_before = "Unexpected whitespace before \":\"";
  check(
    "@stylistic/media-feature-colon-space-before",
    json!(["never"]),
    Css,
    &[
      ok("@media (max-width:600px) {}"),
      ok("@mEdIa (max-width:600px) {}"),
      ok("@MEDIA (max-width:600px) {}"),
      ok("@media (max-width: 600px) {}"),
      ok("@media (max-width:600px) and (min-width:3em) {}"),
      ok("@custom-selector :--enter :hover;"),
      bad("@media (max-width :600px) {}")
        .at(1, 19, rejected_before)
        .fixed("@media (max-width:600px) {}"),
      bad("@mEdIa (max-width :600px) {}")
        .at(1, 19, rejected_before)
        .fixed("@mEdIa (max-width:600px) {}"),
      bad("@MEDIA (max-width :600px) {}")
        .at(1, 19, rejected_before)
        .fixed("@MEDIA (max-width:600px) {}"),
      bad("@media (max-width  :600px) {}")
        .at(1, 20, rejected_before)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width\t:600px) {}")
        .at(1, 19, rejected_before)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width\n:600px) {}")
        .at(2, 1, rejected_before)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width\r\n:600px) {}")
        .at(2, 1, rejected_before)
        .fixed("@media (max-width:600px) {}"),
      bad("@media (max-width:600px) and (min-width :3em) {}")
        .at(1, 41, rejected_before)
        .fixed("@media (max-width:600px) and (min-width:3em) {}"),
      bad("@media (max-width :600px) and (min-width:3em) {}")
        .at(1, 19, rejected_before)
        .fixed("@media (max-width:600px) and (min-width:3em) {}"),
      bad("@media (max-width :600px) and (min-width :3em) {}")
        .at(1, 19, rejected_before)
        .at(1, 42, rejected_before)
        .fixed("@media (max-width:600px) and (min-width:3em) {}"),
      bad("@media (max-width /*comment*/ :600px) {}")
        .at(1, 31, rejected_before)
        .fixed("@media (max-width /*comment*/:600px) {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_colon_space_before_never_gale_cases() {
  check(
    "@stylistic/media-feature-colon-space-before",
    json!("never"),
    Css,
    &[
      ok("@media (min-width: 100px) { }"),
      bad("@media (min-width : 100px) { }")
        .at(1, 19, "Unexpected whitespace before \":\"")
        .fixed("@media (min-width: 100px) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_colon_space_before_always_gale_cases() {
  check(
    "@stylistic/media-feature-colon-space-before",
    json!("always"),
    Css,
    &[bad("@media (min-width: 100px) { }")
      .at(1, 18, "Expected single space before \":\"")
      .fixed("@media (min-width : 100px) { }")],
  );
}

#[test]
fn scss_dollar_variable_colon_space_after_always_scss() {
  let expected_after = "Expected single space after \":\"";
  check(
    "scss/dollar-variable-colon-space-after",
    json!(["always"]),
    Scss,
    &[
      ok("a { width:10px }"),
      ok("a { $var: 10px }"),
      ok("a { $var : 10px }"),
      ok("a { $var\n: 10px }"),
      ok("a { $var\r\n: 10px }"),
      ok("$var: 10px;"),
      ok("$var : 10px"),
      ok("$var: url(data:application/font-woff;...);"),
      ok("@function ($a:10) {}"),
      bad("a { $var :10px; }")
        .at(1, 10, expected_after)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :  10px; }")
        .at(1, 10, expected_after)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :\t10px; }")
        .at(1, 10, expected_after)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :\n10px; }")
        .at(1, 10, expected_after)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :\r\n10px; }")
        .at(1, 10, expected_after)
        .fixed("a { $var : 10px; }"),
      bad("a { $var:10px; }")
        .at(1, 9, expected_after)
        .fixed("a { $var: 10px; }"),
      bad("$var :10px;")
        .at(1, 6, expected_after)
        .fixed("$var : 10px;"),
      bad("$var :  10px;")
        .at(1, 6, expected_after)
        .fixed("$var : 10px;"),
      bad("$var :\t10px;")
        .at(1, 6, expected_after)
        .fixed("$var : 10px;"),
      bad("$var :\n10px;")
        .at(1, 6, expected_after)
        .fixed("$var : 10px;"),
      bad("$var :\r\n10px;")
        .at(1, 6, expected_after)
        .fixed("$var : 10px;"),
      bad("$var:10px;")
        .at(1, 5, expected_after)
        .fixed("$var: 10px;"),
    ],
  );
}

#[test]
fn scss_dollar_variable_colon_space_after_never_scss() {
  let rejected_after = "Unexpected whitespace after \":\"";
  check(
    "scss/dollar-variable-colon-space-after",
    json!(["never"]),
    Scss,
    &[
      ok("a { $var:10px }"),
      ok("a { $var :10px }"),
      ok("a { $var\n:10px }"),
      ok("a { $var\r\n:10px }"),
      ok("$map:(key: value)"),
      bad("a { $var : 10px; }")
        .at(1, 10, rejected_after)
        .fixed("a { $var :10px; }"),
      bad("a { $var:  10px; }")
        .at(1, 9, rejected_after)
        .fixed("a { $var:10px; }"),
      bad("a { $var :\t10px; }")
        .at(1, 10, rejected_after)
        .fixed("a { $var :10px; }"),
      bad("a { $var :\n10px; }")
        .at(1, 10, rejected_after)
        .fixed("a { $var :10px; }"),
      bad("a { $var :\r\n10px; }")
        .at(1, 10, rejected_after)
        .fixed("a { $var :10px; }"),
    ],
  );
}

#[test]
fn scss_dollar_variable_colon_space_after_always_single_line_scss() {
  let expected_after_single_line = "Expected single space after \":\" with a single-line value";
  check(
    "scss/dollar-variable-colon-space-after",
    json!(["always-single-line"]),
    Scss,
    &[
      ok("a { $var: 10px }"),
      ok("$transition: color 1s,\n\twidth 2s;"),
      ok("$transition:color 1s,\n\twidth 2s;"),
      ok("$transition:color 1s,\r\n\twidth 2s;"),
      ok("a { $transition:\tcolor 1s,\n\twidth 2s; }"),
      bad("a { $var :10px; }")
        .at(1, 10, expected_after_single_line)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :  10px; }")
        .at(1, 10, expected_after_single_line)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :\t10px; }")
        .at(1, 10, expected_after_single_line)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :\n10px; }")
        .at(1, 10, expected_after_single_line)
        .fixed("a { $var : 10px; }"),
      bad("a { $var :\r\n10px; }")
        .at(1, 10, expected_after_single_line)
        .fixed("a { $var : 10px; }"),
      bad("a { $var:10px; }")
        .at(1, 9, expected_after_single_line)
        .fixed("a { $var: 10px; }"),
    ],
  );
}

#[test]
fn scss_dollar_variable_colon_space_after_at_least_one_space_scss() {
  let expected_after_at_least = "Expected at least one space after \":\"";
  check(
    "scss/dollar-variable-colon-space-after",
    json!(["at-least-one-space"]),
    Scss,
    &[
      ok("a { width:10px }"),
      ok("a { $var: 10px }"),
      ok("a { $var : 10px }"),
      ok("a { $var\n: 10px }"),
      ok("a { $var\r\n: 10px }"),
      ok("$var: 10px;"),
      ok("$var : 10px"),
      ok("$var: url(data:application/font-woff;...);"),
      ok("@function ($a:10) {}"),
      ok("$var:    10px"),
      bad("a { $var :10px; }")
        .at(1, 10, expected_after_at_least)
        .fixed("a { $var : 10px; }"),
      bad("$var :10px;")
        .at(1, 6, expected_after_at_least)
        .fixed("$var : 10px;"),
      bad("$var:10px;")
        .at(1, 5, expected_after_at_least)
        .fixed("$var: 10px;"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_dollar_variable_colon_space_after_always_scss_gale_cases() {
  check(
    "scss/dollar-variable-colon-space-after",
    json!("always"),
    Scss,
    &[
      ok("$color: red;"),
      bad("$color:red;")
        .at(1, 7, "Expected single space after \":\"")
        .fixed("$color: red;"),
      bad("$color:  red;")
        .at(1, 7, "Expected single space after \":\"")
        .fixed("$color: red;"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_dollar_variable_colon_space_after_always_gale_cases() {
  check(
    "scss/dollar-variable-colon-space-after",
    json!("always"),
    Css,
    &[bad("$color:red;")
      .at(1, 7, "Expected single space after \":\"")
      .fixed("$color: red;")],
  );
}

#[test]
fn scss_dollar_variable_colon_space_before_always_scss() {
  let expected_before = "Expected single space before \":\"";
  check(
    "scss/dollar-variable-colon-space-before",
    json!(["always"]),
    Scss,
    &[
      ok("a { $var1 :200px }"),
      ok("a { $var1 : 200px }"),
      ok("a { $var1 :\n200px }"),
      ok("a { $var1 :\r\n200px }"),
      ok("a { width: 10px; }"),
      ok("$background : url(data:application/font-woff;...);"),
      ok("@function ($a:10) {}"),
      bad("a { $var1: 200px; }")
        .at(1, 10, expected_before)
        .fixed("a { $var1 : 200px; }"),
      bad("a { $var1  : 200px; }")
        .at(1, 12, expected_before)
        .fixed("a { $var1 : 200px; }"),
      bad("a { $var1\t: 200px; }")
        .at(1, 11, expected_before)
        .fixed("a { $var1 : 200px; }"),
      bad("a { $var1\n: 200px; }")
        .at(2, 1, expected_before)
        .fixed("a { $var1 : 200px; }"),
      bad("a { $var1\r\n: 200px; }")
        .at(2, 1, expected_before)
        .fixed("a { $var1 : 200px; }"),
    ],
  );
}

#[test]
fn scss_dollar_variable_colon_space_before_never_scss() {
  let rejected_before = "Unexpected whitespace before \":\"";
  check(
    "scss/dollar-variable-colon-space-before",
    json!(["never"]),
    Scss,
    &[
      ok("a { $var1:200px }"),
      ok("a { $var1: 200px }"),
      ok("a { $var1:\n200px }"),
      ok("a { $var1:\r\n200px }"),
      ok("a { width : 10px; }"),
      bad("a { $var1 : 200px; }")
        .at(1, 11, rejected_before)
        .fixed("a { $var1: 200px; }"),
      bad("a { $var1  : 200px; }")
        .at(1, 12, rejected_before)
        .fixed("a { $var1: 200px; }"),
      bad("a { $var1\t: 200px; }")
        .at(1, 11, rejected_before)
        .fixed("a { $var1: 200px; }"),
      bad("a { $var1\n: 200px; }")
        .at(2, 1, rejected_before)
        .fixed("a { $var1: 200px; }"),
      bad("a { $var1\r\n: 200px; }")
        .at(2, 1, rejected_before)
        .fixed("a { $var1: 200px; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_dollar_variable_colon_space_before_never_scss_gale_cases() {
  check(
    "scss/dollar-variable-colon-space-before",
    json!("never"),
    Scss,
    &[
      ok("$color: red;"),
      bad("$color : red;")
        .at(1, 8, "Unexpected whitespace before \":\"")
        .fixed("$color: red;"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_dollar_variable_colon_space_before_never_gale_cases() {
  check(
    "scss/dollar-variable-colon-space-before",
    json!("never"),
    Css,
    &[bad("$color : red;")
      .at(1, 8, "Unexpected whitespace before \":\"")
      .fixed("$color: red;")],
  );
}
