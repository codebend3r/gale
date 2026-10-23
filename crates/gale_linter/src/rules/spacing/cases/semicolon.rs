//! The `semicolon` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn declaration_block_semicolon_space_after_always() {
  let expected_after = "Expected single space after \";\"";
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!(["always"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink; }"),
      ok("a::before { content: \";a\"; }"),
      ok("a { color: pink; top: 0;}"),
      ok("a { color: pink; top: 0}"),
      bad("a { color: pink;top: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink; top: 0; }"),
      bad("a { color: pink;  top: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink; top: 0; }"),
      bad("a { color: pink;\ntop: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink; top: 0; }"),
      bad("a { color: pink;\r\ntop: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink; top: 0; }"),
      bad("a { color: pink;\ttop: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink; top: 0; }"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_space_after_never() {
  let rejected_after = "Unexpected whitespace after \";\"";
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!(["never"]),
    Css,
    &[
      ok("color: pink;"),
      ok("a { color: pink;}"),
      ok("a::before { content: \";a\";}"),
      ok("a { color: pink;top: 0;}"),
      bad("a { color: pink;\n top: 0;  }")
        .at(1, 17, rejected_after)
        .fixed("a { color: pink;top: 0;  }"),
      bad("a { color: pink; top: 0; }")
        .at(1, 17, rejected_after)
        .fixed("a { color: pink;top: 0; }"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_space_after_always_single_line() {
  let expected_after_single_line =
    "Expected single space after \";\" in a single-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a::before { content: \";a\"; }"),
      ok("a { color: pink; top: 0;}"),
      ok("a,\nb { color: pink; top: 0; }"),
      ok("a,\r\nb { color: pink; top: 0; }"),
      ok("a {\n  color: pink;\n  top: 0;\n}"),
      ok("a {\r\n  color: pink;\r\n  top: 0;\r\n}"),
      bad("a { color: pink;top: 0; }")
        .at(1, 17, expected_after_single_line)
        .fixed("a { color: pink; top: 0; }"),
      bad("a,\nb { color: pink;top: 0; }")
        .at(2, 17, expected_after_single_line)
        .fixed("a,\nb { color: pink; top: 0; }"),
      bad("a,\r\nb { color: pink;top: 0; }")
        .at(2, 17, expected_after_single_line)
        .fixed("a,\r\nb { color: pink; top: 0; }"),
      bad("a { color: pink;  top: 0; }")
        .at(1, 17, expected_after_single_line)
        .fixed("a { color: pink; top: 0; }"),
      bad("a { color: pink;\ttop: 0; }")
        .at(1, 17, expected_after_single_line)
        .fixed("a { color: pink; top: 0; }"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_space_after_never_single_line() {
  let rejected_after_single_line =
    "Unexpected whitespace after \";\" in a single-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a::before { content: \"; a\"; }"),
      ok("a { color: pink;top: 0; }"),
      ok("a,\nb { color: pink;top: 0; }"),
      ok("a {\n  color: pink; top: 0;\n}"),
      ok("a {\r\n  color: pink; top: 0;\r\n}"),
      bad("a { color: pink; top: 0; }")
        .at(1, 17, rejected_after_single_line)
        .fixed("a { color: pink;top: 0; }"),
      bad("a,\nb { color: pink; top: 0; }")
        .at(2, 17, rejected_after_single_line)
        .fixed("a,\nb { color: pink;top: 0; }"),
      bad("a,\r\nb { color: pink; top: 0; }")
        .at(2, 17, rejected_after_single_line)
        .fixed("a,\r\nb { color: pink;top: 0; }"),
      bad("a { color: pink;  top: 0; }")
        .at(1, 17, rejected_after_single_line)
        .fixed("a { color: pink;top: 0; }"),
      bad("a { color: pink;\ttop: 0; }")
        .at(1, 17, rejected_after_single_line)
        .fixed("a { color: pink;top: 0; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_space_after_always_single_line_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!("always-single-line"),
    Css,
    &[
      ok("a { color: red; display: block; }"),
      ok("got: {:?}"),
      bad("a { color: red;display: block; }")
        .at(
          1,
          16,
          "Expected single space after \";\" in a single-line declaration block",
        )
        .fixed("a { color: red; display: block; }"),
      ok("a {\n  color: red;display: block;\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_space_after_never_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!("never"),
    Css,
    &[bad("a {\n  color: red; display: block;\n}")
      .at(2, 14, "Unexpected whitespace after \";\"")
      .fixed("a {\n  color: red;display: block;\n}")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_space_after_always_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!("always"),
    Css,
    &[
      bad("a { color: red;display: block; }")
        .at(1, 16, "Expected single space after \";\"")
        .fixed("a { color: red; display: block; }"),
      ok("a { color: red;}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_space_after_never_single_line_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-space-after",
    json!("never-single-line"),
    Css,
    &[bad("a { color: red; display: block; }")
      .at(
        1,
        16,
        "Unexpected whitespace after \";\" in a single-line declaration block",
      )
      .fixed("a { color: red;display: block; }")],
  );
}

#[test]
fn declaration_block_semicolon_space_before_always() {
  let expected_before = "Expected single space before \";\"";
  check(
    "@stylistic/declaration-block-semicolon-space-before",
    json!(["always"]),
    Css,
    &[
      ok("color: pink ;"),
      ok("a { color: pink ; }"),
      ok("a::before { content: \";a\" ; }"),
      ok("a { color: pink ; top: 0 ; }"),
      ok("a { color: pink ; top: 0}"),
      ok("a { width: 50% !important ;}"),
      ok("a { --foo: ; }"),
      bad("a { color: pink; }")
        .at(1, 15, expected_before)
        .fixed("a { color: pink ; }"),
      bad("a { color: pink  ; }")
        .at(1, 17, expected_before)
        .fixed("a { color: pink ; }"),
      bad("a { color: pink\t; }")
        .at(1, 16, expected_before)
        .fixed("a { color: pink ; }"),
      bad("a { color: pink\n; }")
        .at(1, 16, expected_before)
        .fixed("a { color: pink ; }"),
      bad("a { color: pink\r\n; }")
        .at(1, 17, expected_before)
        .fixed("a { color: pink ; }"),
      bad("a { color: pink ; top: 0; }")
        .at(1, 24, expected_before)
        .fixed("a { color: pink ; top: 0 ; }"),
      bad("a { color: pink/*comment*/; }")
        .at(1, 26, expected_before)
        .fixed("a { color: pink/*comment*/ ; }"),
      bad("a { color: pink /*comment*/; }")
        .at(1, 27, expected_before)
        .fixed("a { color: pink /*comment*/ ; }"),
      bad("a { width: 50% !important; }")
        .at(1, 25, expected_before)
        .fixed("a { width: 50% !important ; }"),
      bad("a { --foo:; }")
        .at(1, 10, expected_before)
        .fixed("a { --foo: ; }"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_space_before_never() {
  let rejected_before = "Unexpected whitespace before \";\"";
  check(
    "@stylistic/declaration-block-semicolon-space-before",
    json!(["never"]),
    Css,
    &[
      ok("color: pink;"),
      ok("a { color: pink; }"),
      ok("a::before { content: \";a\"; }"),
      ok("a { color: pink; top: 0; }"),
      ok("a { width: 50% !important;}"),
      ok("a { --foo:; }"),
      ok("a { --foo: ; }"),
      bad("a { color: pink ; }")
        .at(1, 16, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink  ; }")
        .at(1, 17, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink\t; }")
        .at(1, 16, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink\n; }")
        .at(1, 16, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink\r\n; }")
        .at(1, 17, rejected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink; top: 0 ; }")
        .at(1, 24, rejected_before)
        .fixed("a { color: pink; top: 0; }"),
      bad("a { color: pink/*comment*/ ; }")
        .at(1, 27, rejected_before)
        .fixed("a { color: pink/*comment*/; }"),
      bad("a { color: pink /*comment*/ ; }")
        .at(1, 28, rejected_before)
        .fixed("a { color: pink /*comment*/; }"),
      bad("a { width: 50% !important ; }")
        .at(1, 26, rejected_before)
        .fixed("a { width: 50% !important; }"),
      bad("a { width: 50% !important     ; }")
        .at(1, 30, rejected_before)
        .fixed("a { width: 50% !important; }"),
      bad("a { --foo:      ; }")
        .at(1, 16, rejected_before)
        .fixed("a { --foo: ; }"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_space_before_always_single_line() {
  let expected_before_single_line =
    "Expected single space before \";\" in a single-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-space-before",
    json!(["always-single-line"]),
    Css,
    &[
      ok("color: pink ;"),
      ok("a { color: pink ; }"),
      ok("a::before { content: \";a\" ; }"),
      ok("a { color: pink ; top: 0 ; }"),
      ok("a,\nb { color: pink ; top: 0 ; }"),
      ok("a {\n  color: pink;\n  top: 0;\n}"),
      ok("a {\r\n  color: pink;\r\n  top: 0;\r\n}"),
      ok("a { width: 50% !important ; }"),
      ok("a {\n  width: 50% !important;\n}"),
      ok("a { --foo: ; }"),
      ok("a {\n  --foo: ;\n}"),
      bad("a { color: pink; }")
        .at(1, 15, expected_before_single_line)
        .fixed("a { color: pink ; }"),
      bad("a,\nb { color: pink; }")
        .at(2, 15, expected_before_single_line)
        .fixed("a,\nb { color: pink ; }"),
      bad("a,\r\nb { color: pink; }")
        .at(2, 15, expected_before_single_line)
        .fixed("a,\r\nb { color: pink ; }"),
      bad("a { color: pink  ; }")
        .at(1, 17, expected_before_single_line)
        .fixed("a { color: pink ; }"),
      bad("a { color: pink\t; }")
        .at(1, 16, expected_before_single_line)
        .fixed("a { color: pink ; }"),
      bad("a { color: pink ; top: 0; }")
        .at(1, 24, expected_before_single_line)
        .fixed("a { color: pink ; top: 0 ; }"),
      bad("a { width: 50% !important; }")
        .at(1, 25, expected_before_single_line)
        .fixed("a { width: 50% !important ; }"),
      bad("a { --foo:; }")
        .at(1, 10, expected_before_single_line)
        .fixed("a { --foo: ; }"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_space_before_never_single_line() {
  let rejected_before_single_line =
    "Unexpected whitespace before \";\" in a single-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-space-before",
    json!(["never-single-line"]),
    Css,
    &[
      ok("color: pink;"),
      ok("a { color: pink; }"),
      ok("a::before { content: \";a\"; }"),
      ok("a { color: pink; top: 0; }"),
      ok("a,\nb { color: pink; top: 0; }"),
      ok("a {\n  color: pink ;\n  top: 0 ;\n}"),
      ok("a { width: 50% !important; }"),
      ok("a {\n  width: 50% !important ;\n}"),
      ok("a { --foo:; }"),
      ok("a { --foo: ; }"),
      ok("a {\n  --foo: ;\n  --bar:;\n}"),
      bad("a { color: pink ; }")
        .at(1, 16, rejected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a,\nb { color: pink ; }")
        .at(2, 16, rejected_before_single_line)
        .fixed("a,\nb { color: pink; }"),
      bad("a,\r\nb { color: pink ; }")
        .at(2, 16, rejected_before_single_line)
        .fixed("a,\r\nb { color: pink; }"),
      bad("a { color: pink  ; }")
        .at(1, 17, rejected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a { color: pink\t; }")
        .at(1, 16, rejected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a { color: pink; top: 0 ; }")
        .at(1, 24, rejected_before_single_line)
        .fixed("a { color: pink; top: 0; }"),
      bad("a { width: 50% !important ; }")
        .at(1, 26, rejected_before_single_line)
        .fixed("a { width: 50% !important; }"),
      bad("a { width: 50% !important     ; }")
        .at(1, 30, rejected_before_single_line)
        .fixed("a { width: 50% !important; }"),
      bad("a { --foo:      ; }")
        .at(1, 16, rejected_before_single_line)
        .fixed("a { --foo: ; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_space_before_never_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-space-before",
    json!("never"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { color: red ; }")
        .at(1, 15, "Unexpected whitespace before \";\"")
        .fixed("a { color: red; }"),
      bad("a { --x: ; --y: 1 ; }")
        .at(1, 18, "Unexpected whitespace before \";\"")
        .fixed("a { --x: ; --y: 1; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_space_before_always_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-space-before",
    json!("always"),
    Css,
    &[bad("a { color: red ; top: 0; left: 0 }\nb { right: 0; }")
      .at(1, 23, "Expected single space before \";\"")
      .at(2, 12, "Expected single space before \";\"")
      .fixed("a { color: red ; top: 0 ; left: 0 }\nb { right: 0 ; }")],
  );
}

#[test]
fn declaration_block_semicolon_newline_after_always() {
  let expected_after = "Expected newline after \";\"";
  check(
    "@stylistic/declaration-block-semicolon-newline-after",
    json!(["always"]),
    Css,
    &[
      ok("a { color: pink;\n}"),
      ok("a { color: pink;\r\n}"),
      ok("a { color: pink;\n\n}"),
      ok("a { color: pink;\r\n\r\n}"),
      ok("a::before { content: \";a\";\n}"),
      ok("a {\ncolor: pink;\n top:0;\n}"),
      ok("a {\ncolor: pink;\n  top:0;\n}"),
      ok("a {\ncolor: pink;\n\ttop:0;\n}"),
      ok("a {\r\ncolor: pink;\r\n\ttop:0;\r\n}"),
      ok("a { color: pink;\ntop: 0; }"),
      ok("a { color: pink;\ntop: 0;}"),
      ok("a { color: pink;\r\ntop: 0;}"),
      ok("a { color: pink;\ntop: 0}"),
      ok("a {\n  color: pink; /* 1 */\n  top: 0\n}"),
      ok("a {\n  color: pink;    /* 1 */\n  top: 0\n}"),
      ok("a {\r\n  color: pink;\t/* 1 */\r\n  top: 0\r\n}"),
      ok("a {\n  color: pink;\n  /* 1 */\n  top: 0\n}"),
      ok("a,\nb { color: pink;\ntop: 0}"),
      ok("a,\r\nb { color: pink;\r\ntop: 0}"),
      bad("a { color: pink;top: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink;\ntop: 0; }"),
      bad("a { color: pink; top: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink;\n top: 0; }"),
      bad("a { color: pink; top: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink;\n top: 0; }"),
      bad("a { color: pink;\ttop: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink;\n\ttop: 0; }"),
      bad("a {\n  color: pink; /* 1 */ top: 0\n}")
        .at(2, 15, expected_after)
        .fixed("a {\n  color: pink; /* 1 */\n top: 0\n}"),
      bad("a {\r\n  color: pink; /* 1 */ top: 0\r\n}")
        .at(2, 15, expected_after)
        .fixed("a {\r\n  color: pink; /* 1 */\r\n top: 0\r\n}"),
      bad("a { color: pink; \n top: 0; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink;\n top: 0; }"),
      bad("a {\n  color: pink; /* 1 */ /* 2 */ top: 0\n}")
        .at(2, 15, expected_after)
        .fixed("a {\n  color: pink; /* 1 */ /* 2 */\n top: 0\n}"),
      bad("a {\n  color: pink; /* 1 */ \n top: 0\n}")
        .at(2, 15, expected_after)
        .fixed("a {\n  color: pink; /* 1 */\n top: 0\n}"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \";\" in a multi-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-newline-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a {\ncolor: pink;\n}"),
      ok("a::before {\ncontent: \";a\";\n}"),
      ok("a::before {\r\ncontent: \";a\";\r\n}"),
      ok("a {\ncolor: pink;\n top:0;\n}"),
      ok("a {\ncolor: pink;\n  top:0;\n}"),
      ok("a {\r\ncolor: pink;\r\n  top:0;\r\n}"),
      ok("a {\ncolor: pink;\n\ttop:0;\n}"),
      ok("a {\ncolor: pink;\ntop: 0; }"),
      ok("a {\ncolor: pink;\ntop: 0;}"),
      ok("a { color: pink; top: 0; }"),
      ok("a { color: pink; /* 1 */ top: 0; }"),
      ok("a,\nb { color: pink; top: 0}"),
      ok("a,\r\nb { color: pink; top: 0}"),
      bad("a {\ncolor: pink;top: 0;\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed("a {\ncolor: pink;\ntop: 0;\n}"),
      bad("a {\ncolor: pink; top: 0;\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed("a {\ncolor: pink;\n top: 0;\n}"),
      bad("a {\r\ncolor: pink; top: 0;\r\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed("a {\r\ncolor: pink;\r\n top: 0;\r\n}"),
      bad("a {\ncolor: pink; top: 0;\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed("a {\ncolor: pink;\n top: 0;\n}"),
      bad("a {\ncolor: pink;\ttop: 0;\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed("a {\ncolor: pink;\n\ttop: 0;\n}"),
      bad("a {\r\ncolor: pink;\ttop: 0;\r\n}")
        .at(2, 13, expected_after_multi_line)
        .fixed("a {\r\ncolor: pink;\r\n\ttop: 0;\r\n}"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_newline_after_never_multi_line() {
  let rejected_after_multi_line =
    "Unexpected newline after \";\" in a multi-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-newline-after",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a {\ncolor: pink;\n}"),
      ok("a {\r\ncolor: pink;\r\n}"),
      ok("a::before {\ncontent: \";\na\";\n}"),
      ok("a {\ncolor: pink;top: 0; }"),
      ok("a {\ncolor: pink;top: 0;}"),
      ok("a { color: pink; top: 0; }"),
      ok("a,\nb { color: pink; top: 0}"),
      bad("a {\ncolor: pink; top: 0;\n}")
        .at(2, 13, rejected_after_multi_line)
        .fixed("a {\ncolor: pink;top: 0;\n}"),
      bad("a {\ncolor: pink;  top: 0;\n}")
        .at(2, 13, rejected_after_multi_line)
        .fixed("a {\ncolor: pink;top: 0;\n}"),
      bad("a {\ncolor: pink;\ntop: 0;\n}")
        .at(2, 13, rejected_after_multi_line)
        .fixed("a {\ncolor: pink;top: 0;\n}"),
      bad("a {\r\ncolor: pink;\r\ntop: 0;\r\n}")
        .at(2, 13, rejected_after_multi_line)
        .fixed("a {\r\ncolor: pink;top: 0;\r\n}"),
      bad("a {\ncolor: pink;\ttop: 0;\n}")
        .at(2, 13, rejected_after_multi_line)
        .fixed("a {\ncolor: pink;top: 0;\n}"),
      bad("a {\ncolor: pink; /*comment*/ top: 0;\n}")
        .at(2, 13, rejected_after_multi_line)
        .fixed("a {\ncolor: pink; /*comment*/top: 0;\n}"),
      bad("a {\n  color: pink; /* 1 */ /* 2 */\ntop: 0\n}")
        .at(2, 15, rejected_after_multi_line)
        .fixed("a {\n  color: pink; /* 1 */ /* 2 */top: 0\n}"),
      bad("a {\n  color: pink; /* 1 */ \n top: 0\n}")
        .at(2, 15, rejected_after_multi_line)
        .fixed("a {\n  color: pink; /* 1 */top: 0\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_newline_after_always_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-newline-after",
    json!("always"),
    Css,
    &[
      ok("a {\n  color: red;\n  display: block;\n}"),
      ok("got: {:?}"),
      bad("a { color: red; display: block; }")
        .at(1, 16, "Expected newline after \";\"")
        .fixed("a { color: red;\n display: block; }"),
      ok("a {\n  @include mixin { color: red; };\n  display: block;\n}"),
      ok("a {\n  @include mixin;\n  color: red;\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_newline_after_always_multi_line_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-newline-after",
    json!("always-multi-line"),
    Css,
    &[
      ok("a { color: red; display: block; }"),
      bad("a {\n  color: red; display: block;\n}")
        .at(
          2,
          14,
          "Expected newline after \";\" in a multi-line declaration block",
        )
        .fixed("a {\n  color: red;\n display: block;\n}"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_newline_before_always() {
  let expected_before = "Expected newline before \";\"";
  check(
    "@stylistic/declaration-block-semicolon-newline-before",
    json!(["always"]),
    Css,
    &[
      ok("a {--a:\n;}"),
      ok("a {\n\t--a:\n;\n}"),
      ok("color: pink\n;"),
      ok("a { color: pink\n; }"),
      ok("a { color: pink\n\n; }"),
      ok("a::before { content: \";a\"\n; }"),
      ok("a { color: pink\n;top: 0 }"),
      ok("a { color: pink\n;top: 0}"),
      ok("a { color: pink\r\n;top: 0}"),
      ok("a { color: pink\r\n\r\n;top: 0}"),
      ok("a,\nb { color: pink\n;top: 0}"),
      bad("a {--a:;}")
        .at(1, 7, expected_before)
        .fixed("a {--a:\n;}"),
      bad("a {\n\t--a:;\n}")
        .at(2, 5, expected_before)
        .fixed("a {\n\t--a:\n;\n}"),
      bad("a { color: pink;top: 0 }")
        .at(1, 15, expected_before)
        .fixed("a { color: pink\n;top: 0 }"),
      bad("a { color: pink ;top: 0 }")
        .at(1, 16, expected_before)
        .fixed("a { color: pink\n;top: 0 }"),
      bad("a { color: pink  ;top: 0 }")
        .at(1, 17, expected_before)
        .fixed("a { color: pink\n;top: 0 }"),
      bad("a { color: pink\t;top: 0 }")
        .at(1, 16, expected_before)
        .fixed("a { color: pink\n;top: 0 }"),
      bad("a { color: pink\t ;top: 0 }")
        .at(1, 17, expected_before)
        .fixed("a { color: pink\n;top: 0 }"),
      bad("a { color: pink/*comment*/;top: 0 }")
        .at(1, 26, expected_before)
        .fixed("a { color: pink/*comment*/\n;top: 0 }"),
      bad("a { color: pink/*comment*/ ;top: 0 }")
        .at(1, 27, expected_before)
        .fixed("a { color: pink/*comment*/\n;top: 0 }"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_newline_before_always_multi_line() {
  let expected_before_multi_line =
    "Expected newline before \";\" in a multi-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-newline-before",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a {\n\t--a:\n;\n}"),
      ok("color: pink\n;"),
      ok("a {\ncolor: pink\n; }"),
      ok("a::before {\ncontent: \";a\"\n; }"),
      ok("a::before {\r\ncontent: \";a\"\r\n; }"),
      ok("a {\ncolor: pink\n;top: 0 }"),
      ok("a {\ncolor: pink\n;top: 0}"),
      ok("a {\r\ncolor: pink\r\n;top: 0}"),
      ok("a { color: pink;top: 0; }"),
      ok("a,\nb { color: pink; top: 0}"),
      ok("a,\r\nb { color: pink; top: 0}"),
      bad("a {\n\t--a:;\n}")
        .at(2, 5, expected_before_multi_line)
        .fixed("a {\n\t--a:\n;\n}"),
      bad("a {\ncolor: pink;top: 0\n}")
        .at(2, 11, expected_before_multi_line)
        .fixed("a {\ncolor: pink\n;top: 0\n}"),
      bad("a {\ncolor: pink ;top: 0\n}")
        .at(2, 12, expected_before_multi_line)
        .fixed("a {\ncolor: pink\n;top: 0\n}"),
      bad("a {\ncolor: pink  ;top: 0\n}")
        .at(2, 13, expected_before_multi_line)
        .fixed("a {\ncolor: pink\n;top: 0\n}"),
      bad("a {\r\ncolor: pink  ;top: 0\r\n}")
        .at(2, 13, expected_before_multi_line)
        .fixed("a {\r\ncolor: pink\r\n;top: 0\r\n}"),
      bad("a {\ncolor: pink\t;top: 0\n}")
        .at(2, 12, expected_before_multi_line)
        .fixed("a {\ncolor: pink\n;top: 0\n}"),
      bad("a {\ncolor: pink/*comment*/;top: 0\n}")
        .at(2, 22, expected_before_multi_line)
        .fixed("a {\ncolor: pink/*comment*/\n;top: 0\n}"),
    ],
  );
}

#[test]
fn declaration_block_semicolon_newline_before_never_multi_line() {
  let rejected_before_multi_line =
    "Unexpected whitespace before \";\" in a multi-line declaration block";
  check(
    "@stylistic/declaration-block-semicolon-newline-before",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("color: pink;"),
      ok("a {\n\t--a:;\n}"),
      ok("a {\n\t--a: ;\n}"),
      ok("a {\ncolor: pink;\n}"),
      ok("a {\r\ncolor: pink;\r\n}"),
      ok("a::before {\ncontent: \";a\";\n}"),
      ok("a {\ncolor: pink;\ntop: 0 }"),
      ok("a {\ncolor: pink;\ntop: 0}"),
      ok("a {\r\ncolor: pink;\r\ntop: 0}"),
      ok("a { color: pink; top: 0; }"),
      ok("a,\nb { color: pink ;top: 0}"),
      bad("a {\n\t--a:\n;\n}")
        .at(2, 6, rejected_before_multi_line)
        .fixed("a {\n\t--a:;\n}"),
      bad("a {\ncolor: pink\n;top: 0\n}")
        .at(2, 12, rejected_before_multi_line)
        .fixed("a {\ncolor: pink;top: 0\n}"),
      bad("a {\ncolor: pink ;top: 0\n}")
        .at(2, 12, rejected_before_multi_line)
        .fixed("a {\ncolor: pink;top: 0\n}"),
      bad("a {\r\ncolor: pink ;top: 0\r\n}")
        .at(2, 12, rejected_before_multi_line)
        .fixed("a {\r\ncolor: pink;top: 0\r\n}"),
      bad("a {\ncolor: pink  ;top: 0\n}")
        .at(2, 13, rejected_before_multi_line)
        .fixed("a {\ncolor: pink;top: 0\n}"),
      bad("a {\ncolor: pink\t;top: 0\n}")
        .at(2, 12, rejected_before_multi_line)
        .fixed("a {\ncolor: pink;top: 0\n}"),
      bad("a {\r\ncolor: pink\t;top: 0\r\n}")
        .at(2, 12, rejected_before_multi_line)
        .fixed("a {\r\ncolor: pink;top: 0\r\n}"),
      bad("a {\ncolor: pink/*comment*/\n;top: 0\n}")
        .at(2, 23, rejected_before_multi_line)
        .fixed("a {\ncolor: pink/*comment*/;top: 0\n}"),
      bad("a {\ncolor: pink/*comment*/ ;top: 0\n}")
        .at(2, 23, rejected_before_multi_line)
        .fixed("a {\ncolor: pink/*comment*/;top: 0\n}"),
    ],
  );
}

// Gale's own `"never"`, which the plugin does not take: no line break
// next to the character.  Gale adds no fix for it.
#[test]
fn declaration_block_semicolon_newline_before_never_gale_only() {
  check(
    "@stylistic/declaration-block-semicolon-newline-before",
    json!("never"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { color: red\n; }").at(1, 15, "Unexpected whitespace before \";\""),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_block_semicolon_newline_before_always_gale_cases() {
  check(
    "@stylistic/declaration-block-semicolon-newline-before",
    json!("always"),
    Css,
    &[bad("a { color: red\n; top: 0; left: 0 }\nb { right: 0; }")
      .at(2, 8, "Expected newline before \";\"")
      .at(3, 12, "Expected newline before \";\"")
      .fixed("a { color: red\n; top: 0\n; left: 0 }\nb { right: 0\n; }")],
  );
}
