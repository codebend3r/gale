//! The `bang` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn declaration_bang_space_after_always() {
  let expected_after = "Expected single space after \"!\"";
  check(
    "@stylistic/declaration-bang-space-after",
    json!(["always"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink! important; }"),
      ok("a { color: pink ! default; }"),
      ok("a { color: pink\n! important; }"),
      ok("a { color: pink\r\n! optional; }"),
      ok("a::before { content: \"!!!\" ! important; }"),
      ok("a { color: pink /* !important */;}"),
      bad("a { color: pink!important; }")
        .at(1, 16, expected_after)
        .fixed("a { color: pink! important; }"),
      bad("a { color: pink!  global; }")
        .at(1, 16, expected_after)
        .fixed("a { color: pink! global; }"),
      bad("a { color: pink!\nimportant; }")
        .at(1, 16, expected_after)
        .fixed("a { color: pink! important; }"),
      bad("a { color: pink!\r\nexciting; }")
        .at(1, 16, expected_after)
        .fixed("a { color: pink! exciting; }"),
      bad("a { color: pink !/*comment*/important; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink ! /*comment*/important; }"),
      bad("a { color: pink !/*comment*/global; }")
        .at(1, 17, expected_after)
        .fixed("a { color: pink ! /*comment*/global; }"),
    ],
  );
}

#[test]
fn declaration_bang_space_after_never() {
  let rejected_after = "Unexpected whitespace after \"!\"";
  check(
    "@stylistic/declaration-bang-space-after",
    json!(["never"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink!important; }"),
      ok("a { color: pink !important; }"),
      ok("a { color: pink\n!important; }"),
      ok("a { color: pink\r\n!important; }"),
      bad("a { color: pink! important; }")
        .at(1, 16, rejected_after)
        .fixed("a { color: pink!important; }"),
      bad("a { color: pink!\nimportant; }")
        .at(1, 16, rejected_after)
        .fixed("a { color: pink!important; }"),
      bad("a { color: pink!\r\nimportant; }")
        .at(1, 16, rejected_after)
        .fixed("a { color: pink!important; }"),
      bad("a { color: pink ! /*comment*/important; }")
        .at(1, 17, rejected_after)
        .fixed("a { color: pink !/*comment*/important; }"),
      bad("a { color: pink ! /*comment*/global; }")
        .at(1, 17, rejected_after)
        .fixed("a { color: pink !/*comment*/global; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_bang_space_after_never_gale_cases() {
  check(
    "@stylistic/declaration-bang-space-after",
    json!("never"),
    Css,
    &[
      ok("a { color: red !important; }"),
      bad("a { color: red ! important; }")
        .at(1, 16, "Unexpected whitespace after \"!\"")
        .fixed("a { color: red !important; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_bang_space_after_always_gale_cases() {
  check(
    "@stylistic/declaration-bang-space-after",
    json!("always"),
    Css,
    &[
      ok("a { color: red ! important; }"),
      bad("a { color: red !important; }")
        .at(1, 16, "Expected single space after \"!\"")
        .fixed("a { color: red ! important; }"),
    ],
  );
}

#[test]
fn declaration_bang_space_before_always() {
  let expected_before = "Expected single space before \"!\"";
  check(
    "@stylistic/declaration-bang-space-before",
    json!(["always"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink !important; }"),
      ok("a { color: pink ! important; }"),
      ok("a { color: pink !\noptional; }"),
      ok("a { color: pink !\r\nimportant; }"),
      ok("a::before { content: \"!!!\" !default; }"),
      ok("a { color: pink/*!important */;}"),
      bad("a { color: pink  !important; }")
        .at(1, 18, expected_before)
        .fixed("a { color: pink !important; }"),
      bad("a { color: pink!default; }")
        .at(1, 16, expected_before)
        .fixed("a { color: pink !default; }"),
      bad("a { color: pink\n!important; }")
        .at(2, 1, expected_before)
        .fixed("a { color: pink !important; }"),
      bad("a { color: pink\r\n!something; }")
        .at(2, 1, expected_before)
        .fixed("a { color: pink !something; }"),
      bad("a { color: pink/*comment*/!important; }")
        .at(1, 27, expected_before)
        .fixed("a { color: pink/*comment*/ !important; }"),
      bad("a { color: pink/*comment*/!something; }")
        .at(1, 27, expected_before)
        .fixed("a { color: pink/*comment*/ !something; }"),
      bad("a { color: pink\n/*comment*/!important; }")
        .at(2, 12, expected_before)
        .fixed("a { color: pink\n/*comment*/ !important; }"),
      bad("a { color: pink\n/*comment*/!something; }")
        .at(2, 12, expected_before)
        .fixed("a { color: pink\n/*comment*/ !something; }"),
    ],
  );
}

#[test]
fn declaration_bang_space_before_never() {
  let rejected_before = "Unexpected whitespace before \"!\"";
  check(
    "@stylistic/declaration-bang-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink!important; }"),
      ok("a { color: pink! important; }"),
      ok("a { color: pink!\nimportant; }"),
      ok("a { color: pink!\r\nimportant; }"),
      bad("a { color: pink !important; }")
        .at(1, 17, rejected_before)
        .fixed("a { color: pink!important; }"),
      bad("a { color: pink\n!important; }")
        .at(2, 1, rejected_before)
        .fixed("a { color: pink!important; }"),
      bad("a { color: pink\r\n!important; }")
        .at(2, 1, rejected_before)
        .fixed("a { color: pink!important; }"),
      bad("a { color: pink/*comment*/ !important; }")
        .at(1, 28, rejected_before)
        .fixed("a { color: pink/*comment*/!important; }"),
      bad("a { color: pink/*comment*/ !something; }")
        .at(1, 28, rejected_before)
        .fixed("a { color: pink/*comment*/!something; }"),
      bad("a { color: pink\n/*comment*/\n!important; }")
        .at(3, 1, rejected_before)
        .fixed("a { color: pink\n/*comment*/!important; }"),
      bad("a { color: pink\n/*comment*/\n!something; }")
        .at(3, 1, rejected_before)
        .fixed("a { color: pink\n/*comment*/!something; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_bang_space_before_always_gale_cases() {
  check(
    "@stylistic/declaration-bang-space-before",
    json!("always"),
    Css,
    &[
      ok("a { color: red !important; }"),
      bad("a { color: red!important; }")
        .at(1, 15, "Expected single space before \"!\"")
        .fixed("a { color: red !important; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn declaration_bang_space_before_never_gale_cases() {
  check(
    "@stylistic/declaration-bang-space-before",
    json!("never"),
    Css,
    &[
      ok("a { color: red!important; }"),
      bad("a { color: red !important; }")
        .at(1, 16, "Unexpected whitespace before \"!\"")
        .fixed("a { color: red!important; }"),
    ],
  );
}
