//! The `brace` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn block_opening_brace_space_before_always() {
  let expected_before = "Expected single space before \"{\"";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["always"]),
    Css,
    &[
      ok("@import url(x.css)"),
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("a {{ &:hover { color: pink; }}}"),
      ok("a {\n&:hover { color: pink; }}"),
      bad("a{ color: pink; }")
        .at(1, 1, expected_before)
        .fixed("a { color: pink; }"),
      bad("a  { color: pink; }")
        .at(1, 3, expected_before)
        .fixed("a { color: pink; }"),
      bad("a\t{ color: pink; }")
        .at(1, 2, expected_before)
        .fixed("a { color: pink; }"),
      bad("a\n{ color: pink; }")
        .at(1, 2, expected_before)
        .fixed("a { color: pink; }"),
      bad("a\r\n{ color: pink; }")
        .at(1, 2, expected_before)
        .fixed("a { color: pink; }"),
      bad("@media print\n{ a { color: pink; } }")
        .at(1, 13, expected_before)
        .fixed("@media print { a { color: pink; } }"),
      bad("@media print { a\n{ color: pink; } }")
        .at(1, 17, expected_before)
        .fixed("@media print { a { color: pink; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_before_always_with_ignoreatrules() {
  let expected_before = "Expected single space before \"{\"";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["always", {"ignoreAtRules": ["for", "/for/i", "/for/iu"]}]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("@for ...\n{ color: pink; }"),
      ok("@for ...\r\n{ color: pink; }"),
      bad("a{ color: pink; }")
        .at(1, 1, expected_before)
        .fixed("a { color: pink; }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_before_always_with_ignoreselectors() {
  let expected_before = "Expected single space before \"{\"";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["always", {"ignoreSelectors": ["a", "/a/", "/a/u"]}]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a{ color: pink; }"),
      ok("a\n{ color: pink; }"),
      ok("a\r\n{ color: pink; }"),
      bad("b{ color: pink; }")
        .at(1, 1, expected_before)
        .fixed("b { color: pink; }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_before_never() {
  let rejected_before = "Unexpected whitespace before \"{\"";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a{ color: pink; }"),
      ok("@media print{ a{ color: pink; } }"),
      bad("a { color: pink; }")
        .at(1, 2, rejected_before)
        .fixed("a{ color: pink; }"),
      bad("a  { color: pink; }")
        .at(1, 3, rejected_before)
        .fixed("a{ color: pink; }"),
      bad("a\t{ color: pink; }")
        .at(1, 2, rejected_before)
        .fixed("a{ color: pink; }"),
      bad("a\n{ color: pink; }")
        .at(1, 2, rejected_before)
        .fixed("a{ color: pink; }"),
      bad("a\r\n{ color: pink; }")
        .at(1, 2, rejected_before)
        .fixed("a{ color: pink; }"),
      bad("@media print { a{ color: pink; } }")
        .at(1, 13, rejected_before)
        .fixed("@media print{ a{ color: pink; } }"),
      bad("@media print{ a { color: pink; } }")
        .at(1, 16, rejected_before)
        .fixed("@media print{ a{ color: pink; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_before_always_single_line() {
  let expected_before_single_line = "Expected single space before \"{\" of a single-line block";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("a{ color:\npink; }"),
      ok("@media print { a{ color:\npink; } }"),
      ok("@media print{ a { color:\npink; } }"),
      ok("@media print{\na { color: pink; } }"),
      bad("a{ color: pink; }")
        .at(1, 1, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a  { color: pink; }")
        .at(1, 3, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a\t{ color: pink; }")
        .at(1, 2, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a\n{ color: pink; }")
        .at(1, 2, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a\r\n{ color: pink; }")
        .at(1, 2, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("@media print\n{ a { color: pink; } }")
        .at(1, 13, expected_before_single_line)
        .fixed("@media print { a { color: pink; } }"),
      bad("@media print { a\n{ color: pink; } }")
        .at(1, 17, expected_before_single_line)
        .fixed("@media print { a { color: pink; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_before_never_single_line() {
  let rejected_before_single_line = "Unexpected whitespace before \"{\" of a single-line block";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a{ color: pink; }"),
      ok("@media print{ a{ color: pink; } }"),
      ok("a { color:\npink; }"),
      ok("a { color:\r\npink; }"),
      ok("@media print { a { color:\npink; } }"),
      ok("@media print{ a{ color:\npink; } }"),
      ok("@media print {\na{ color: pink; } }"),
      ok("@media print{\na{ color: pink; } }"),
      ok("@media print{\r\na{ color: pink; } }"),
      bad("a { color: pink; }")
        .at(1, 2, rejected_before_single_line)
        .fixed("a{ color: pink; }"),
      bad("a  { color: pink; }")
        .at(1, 3, rejected_before_single_line)
        .fixed("a{ color: pink; }"),
      bad("a\t{ color: pink; }")
        .at(1, 2, rejected_before_single_line)
        .fixed("a{ color: pink; }"),
      bad("a\n{ color: pink; }")
        .at(1, 2, rejected_before_single_line)
        .fixed("a{ color: pink; }"),
      bad("a\r\n{ color: pink; }")
        .at(1, 2, rejected_before_single_line)
        .fixed("a{ color: pink; }"),
      bad("@media print { a{ color: pink; } }")
        .at(1, 13, rejected_before_single_line)
        .fixed("@media print{ a{ color: pink; } }"),
      bad("@media print{ a { color: pink; } }")
        .at(1, 16, rejected_before_single_line)
        .fixed("@media print{ a{ color: pink; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_before_always_multi_line() {
  let expected_before_multi_line = "Expected single space before \"{\" of a multi-line block";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\nbackground: orange; }"),
      ok("@media print {\na { color: pink;\nbackground: orange } }"),
      ok("@media print {\r\na { color: pink;\r\nbackground: orange } }"),
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("a{ color: pink; }"),
      ok("a  { color: pink; }"),
      ok("a\t{ color: pink; }"),
      bad("a{ color: pink;\nbackground: orange; }")
        .at(1, 1, expected_before_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a  { color: pink;\nbackground: orange; }")
        .at(1, 3, expected_before_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a\t{ color: pink;\nbackground: orange; }")
        .at(1, 2, expected_before_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a\n{ color: pink;\nbackground: orange; }")
        .at(1, 2, expected_before_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a\r\n{ color: pink;\r\nbackground: orange; }")
        .at(1, 2, expected_before_multi_line)
        .fixed("a { color: pink;\r\nbackground: orange; }"),
      bad("@media print\n{\na { color: pink;\nbackground: orange; } }")
        .at(1, 13, expected_before_multi_line)
        .fixed("@media print {\na { color: pink;\nbackground: orange; } }"),
      bad("@media print { a\n{ color: pink;\nbackground: orange; } }")
        .at(1, 17, expected_before_multi_line)
        .fixed("@media print { a { color: pink;\nbackground: orange; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_before_never_multi_line() {
  let rejected_before_multi_line = "Unexpected whitespace before \"{\" of a multi-line block";
  check(
    "@stylistic/block-opening-brace-space-before",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a{ color: pink;\nbackground: orange; }"),
      ok("@media print{\na{ color: pink;\nbackground: orange } }"),
      ok("@media print{\r\na{ color: pink;\r\nbackground: orange } }"),
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("a{ color: pink; }"),
      ok("a  { color: pink; }"),
      ok("a\t{ color: pink; }"),
      bad("a { color: pink;\nbackground: orange; }")
        .at(1, 2, rejected_before_multi_line)
        .fixed("a{ color: pink;\nbackground: orange; }"),
      bad("a  { color: pink;\nbackground: orange; }")
        .at(1, 3, rejected_before_multi_line)
        .fixed("a{ color: pink;\nbackground: orange; }"),
      bad("a\t{ color: pink;\nbackground: orange; }")
        .at(1, 2, rejected_before_multi_line)
        .fixed("a{ color: pink;\nbackground: orange; }"),
      bad("a\n{ color: pink;\nbackground: orange; }")
        .at(1, 2, rejected_before_multi_line)
        .fixed("a{ color: pink;\nbackground: orange; }"),
      bad("@media print\n{\na{ color: pink;\nbackground: orange; } }")
        .at(1, 13, rejected_before_multi_line)
        .fixed("@media print{\na{ color: pink;\nbackground: orange; } }"),
      bad("@media print{ a\n{ color: pink;\nbackground: orange; } }")
        .at(1, 16, rejected_before_multi_line)
        .fixed("@media print{ a{ color: pink;\nbackground: orange; } }"),
      bad("@media print{ a\r\n{ color: pink;\r\nbackground: orange; } }")
        .at(1, 16, rejected_before_multi_line)
        .fixed("@media print{ a{ color: pink;\r\nbackground: orange; } }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_before_always_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-before",
    json!("always"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a{ color: red; }")
        .at(1, 1, "Expected single space before \"{\"")
        .fixed("a { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_before_never_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-before",
    json!("never"),
    Css,
    &[
      ok("a{ color: red; }"),
      bad("a { color: red; }")
        .at(1, 2, "Unexpected whitespace before \"{\"")
        .fixed("a{ color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_before_always_single_line_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-before",
    json!("always-single-line"),
    Css,
    &[ok("a { color: red; }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_before_always_multi_line_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-before",
    json!("always-multi-line"),
    Css,
    &[
      ok("a{ color: red; }"),
      bad("a{\n  color: red;\n}")
        .at(
          1,
          1,
          "Expected single space before \"{\" of a multi-line block",
        )
        .fixed("a {\n  color: red;\n}"),
      ok("a { color: red; }"),
      ok("@mixin foo() {}"),
    ],
  );
}

#[test]
fn block_opening_brace_space_after_always() {
  let expected_after = "Expected single space after \"{\"";
  check(
    "@stylistic/block-opening-brace-space-after",
    json!(["always"]),
    Css,
    &[
      ok("@import url(x.css)"),
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      bad("a {color: pink; }")
        .at(1, 4, expected_after)
        .fixed("a { color: pink; }"),
      bad("a {  color: pink; }")
        .at(1, 4, expected_after)
        .fixed("a { color: pink; }"),
      bad("a {\tcolor: pink; }")
        .at(1, 4, expected_after)
        .fixed("a { color: pink; }"),
      bad("a {\ncolor: pink; }")
        .at(1, 4, expected_after)
        .fixed("a { color: pink; }"),
      bad("a {\r\ncolor: pink; }")
        .at(1, 4, expected_after)
        .fixed("a { color: pink; }"),
      bad("@media print {\na { color: pink; } }")
        .at(1, 15, expected_after)
        .fixed("@media print { a { color: pink; } }"),
      bad("@media print { a {\ncolor: pink; } }")
        .at(1, 19, expected_after)
        .fixed("@media print { a { color: pink; } }"),
      bad("@media print { a {\r\ncolor: pink; } }")
        .at(1, 19, expected_after)
        .fixed("@media print { a { color: pink; } }"),
      bad("a {/*comment*/ color: pink; }")
        .at(1, 4, expected_after)
        .fixed("a { /*comment*/ color: pink; }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_after_never() {
  let rejected_after = "Unexpected whitespace after \"{\"";
  check(
    "@stylistic/block-opening-brace-space-after",
    json!(["never"]),
    Css,
    &[
      ok("a {color: pink; }"),
      ok("@media print {a {color: pink; } }"),
      bad("a { color: pink; }")
        .at(1, 4, rejected_after)
        .fixed("a {color: pink; }"),
      bad("a {  color: pink; }")
        .at(1, 4, rejected_after)
        .fixed("a {color: pink; }"),
      bad("a {\tcolor: pink; }")
        .at(1, 4, rejected_after)
        .fixed("a {color: pink; }"),
      bad("a {\ncolor: pink; }")
        .at(1, 4, rejected_after)
        .fixed("a {color: pink; }"),
      bad("a {\r\ncolor: pink; }")
        .at(1, 4, rejected_after)
        .fixed("a {color: pink; }"),
      bad("@media print {\na {color: pink; } }")
        .at(1, 15, rejected_after)
        .fixed("@media print {a {color: pink; } }"),
      bad("@media print {a {\ncolor: pink; } }")
        .at(1, 18, rejected_after)
        .fixed("@media print {a {color: pink; } }"),
      bad("a { /*comment*/ color: pink; }")
        .at(1, 4, rejected_after)
        .fixed("a {/*comment*/ color: pink; }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_after_always_single_line() {
  let expected_after_single_line = "Expected single space after \"{\" of a single-line block";
  check(
    "@stylistic/block-opening-brace-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("a {\ncolor: pink; }"),
      ok("a {\r\ncolor: pink; }"),
      ok("a {color:\npink; }"),
      ok("@media print {a {color:\npink; } }"),
      ok("@media print{a {color:\npink; } }"),
      bad("a {color: pink; }")
        .at(1, 4, expected_after_single_line)
        .fixed("a { color: pink; }"),
      bad("a {  color: pink; }")
        .at(1, 4, expected_after_single_line)
        .fixed("a { color: pink; }"),
      bad("a {\tcolor: pink; }")
        .at(1, 4, expected_after_single_line)
        .fixed("a { color: pink; }"),
      bad("@media print {\ta { color: pink; } }")
        .at(1, 15, expected_after_single_line)
        .fixed("@media print { a { color: pink; } }"),
      bad("@media print { a {\tcolor: pink; } }")
        .at(1, 19, expected_after_single_line)
        .fixed("@media print { a { color: pink; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_after_never_single_line() {
  let rejected_after_single_line = "Unexpected whitespace after \"{\" of a single-line block";
  check(
    "@stylistic/block-opening-brace-space-after",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a {color: pink; }"),
      ok("@media print {a {color: pink; } }"),
      ok("a { color:\npink; }"),
      ok("@media print { a { color:\npink; } }"),
      ok("@media print { a\n{color: pink; } }"),
      ok("@media print { a\r\n{color: pink; } }"),
      bad("a { color: pink; }")
        .at(1, 4, rejected_after_single_line)
        .fixed("a {color: pink; }"),
      bad("a {  color: pink; }")
        .at(1, 4, rejected_after_single_line)
        .fixed("a {color: pink; }"),
      bad("a {\tcolor: pink; }")
        .at(1, 4, rejected_after_single_line)
        .fixed("a {color: pink; }"),
      bad("@media print { a {color: pink; } }")
        .at(1, 15, rejected_after_single_line)
        .fixed("@media print {a {color: pink; } }"),
      bad("@media print {a { color: pink; } }")
        .at(1, 18, rejected_after_single_line)
        .fixed("@media print {a {color: pink; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_after_always_multi_line() {
  let expected_after_multi_line = "Expected single space after \"{\" of a multi-line block";
  check(
    "@stylistic/block-opening-brace-space-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\nbackground: orange; }"),
      ok("@media print { a { color: pink;\nbackground: orange } }"),
      ok("a {color: pink; }"),
      ok("@media print {a {color: pink; } }"),
      ok("a { color: pink; }"),
      ok("a {  color: pink; }"),
      ok("a {\tcolor: pink; }"),
      bad("a {color: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a {  color: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a {\tcolor: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a {\ncolor: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a { color: pink;\nbackground: orange; }"),
      bad("a {\r\ncolor: pink;\r\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a { color: pink;\r\nbackground: orange; }"),
      bad("@media print\n{a { color: pink;\nbackground: orange; } }")
        .at(2, 2, expected_after_multi_line)
        .fixed("@media print\n{ a { color: pink;\nbackground: orange; } }"),
      bad("@media print { a\n{color: pink;\nbackground: orange; } }")
        .at(2, 2, expected_after_multi_line)
        .fixed("@media print { a\n{ color: pink;\nbackground: orange; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_after_never_multi_line() {
  let rejected_after_multi_line = "Unexpected whitespace after \"{\" of a multi-line block";
  check(
    "@stylistic/block-opening-brace-space-after",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a {color: pink;\nbackground: orange; }"),
      ok("@media print {a\n{color: pink;\nbackground: orange } }"),
      ok("@media print {a\r\n{color: pink;\r\nbackground: orange } }"),
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("a { color: pink; }"),
      ok("a {  color: pink; }"),
      ok("a {\tcolor: pink; }"),
      bad("a { color: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("a {  color: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("a {\tcolor: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("a {\tcolor: pink;\r\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\r\nbackground: orange; }"),
      bad("a {\ncolor: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("@media print\n{ a {color: pink;\nbackground: orange; } }")
        .at(2, 2, rejected_after_multi_line)
        .fixed("@media print\n{a {color: pink;\nbackground: orange; } }"),
      bad("@media print{a\n{ color: pink;\nbackground: orange; } }")
        .at(2, 2, rejected_after_multi_line)
        .fixed("@media print{a\n{color: pink;\nbackground: orange; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_space_after_always_with_ignore() {
  let expected_after = "Expected single space after \"{\"";
  check(
    "@stylistic/block-opening-brace-space-after",
    json!(["always", {"ignore": ["at-rules"]}]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("@media print {\na { color: pink; } }"),
      bad("@media print {\n  a {color: pink; } }")
        .at(2, 6, expected_after)
        .fixed("@media print {\n  a { color: pink; } }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_after_always_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-after",
    json!("always"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a {color: red; }")
        .at(1, 4, "Expected single space after \"{\"")
        .fixed("a { color: red; }"),
      bad("a {\n  color: red;\n}")
        .at(1, 4, "Expected single space after \"{\"")
        .fixed("a { color: red;\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_after_never_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-after",
    json!("never"),
    Css,
    &[
      ok("a {color: red;}"),
      bad("a { color: red;}")
        .at(1, 4, "Unexpected whitespace after \"{\"")
        .fixed("a {color: red;}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_after_always_single_line_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-after",
    json!("always-single-line"),
    Css,
    &[
      ok("a {\ncolor: red;\n}"),
      bad("a {color: red; }")
        .at(
          1,
          4,
          "Expected single space after \"{\" of a single-line block",
        )
        .fixed("a { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_space_after_always_multi_line_gale_cases() {
  check(
    "@stylistic/block-opening-brace-space-after",
    json!("always-multi-line"),
    Css,
    &[ok("a {color: red; }")],
  );
}

#[test]
fn block_opening_brace_newline_after_always() {
  let expected_after = "Expected newline after \"{\"";
  check(
    "@stylistic/block-opening-brace-newline-after",
    json!(["always"]),
    Css,
    &[
      ok("@import url(x.css)"),
      ok("a {\ncolor: pink; }"),
      ok("a {\r\ncolor: pink; }"),
      ok("a {\n\ncolor: pink; }"),
      ok("a {\r\n\r\ncolor: pink; }"),
      ok("a{\ncolor: pink; }"),
      ok("a{\n\tcolor: pink; }"),
      ok("a{\n  color: pink; }"),
      ok("a{\r\n  color: pink; }"),
      ok("@media print {\na {\ncolor: pink; } }"),
      ok("@media print{\na{\ncolor: pink; } }"),
      ok("@media print{\r\na{\r\ncolor: pink; } }"),
      ok("@media print{\n\ta{\n  color: pink; } }"),
      ok("a { /* 1 */\n  color: pink;\n}"),
      ok("a {    /* 1 */\n  color: pink;\n}"),
      ok("a {\n  /* 1 */\n  color: pink;\n}"),
      ok("a {\r\n  /* 1 */\r\n  color: pink;\r\n}"),
      ok(".a {\n/*.b*/.c {\n color: pink; }\n }"),
      ok(".a {\r\n/*.b*/.c {\r\n color: pink; }\r\n }"),
      ok("@media print {\n /*.test2*/.a {\n color: pink;\n }\n }"),
      ok("@media print {\r\n /*.test2*/.a {\r\n color: pink;\r\n }\r\n }"),
      ok("@media print {\n /*.test2*/\n .a {\n color: pink;\n }\n }"),
      ok("@media print {\r\n /*.test2*/\r\n .a {\r\n color: pink;\r\n }\r\n }"),
      ok("@media print {\r\n      /*.test2*/\r\n .a {\r\n color: pink;\r\n }\r\n }"),
      bad("a { color: pink; }")
        .at(1, 4, expected_after)
        .fixed("a {\n color: pink; }"),
      bad("a {color: pink; }")
        .at(1, 4, expected_after)
        .fixed("a {\ncolor: pink; }"),
      bad("a {  color: pink; }")
        .at(1, 4, expected_after)
        .fixed("a {\n  color: pink; }"),
      bad("a {\tcolor: pink; }")
        .at(1, 4, expected_after)
        .fixed("a {\n\tcolor: pink; }"),
      bad("@media print { a {\ncolor: pink; } }")
        .at(1, 15, expected_after)
        .fixed("@media print {\n a {\ncolor: pink; } }"),
      bad("@media print {\na { color: pink; } }")
        .at(2, 4, expected_after)
        .fixed("@media print {\na {\n color: pink; } }"),
      bad("@media print {\r\na { color: pink; } }")
        .at(2, 4, expected_after)
        .fixed("@media print {\r\na {\r\n color: pink; } }"),
      bad("a { /* 1 */ color: pink; }")
        .at(1, 4, expected_after)
        .fixed("a { /* 1 */\n color: pink; }"),
      bad(".a {/*.b*/.c { color: pink; } }")
        .at(1, 5, expected_after)
        .at(1, 15, expected_after)
        .fixed(".a {/*.b*/\n.c {\n color: pink; } }"),
      bad(".a {/*.b*/\n.c { color: pink; } }")
        .at(2, 5, expected_after)
        .fixed(".a {/*.b*/\n.c {\n color: pink; } }"),
      bad(".a {/*.b*/\r\n.c { color: pink; } }")
        .at(2, 5, expected_after)
        .fixed(".a {/*.b*/\r\n.c {\r\n color: pink; } }"),
      bad("a { \ncolor: pink; }")
        .at(1, 4, expected_after)
        .fixed("a {\ncolor: pink; }"),
      bad("a { \r\ncolor: pink; }")
        .at(1, 4, expected_after)
        .fixed("a {\r\ncolor: pink; }"),
    ],
  );
}

#[test]
fn block_opening_brace_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \"{\" of a multi-line block";
  check(
    "@stylistic/block-opening-brace-newline-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a {\ncolor: pink; }"),
      ok("a {\n  color: pink;\n  background: orange; }"),
      ok("a {\r\n  color: pink;\r\n  background: orange; }"),
      ok("a{\ncolor: pink; }"),
      ok("a{\n\tcolor: pink; }"),
      ok("a{\n  color: pink; }"),
      ok("@media print {\na {\ncolor: pink; } }"),
      ok("@media print {\r\na {\r\ncolor: pink; } }"),
      ok("@media print{\na{\ncolor: pink; } }"),
      ok("@media print{\n\ta{\n  color: pink; } }"),
      ok("@media print{\r\n\ta{\r\n  color: pink; } }"),
      ok("a { color: pink; }"),
      ok("a {\tcolor: pink; }"),
      ok("a {  color: pink;  background: orange; }"),
      ok("a { /* 1 */ color: pink; }"),
      ok(".a {/*.b*/.c { color: pink; } }"),
      bad("a { color: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a {\n color: pink;\nbackground: orange; }"),
      bad("a {color: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a {\ncolor: pink;\nbackground: orange; }"),
      bad("a {  color: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a {\n  color: pink;\nbackground: orange; }"),
      bad("a {\tcolor: pink;\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a {\n\tcolor: pink;\nbackground: orange; }"),
      bad("a {\tcolor: pink;\r\nbackground: orange; }")
        .at(1, 4, expected_after_multi_line)
        .fixed("a {\r\n\tcolor: pink;\r\nbackground: orange; }"),
      bad("@media print { a {\ncolor:\npink; } }")
        .at(1, 15, expected_after_multi_line)
        .fixed("@media print {\n a {\ncolor:\npink; } }"),
      bad("@media print {\na { color:\npink; } }")
        .at(2, 4, expected_after_multi_line)
        .fixed("@media print {\na {\n color:\npink; } }"),
      bad("@media print {\r\na { color:\r\npink; } }")
        .at(2, 4, expected_after_multi_line)
        .fixed("@media print {\r\na {\r\n color:\r\npink; } }"),
    ],
  );
}

#[test]
fn block_opening_brace_newline_after_never_multi_line() {
  let rejected_after_multi_line = "Unexpected whitespace after \"{\" of a multi-line block";
  check(
    "@stylistic/block-opening-brace-newline-after",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a {color: pink;\nbackground: orange; }"),
      ok("a {color: pink;\r\nbackground: orange; }"),
      ok("a{color: pink;\nbackground: orange; }"),
      ok("@media print {a {color: pink;\nbackground: orange; } }"),
      ok("@media print{a{color: pink;\nbackground: orange; } }"),
      ok("a { color: pink; }"),
      ok("a {  color: pink; }"),
      ok("a {\tcolor: pink; }"),
      ok("@media print { a { color: pink; } }"),
      ok("@media print {\ta {\tcolor: pink; } }"),
      bad("a { color: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("a {\ncolor: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("a {\r\ncolor: pink;\r\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\r\nbackground: orange; }"),
      bad("a {  color: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("a {\tcolor: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {color: pink;\nbackground: orange; }"),
      bad("@media print {\na {color: pink;\nbackground: orange; } }")
        .at(1, 15, rejected_after_multi_line)
        .fixed("@media print {a {color: pink;\nbackground: orange; } }"),
      bad("@media print {a {\ncolor: pink;\nbackground: orange; } }")
        .at(1, 18, rejected_after_multi_line)
        .fixed("@media print {a {color: pink;\nbackground: orange; } }"),
      bad("a {\n/*comment*/ color: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a {/*comment*/color: pink;\nbackground: orange; }"),
      bad("a {\n /*c1*/ /*c2*/ \n\n /*c3*/ color: pink;\nbackground: orange; }")
        .at(1, 4, rejected_after_multi_line)
        .fixed("a { /*c1*/ /*c2*/  /*c3*/color: pink;\nbackground: orange; }"),
    ],
  );
}

#[test]
fn block_opening_brace_newline_after_always_with_ignore() {
  let expected_after = "Expected newline after \"{\"";
  check(
    "@stylistic/block-opening-brace-newline-after",
    json!(["always", {"ignore": ["rules"]}]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a {\ncolor: pink; }"),
      ok("@media print {\na {\ncolor: pink; } }"),
      bad("@media print { a { color: pink; } }")
        .at(1, 15, expected_after)
        .fixed("@media print {\n a { color: pink; } }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_newline_after_always_gale_cases() {
  check(
    "@stylistic/block-opening-brace-newline-after",
    json!("always"),
    Css,
    &[
      ok("a {\n  color: red;\n}"),
      bad("a { color: red;\n}")
        .at(1, 4, "Expected newline after \"{\"")
        .fixed("a {\n color: red;\n}"),
      ok("/* { no newline } */ a {\n  color: red;\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_opening_brace_newline_after_always_multi_line_gale_cases() {
  check(
    "@stylistic/block-opening-brace-newline-after",
    json!("always-multi-line"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { color: red;\nbackground: blue; }")
        .at(1, 4, "Expected newline after \"{\" of a multi-line block")
        .fixed("a {\n color: red;\nbackground: blue; }"),
    ],
  );
}

#[test]
fn block_closing_brace_space_before_always() {
  let expected_before = "Expected single space before \"}\"";
  check(
    "@stylistic/block-closing-brace-space-before",
    json!(["always"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink; } b { color: red; }"),
      ok("a { color: pink; }b { color: red; }"),
      bad("a { color: pink;}")
        .at(1, 16, expected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink;  }")
        .at(1, 18, expected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink;\n}")
        .at(1, 17, expected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink;\r\n}")
        .at(1, 17, expected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink;\t}")
        .at(1, 17, expected_before)
        .fixed("a { color: pink; }"),
      bad("a { color: pink; } b { color: red;}")
        .at(1, 34, expected_before)
        .fixed("a { color: pink; } b { color: red; }"),
      bad("a { color: pink;} b { color: red;}")
        .at(1, 16, expected_before)
        .at(1, 33, expected_before)
        .fixed("a { color: pink; } b { color: red; }"),
      bad("a { color: pink;/*comment*/}")
        .at(1, 27, expected_before)
        .fixed("a { color: pink;/*comment*/ }"),
      bad("a { color: pink;;;}")
        .at(1, 18, expected_before)
        .fixed("a { color: pink;;; }"),
    ],
  );
}

#[test]
fn block_closing_brace_space_before_never() {
  let rejected_before = "Unexpected whitespace before \"}\"";
  check(
    "@stylistic/block-closing-brace-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a { color: pink;}"),
      ok("a { color: pink;} b { color: red;}"),
      ok("a { color: pink;}b { color: red;}"),
      bad("a { color: pink; }")
        .at(1, 17, rejected_before)
        .fixed("a { color: pink;}"),
      bad("a { color: pink;  }")
        .at(1, 18, rejected_before)
        .fixed("a { color: pink;}"),
      bad("a { color: pink;\n}")
        .at(1, 17, rejected_before)
        .fixed("a { color: pink;}"),
      bad("a { color: pink;\r\n}")
        .at(1, 17, rejected_before)
        .fixed("a { color: pink;}"),
      bad("a { color: pink;\t}")
        .at(1, 17, rejected_before)
        .fixed("a { color: pink;}"),
      bad("a { color: pink;} b { color: red; }")
        .at(1, 34, rejected_before)
        .fixed("a { color: pink;} b { color: red;}"),
      bad("a { color: pink; } b { color: red; }")
        .at(1, 17, rejected_before)
        .at(1, 35, rejected_before)
        .fixed("a { color: pink;} b { color: red;}"),
      bad("a { color: pink; /*comment*/ }")
        .at(1, 29, rejected_before)
        .fixed("a { color: pink; /*comment*/}"),
      bad("a { color: pink ; ; ; }")
        .at(1, 22, rejected_before)
        .fixed("a { color: pink ; ; ;}"),
    ],
  );
}

#[test]
fn block_closing_brace_space_before_always_single_line() {
  let expected_before_single_line = "Expected single space before \"}\" of a single-line block";
  check(
    "@stylistic/block-closing-brace-space-before",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink; } b { color: red; }"),
      ok("a { color: pink; }b { color: red; }"),
      ok("a,\nb { color: pink; } c { color: red; }"),
      ok("a { color: pink;\ntop: 0;}"),
      ok("a { color: pink;\n\ntop: 0;}"),
      ok("a { color: pink;\ntop: 0;  } b { color: red; }"),
      ok("a { color: pink;\ntop: 0;\n}b { color: red; }"),
      bad("a { color: pink;}")
        .at(1, 16, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a,\nb { color: pink;}")
        .at(2, 16, expected_before_single_line)
        .fixed("a,\nb { color: pink; }"),
      bad("a,\r\nb { color: pink;}")
        .at(2, 16, expected_before_single_line)
        .fixed("a,\r\nb { color: pink; }"),
      bad("a { color: pink;  }")
        .at(1, 18, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a { color: pink;\t}")
        .at(1, 17, expected_before_single_line)
        .fixed("a { color: pink; }"),
      bad("a { color: pink; } b { color: red;}")
        .at(1, 34, expected_before_single_line)
        .fixed("a { color: pink; } b { color: red; }"),
    ],
  );
}

#[test]
fn block_closing_brace_space_before_never_single_line() {
  let rejected_before_single_line = "Unexpected whitespace before \"}\" of a single-line block";
  check(
    "@stylistic/block-closing-brace-space-before",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a { color: pink;}"),
      ok("a { color: pink;} b { color: red;}"),
      ok("a { color: pink;}b { color: red;}"),
      ok("a,\nb { color: pink;} b { color: red;}"),
      ok("a { color: pink;\ntop: 0; }"),
      ok("a { color: pink;\r\ntop: 0; }"),
      ok("a { color: pink;\ntop: 0;  } b { color: red;}"),
      ok("a { color: pink;\ntop: 0;\n}b { color: red;}"),
      bad("a { color: pink; }")
        .at(1, 17, rejected_before_single_line)
        .fixed("a { color: pink;}"),
      bad("a,\nb { color: pink; }")
        .at(2, 17, rejected_before_single_line)
        .fixed("a,\nb { color: pink;}"),
      bad("a,\r\nb { color: pink; }")
        .at(2, 17, rejected_before_single_line)
        .fixed("a,\r\nb { color: pink;}"),
      bad("a { color: pink;  }")
        .at(1, 18, rejected_before_single_line)
        .fixed("a { color: pink;}"),
      bad("a { color: pink;\t}")
        .at(1, 17, rejected_before_single_line)
        .fixed("a { color: pink;}"),
      bad("a { color: pink;} b { color: red;\t}")
        .at(1, 34, rejected_before_single_line)
        .fixed("a { color: pink;} b { color: red;}"),
      bad("a { color: pink;  } b { color: red;}")
        .at(1, 18, rejected_before_single_line)
        .fixed("a { color: pink;} b { color: red;}"),
    ],
  );
}

#[test]
fn block_closing_brace_space_before_always_multi_line() {
  let expected_before_multi_line = "Expected single space before \"}\" of a multi-line block";
  check(
    "@stylistic/block-closing-brace-space-before",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\ntop: 0; }"),
      ok("a { color: pink;\ntop: 0; } b { color: red; }"),
      ok("a { color: pink;\ntop: 0; }b { color: red; }"),
      ok("a { color: pink;\r\ntop: 0; }b { color: red; }"),
      ok("a { color: pink;}"),
      ok("a { color: pink;  } b { color: red; }"),
      ok("a { color: pink;\t}b { color: red; }"),
      bad("a { color: pink;\ntop: 0;}")
        .at(2, 7, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0; }"),
      bad("a { color: pink;\ntop: 0;  }")
        .at(2, 9, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0; }"),
      bad("a { color: pink;\ntop: 0;\t}")
        .at(2, 8, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0; }"),
      bad("a { color: pink; } b { color: red;\ntop: 0;}")
        .at(2, 7, expected_before_multi_line)
        .fixed("a { color: pink; } b { color: red;\ntop: 0; }"),
      bad("a { color: pink;\ntop: 0;} b { color: red; }")
        .at(2, 7, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0; } b { color: red; }"),
      bad("a { color: pink;\r\ntop: 0;} b { color: red; }")
        .at(2, 7, expected_before_multi_line)
        .fixed("a { color: pink;\r\ntop: 0; } b { color: red; }"),
    ],
  );
}

#[test]
fn block_closing_brace_space_before_never_multi_line() {
  let rejected_before_multi_line = "Unexpected whitespace before \"}\" of a multi-line block";
  check(
    "@stylistic/block-closing-brace-space-before",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\ntop: 0;}"),
      ok("a { color: pink;\ntop: 0;} b { color: red;}"),
      ok("a { color: pink;\r\ntop: 0;} b { color: red;}"),
      ok("a { color: pink;\ntop: 0;}b { color: red;}"),
      ok("a { color: pink; }"),
      ok("a { color: pink;  } b { color: red; }"),
      ok("a { color: pink;\t}b { color: red; }"),
      bad("a { color: pink;\ntop: 0; }")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;}"),
      bad("a { color: pink;\ntop: 0;  }")
        .at(2, 9, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;}"),
      bad("a { color: pink;\ntop: 0;\t}")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;}"),
      bad("a { color: pink;\r\ntop: 0;\t}")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\r\ntop: 0;}"),
      bad("a { color: pink; } b { color: red;\ntop: 0; }")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink; } b { color: red;\ntop: 0;}"),
      bad("a { color: pink;\ntop: 0; } b { color: red; }")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;} b { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_closing_brace_space_before_always_single_line_gale_cases() {
  check(
    "@stylistic/block-closing-brace-space-before",
    json!("always-single-line"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { color: red;}")
        .at(
          1,
          15,
          "Expected single space before \"}\" of a single-line block",
        )
        .fixed("a { color: red; }"),
      ok("a {\n  color: red;\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_closing_brace_space_before_never_single_line_gale_cases() {
  check(
    "@stylistic/block-closing-brace-space-before",
    json!("never-single-line"),
    Css,
    &[
      ok("a { color: red;}"),
      bad("a { color: red; }")
        .at(
          1,
          16,
          "Unexpected whitespace before \"}\" of a single-line block",
        )
        .fixed("a { color: red;}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_closing_brace_space_before_always_gale_cases() {
  check(
    "@stylistic/block-closing-brace-space-before",
    json!("always"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a { color: red;}")
        .at(1, 15, "Expected single space before \"}\"")
        .fixed("a { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_closing_brace_space_before_never_gale_cases() {
  check(
    "@stylistic/block-closing-brace-space-before",
    json!("never"),
    Css,
    &[
      ok("a { color: red;}"),
      bad("a { color: red; }")
        .at(1, 16, "Unexpected whitespace before \"}\"")
        .fixed("a { color: red;}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_after_always() {
  let expected_after = "Expected newline after \"}\"";
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!(["always"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink; }\nb { color: red; }"),
      ok("a { color: pink; }\r\nb { color: red; }"),
      ok("a { color: pink; }\n\nb { color: red; }"),
      ok("a { color: pink; }\r\n\r\nb { color: red; }"),
      ok("a { color: pink;}\n\t\tb { color: red;}"),
      ok("a { color: pink;}\r\n\t\tb { color: red;}"),
      ok("a { @extend foo; color: pink; }"),
      ok("a { @extend foo; /* comment */\ncolor: pink;  }"),
      ok("@media print { a { color: pink; }\nb { color: red; }}"),
      ok("@media print { a { color: pink; }}\n@media screen { b { color: red; }}"),
      ok(".a {} /* comment */"),
      ok(".a {} /* comment */\n b {}"),
      bad("a { color: pink; }b { color: red; }")
        .at(1, 19, expected_after)
        .fixed("a { color: pink; }\nb { color: red; }"),
      bad("a { color: pink; } b { color: red; }")
        .at(1, 19, expected_after)
        .fixed("a { color: pink; }\n b { color: red; }"),
      bad("a { color: pink; }  b { color: red; }")
        .at(1, 19, expected_after)
        .fixed("a { color: pink; }\n  b { color: red; }"),
      bad("a { color: pink; }\tb { color: red; }")
        .at(1, 19, expected_after)
        .fixed("a { color: pink; }\n\tb { color: red; }"),
      bad("@media print { a { color: pink; } b { color: red; }}")
        .at(1, 34, expected_after)
        .fixed("@media print { a { color: pink; }\n b { color: red; }}"),
      bad("@media print { a { color: pink; }} @media screen { b { color: red; }}")
        .at(1, 35, expected_after)
        .fixed("@media print { a { color: pink; }}\n @media screen { b { color: red; }}"),
      bad(".a {} /* comment */ b {}")
        .at(1, 6, expected_after)
        .fixed(".a {} /* comment */\n b {}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_after_always_with_ignoreatrules() {
  let expected_after = "Expected newline after \"}\"";
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!(["always", {"ignoreAtRules": ["if", "else"]}]),
    Css,
    &[
      ok("a { color: pink; }\nb {}"),
      ok("@if ... { color: pink; } @else {}"),
      ok("@if ... { color: pink; } @else if {} else {}"),
      ok("@if ... {\r\n  color: pink; \n} @else if {\n  color: pink;\n} else {}"),
      bad("a { color: pink; }b{}")
        .at(1, 19, expected_after)
        .fixed("a { color: pink; }\nb{}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_after_always_with_ignoreatrules_2() {
  let expected_after = "Expected newline after \"}\"";
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!(["always", {"ignoreAtRules": "/if/"}]),
    Css,
    &[
      ok("a { color: pink; }\nb {}"),
      ok("@if ... { color: pink; } @else {}"),
      bad("a { color: pink; }b{}")
        .at(1, 19, expected_after)
        .fixed("a { color: pink; }\nb{}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_after_always_single_line() {
  let expected_after_single_line = "Expected newline after \"}\" of a single-line block";
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink; }\nb { color: red; }"),
      ok("a { color: pink; }\r\nb { color: red; }"),
      ok("a { color: pink;}\n\t\tb { color: red;}"),
      ok("a { color: pink;}\r\n\t\tb { color: red;}"),
      ok("@media print { a { color: pink; }\nb { color: red; }}"),
      ok("@media print { a { color: pink; }}\n@media screen { b { color: red; }}"),
      ok("a { color: pink;\ntop: 0; }b { color: red; }"),
      ok("a { color: pink;\ntop: 0;}b { color: red;}"),
      ok("a { color: pink;\r\ntop: 0;}b { color: red;}"),
      bad("a { color: pink; }b { color: red; }")
        .at(1, 19, expected_after_single_line)
        .fixed("a { color: pink; }\nb { color: red; }"),
      bad("a { color: pink; } b { color: red; }")
        .at(1, 19, expected_after_single_line)
        .fixed("a { color: pink; }\n b { color: red; }"),
      bad("a { color: pink; }  b { color: red; }")
        .at(1, 19, expected_after_single_line)
        .fixed("a { color: pink; }\n  b { color: red; }"),
      bad("a { color: pink; }\tb { color: red; }")
        .at(1, 19, expected_after_single_line)
        .fixed("a { color: pink; }\n\tb { color: red; }"),
      bad("@media print { a { color: pink; } b { color: red; }}")
        .at(1, 34, expected_after_single_line)
        .fixed("@media print { a { color: pink; }\n b { color: red; }}"),
      bad("@media print { a { color: pink; }} @media screen { b { color: red; }}")
        .at(1, 35, expected_after_single_line)
        .fixed("@media print { a { color: pink; }}\n @media screen { b { color: red; }}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_after_never_single_line() {
  let rejected_after_single_line = "Unexpected whitespace after \"}\" of a single-line block";
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a { color: pink; }"),
      ok("a { color: pink; }b { color: red; }"),
      ok("a { color: pink;}b { color: red;}"),
      ok("@media print { a { color: pink; }b { color: red; }}"),
      ok("@media print { a { color: pink; }}@media screen { b { color: red; }}"),
      ok("a { color: pink;\ntop: 0; }\nb { color: red; }"),
      ok("a { color: pink;\ntop: 0;} b { color: red;}"),
      bad("a { color: pink; }\nb { color: red; }")
        .at(1, 19, rejected_after_single_line)
        .fixed("a { color: pink; }b { color: red; }"),
      bad("a { color: pink; } b { color: red; }")
        .at(1, 19, rejected_after_single_line)
        .fixed("a { color: pink; }b { color: red; }"),
      bad("a { color: pink; }  b { color: red; }")
        .at(1, 19, rejected_after_single_line)
        .fixed("a { color: pink; }b { color: red; }"),
      bad("a { color: pink; }\tb { color: red; }")
        .at(1, 19, rejected_after_single_line)
        .fixed("a { color: pink; }b { color: red; }"),
      bad("@media print { a { color: pink; }\nb { color: red; }}")
        .at(1, 34, rejected_after_single_line)
        .fixed("@media print { a { color: pink; }b { color: red; }}"),
      bad("@media print { a { color: pink; }}\n @media screen { b { color: red; }}")
        .at(1, 35, rejected_after_single_line)
        .fixed("@media print { a { color: pink; }}@media screen { b { color: red; }}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \"}\" of a multi-line block";
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\ntop: 0; }"),
      ok("a { color: pink;\ntop: 0; }\nb { color: red; }"),
      ok("a { color: pink;\r\ntop: 0; }\r\nb { color: red; }"),
      ok("a { color: pink;\ntop: 0;}\n\t\tb { color: red;}"),
      ok("@media print { a {\ncolor: pink; }\nb { color: red; }}"),
      ok("@media print { a {\ncolor: pink; }}\n@media screen { b { color: red; }}"),
      ok("a { color: pink; }\nb { color: red; }"),
      ok("a { color: pink; }b { color: red;}"),
      bad("a { color: pink;\ntop: 0; }b { color: red; }")
        .at(2, 10, expected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }\nb { color: red; }"),
      bad("a { color: pink;\r\ntop: 0; }b { color: red; }")
        .at(2, 10, expected_after_multi_line)
        .fixed("a { color: pink;\r\ntop: 0; }\r\nb { color: red; }"),
      bad("a { color: pink;\ntop: 0; } b { color: red; }")
        .at(2, 10, expected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }\n b { color: red; }"),
      bad("a { color: pink;\ntop: 0; }  b { color: red; }")
        .at(2, 10, expected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }\n  b { color: red; }"),
      bad("a { color: pink;\ntop: 0; }\tb { color: red; }")
        .at(2, 10, expected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }\n\tb { color: red; }"),
      bad("@media print { a {\ncolor: pink; } b { color: red; }}")
        .at(2, 15, expected_after_multi_line)
        .fixed("@media print { a {\ncolor: pink; }\n b { color: red; }}"),
      bad("@media print { a {\ncolor: pink; }} @media screen { b {\ncolor: red; }}")
        .at(2, 16, expected_after_multi_line)
        .fixed("@media print { a {\ncolor: pink; }}\n @media screen { b {\ncolor: red; }}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_after_never_multi_line() {
  let rejected_after_multi_line = "Unexpected whitespace after \"}\" of a multi-line block";
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\ntop: 0; }"),
      ok("a { color: pink;\ntop: 0; }b { color: red; }"),
      ok("a { color: pink;\ntop: 0;}b { color: red;}"),
      ok("a { color: pink;\r\ntop: 0;}b { color: red;}"),
      ok("@media print { a {\ncolor: pink; }b { color: red; }}"),
      ok("@media print { a {\ncolor: pink; }}@media screen { b { color: red; }}"),
      ok("@media print { a {\r\ncolor: pink; }}@media screen { b { color: red; }}"),
      ok("a { color: pink; }\nb { color: red; }"),
      ok("a { color: pink; }\r\nb { color: red; }"),
      ok("a { color: pink;} b { color: red;}"),
      bad("a { color: pink;\ntop: 0; }\nb { color: red; }")
        .at(2, 10, rejected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }b { color: red; }"),
      bad("a { color: pink;\r\ntop: 0; }\r\nb { color: red; }")
        .at(2, 10, rejected_after_multi_line)
        .fixed("a { color: pink;\r\ntop: 0; }b { color: red; }"),
      bad("a { color: pink;\ntop: 0; } b { color: red; }")
        .at(2, 10, rejected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }b { color: red; }"),
      bad("a { color: pink;\ntop: 0; }  b { color: red; }")
        .at(2, 10, rejected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }b { color: red; }"),
      bad("a { color: pink;\ntop: 0; }\tb { color: red; }")
        .at(2, 10, rejected_after_multi_line)
        .fixed("a { color: pink;\ntop: 0; }b { color: red; }"),
      bad("@media print { a {\ncolor: pink; }\nb { color: red; }}")
        .at(2, 15, rejected_after_multi_line)
        .fixed("@media print { a {\ncolor: pink; }b { color: red; }}"),
      bad("@media print { a {\ncolor: pink; }}\n@media screen { b {\ncolor: red; }}")
        .at(2, 16, rejected_after_multi_line)
        .fixed("@media print { a {\ncolor: pink; }}@media screen { b {\ncolor: red; }}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_closing_brace_newline_after_always_gale_cases() {
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!("always"),
    Css,
    &[
      ok("a { color: red; }\nb { color: blue; }"),
      bad("a { color: red; } b { color: blue; }")
        .at(1, 18, "Expected newline after \"}\"")
        .fixed("a { color: red; }\n b { color: blue; }"),
      bad("@if $cond { color: red; } @else { color: blue; }")
        .at(1, 26, "Expected newline after \"}\"")
        .fixed("@if $cond { color: red; }\n @else { color: blue; }"),
      bad("@if $a { color: red; } @else if $b { color: blue; } @else { color: green; }")
        .at(1, 23, "Expected newline after \"}\"")
        .at(1, 52, "Expected newline after \"}\"")
        .fixed("@if $a { color: red; }\n @else if $b { color: blue; }\n @else { color: green; }"),
      ok("@function foo() {\n  @return 1;\n};\n"),
    ],
  );
}

// Gale's own `"never"`, which the plugin does not take: no line break
// next to the character.  Gale adds no fix for it.
#[test]
fn block_closing_brace_newline_after_never_gale_only() {
  check(
    "@stylistic/block-closing-brace-newline-after",
    json!("never"),
    Css,
    &[bad("a { color: red; }\nb { }").at(1, 18, "Unexpected newline after \"}\"")],
  );
}

#[test]
fn block_closing_brace_newline_before_always() {
  let expected_before = "Expected newline before \"}\"";
  check(
    "@stylistic/block-closing-brace-newline-before",
    json!(["always"]),
    Css,
    &[
      ok("a { color: pink;\n}"),
      ok("a { color: pink;;\n}"),
      ok("a { color: pink;;;\n}"),
      ok("a { color: pink;\r\n}"),
      ok("a { color: pink;\n\n}"),
      ok("a { color: pink;\r\n\r\n}"),
      ok("a { color: pink;\n\t\t}"),
      ok("a { color: pink;\n} b { color: red;\n}"),
      ok("a { color: pink;\n}b { color: red;\n}"),
      ok("@media print {\n  a {\n     color: pink;\n  }\n}"),
      ok("@media print {\n\ta {\n\t\tcolor: pink;\n\t\t{\n\t\t\t&:hover;\n\t\t\t}\n\t\t}\n}"),
      bad("a { color: pink;}")
        .at(1, 16, expected_before)
        .fixed("a { color: pink;\n}"),
      bad("a { color: pink;;}")
        .at(1, 17, expected_before)
        .fixed("a { color: pink;;\n}"),
      bad("a { color: pink; }")
        .at(1, 17, expected_before)
        .fixed("a { color: pink;\n }"),
      bad("a { color: pink; \n}")
        .at(1, 18, expected_before)
        .fixed("a { color: pink;\n}"),
      bad("a { color: pink; \r\n}")
        .at(1, 18, expected_before)
        .fixed("a { color: pink;\r\n}"),
      bad("a { color: pink;  }")
        .at(1, 18, expected_before)
        .fixed("a { color: pink;\n  }"),
      bad("a { color: pink;\t}")
        .at(1, 17, expected_before)
        .fixed("a { color: pink;\n\t}"),
      bad("a { color: pink;\n} b { color: red; }")
        .at(2, 18, expected_before)
        .fixed("a { color: pink;\n} b { color: red;\n }"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_before_always_multi_line() {
  let expected_before_multi_line = "Expected newline before \"}\" of a multi-line block";
  check(
    "@stylistic/block-closing-brace-newline-before",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\ntop: 0;\n}"),
      ok("a { color: pink;\r\ntop: 0;\r\n}"),
      ok("a { color: pink;;\ntop: 0;;\n}"),
      ok("a { color: pink;;\r\ntop: 0;;\r\n}"),
      ok("a { color: pink;;;\ntop: 0;;;\n}"),
      ok("a { color: pink;;;\r\ntop: 0;;;\r\n}"),
      ok("a { color: pink;\ntop: 0;\n\t\t}"),
      ok("a { color: pink;\r\ntop: 0;\r\n\t\t}"),
      ok("a { color: pink;\ntop: 0;\n} b { color: red;\n}"),
      ok("a { color: pink;\ntop: 0;\n}b { color: red;\n}"),
      ok("a { color: pink;}"),
      ok("a { color: pink;;}"),
      ok("a { color: pink;;;}"),
      ok("a { color: pink;} b { color: red;}"),
      ok("a { color: pink;}b { color: red;}"),
      bad("a { color: pink;\ntop: 0;}")
        .at(2, 7, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;\n}"),
      bad("a { color: pink;\r\ntop: 0;}")
        .at(2, 7, expected_before_multi_line)
        .fixed("a { color: pink;\r\ntop: 0;\r\n}"),
      bad("a { color: pink;;\ntop: 0;;}")
        .at(2, 8, expected_before_multi_line)
        .fixed("a { color: pink;;\ntop: 0;;\n}"),
      bad("a { color: pink;;\r\ntop: 0;;}")
        .at(2, 8, expected_before_multi_line)
        .fixed("a { color: pink;;\r\ntop: 0;;\r\n}"),
      bad("a { color: pink;;;\ntop: 0;;;}")
        .at(2, 9, expected_before_multi_line)
        .fixed("a { color: pink;;;\ntop: 0;;;\n}"),
      bad("a { color: pink;;;\r\ntop: 0;;;}")
        .at(2, 9, expected_before_multi_line)
        .fixed("a { color: pink;;;\r\ntop: 0;;;\r\n}"),
      bad("a { color: pink;\ntop: 0; }")
        .at(2, 8, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;\n }"),
      bad("a { color: pink;\ntop: 0; \n}")
        .at(2, 9, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;\n}"),
      bad("a { color: pink;\ntop: 0;  }")
        .at(2, 9, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;\n  }"),
      bad("a { color: pink;\ntop: 0;\t}")
        .at(2, 8, expected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;\n\t}"),
    ],
  );
}

#[test]
fn block_closing_brace_newline_before_never_multi_line() {
  let rejected_before_multi_line = "Unexpected whitespace before \"}\" of a multi-line block";
  check(
    "@stylistic/block-closing-brace-newline-before",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a { color: pink;\ntop: 0;}"),
      ok("a { color: pink;\r\ntop: 0;}"),
      ok("a { color: pink;;\ntop: 0;;}"),
      ok("a { color: pink;;\r\ntop: 0;;}"),
      ok("a { color: pink;;;\ntop: 0;;;}"),
      ok("a { color: pink;;;\r\ntop: 0;;;}"),
      ok("a { color: pink;\ntop: 0;} b { color: red;\ntop: 0;}"),
      ok("a { color: pink;\ntop: 0;}b { color: red;\ntop: 0;}"),
      ok("a { color: pink; }"),
      ok("a { color: pink;\t}"),
      ok("a { color: pink;  }"),
      bad("a { color: pink;\ntop: 0; }")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;}"),
      bad("a { color: pink;\r\ntop: 0; }")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\r\ntop: 0;}"),
      bad("a { color: pink;;\ntop: 0;; }")
        .at(2, 9, rejected_before_multi_line)
        .fixed("a { color: pink;;\ntop: 0;;}"),
      bad("a { color: pink;;\r\ntop: 0;; }")
        .at(2, 9, rejected_before_multi_line)
        .fixed("a { color: pink;;\r\ntop: 0;;}"),
      bad("a { color: pink;;;\ntop: 0;;; }")
        .at(2, 10, rejected_before_multi_line)
        .fixed("a { color: pink;;;\ntop: 0;;;}"),
      bad("a { color: pink;;;\r\ntop: 0;;; }")
        .at(2, 10, rejected_before_multi_line)
        .fixed("a { color: pink;;;\r\ntop: 0;;;}"),
      bad("a { color: pink;\ntop: 0;\n}")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;}"),
      bad("a { color: pink;\ntop: 0;  }")
        .at(2, 9, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;}"),
      bad("a { color: pink;\ntop: 0;\t}")
        .at(2, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;}"),
      bad("a { color: pink;\ntop: 0;} b { color: red;\ntop: 0;\n}")
        .at(3, 8, rejected_before_multi_line)
        .fixed("a { color: pink;\ntop: 0;} b { color: red;\ntop: 0;}"),
      bad("a { color: pink;;;\ntop: 0; ;;}")
        .at(2, 10, rejected_before_multi_line)
        .fixed("a { color: pink;;;\ntop: 0;;;}"),
      bad("a { color: pink;;;\ntop: 0;; ;}")
        .at(2, 10, rejected_before_multi_line)
        .fixed("a { color: pink;;;\ntop: 0;;;}"),
      bad("a { color: pink;;;\ntop: 0; ; ; }")
        .at(2, 12, rejected_before_multi_line)
        .fixed("a { color: pink;;;\ntop: 0;;;}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_closing_brace_newline_before_always_gale_cases() {
  check(
    "@stylistic/block-closing-brace-newline-before",
    json!("always"),
    Css,
    &[
      ok("a {\n  color: red;\n}"),
      bad("a { color: red; }")
        .at(1, 16, "Expected newline before \"}\"")
        .fixed("a { color: red;\n }"),
    ],
  );
}

// Gale's own `"never"`, which the plugin does not take: no line break
// next to the character.  Gale adds no fix for it.
#[test]
fn block_closing_brace_newline_before_never_gale_only() {
  check(
    "@stylistic/block-closing-brace-newline-before",
    json!("never"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a {\n  color: red;\n}").at(2, 14, "Unexpected newline before \"}\""),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn block_closing_brace_newline_before_always_multi_line_gale_cases() {
  check(
    "@stylistic/block-closing-brace-newline-before",
    json!("always-multi-line"),
    Css,
    &[
      ok("a { color: red; }"),
      bad("a {\n  color: red; }")
        .at(2, 14, "Expected newline before \"}\" of a multi-line block")
        .fixed("a {\n  color: red;\n }"),
    ],
  );
}
