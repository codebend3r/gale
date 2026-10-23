//! The `comma` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn selector_list_comma_space_after_always() {
  let expected_after = "Expected single space after \",\"";
  check(
    "@stylistic/selector-list-comma-space-after",
    json!(["always"]),
    Css,
    &[
      ok("a, b {}"),
      ok("a, b, c {}"),
      ok("a , b {}"),
      ok("a\n, b {}"),
      ok("a\r\n, b {}"),
      ok("a, b[data-foo=\"tr,tr\"] {}"),
      ok("a:matches(:hover,:focus) {}"),
      ok(":not(:hover,:focus) {}"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a/*comment,comment*/, /*comment*/b {}"),
      bad("a,b {}").at(1, 2, expected_after).fixed("a, b {}"),
      bad("a,  b {}").at(1, 2, expected_after).fixed("a, b {}"),
      bad("a,\nb {}").at(1, 2, expected_after).fixed("a, b {}"),
      bad("a,\r\nb {}").at(1, 2, expected_after).fixed("a, b {}"),
      bad("a,\tb {}").at(1, 2, expected_after).fixed("a, b {}"),
      bad("a, b,c {}")
        .at(1, 5, expected_after)
        .fixed("a, b, c {}"),
      bad("a, b,  c {}")
        .at(1, 5, expected_after)
        .fixed("a, b, c {}"),
      bad("a/*comment*/,/*comment*/b {}")
        .at(1, 13, expected_after)
        .fixed("a/*comment*/, /*comment*/b {}"),
      bad("a,b,c {}")
        .at(1, 2, expected_after)
        .at(1, 4, expected_after)
        .fixed("a, b, c {}"),
      bad("a,b,c,d,e,f,g {}")
        .at(1, 2, expected_after)
        .at(1, 4, expected_after)
        .at(1, 6, expected_after)
        .at(1, 8, expected_after)
        .at(1, 10, expected_after)
        .at(1, 12, expected_after)
        .fixed("a, b, c, d, e, f, g {}"),
    ],
  );
}

#[test]
fn selector_list_comma_space_after_never() {
  let rejected_after = "Unexpected whitespace after \",\"";
  check(
    "@stylistic/selector-list-comma-space-after",
    json!(["never"]),
    Css,
    &[
      ok("a,b {}"),
      ok("a,b,c {}"),
      ok("a ,b {}"),
      ok("a\n,b {}"),
      ok("a\r\n,b {}"),
      ok("a,b[data-foo=\"tr, tr\"] {}"),
      ok("a:matches(:hover, :focus) {}"),
      ok(":not(:hover, :focus) {}"),
      ok("a/*comment, comment*/,/*comment*/b {}"),
      bad("a, b {}").at(1, 2, rejected_after).fixed("a,b {}"),
      bad("a,  b {}").at(1, 2, rejected_after).fixed("a,b {}"),
      bad("a,\nb {}").at(1, 2, rejected_after).fixed("a,b {}"),
      bad("a,\r\nb {}").at(1, 2, rejected_after).fixed("a,b {}"),
      bad("a,\tb {}").at(1, 2, rejected_after).fixed("a,b {}"),
      bad("a,b, c {}").at(1, 4, rejected_after).fixed("a,b,c {}"),
      bad("a,b,  c {}").at(1, 4, rejected_after).fixed("a,b,c {}"),
      bad("a/*comment*/, /*comment*/b {}")
        .at(1, 13, rejected_after)
        .fixed("a/*comment*/,/*comment*/b {}"),
      bad("a, b, c {}")
        .at(1, 2, rejected_after)
        .at(1, 5, rejected_after)
        .fixed("a,b,c {}"),
    ],
  );
}

#[test]
fn selector_list_comma_space_after_always_single_line() {
  let expected_after_single_line = "Expected single space after \",\" in a single-line list";
  check(
    "@stylistic/selector-list-comma-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a, b {}"),
      ok("a, b {\n}"),
      ok("a, b {\r\n}"),
      bad("a,b {}")
        .at(1, 2, expected_after_single_line)
        .fixed("a, b {}"),
      bad("a,b {\n}")
        .at(1, 2, expected_after_single_line)
        .fixed("a, b {\n}"),
      bad("a,b {\r\n}")
        .at(1, 2, expected_after_single_line)
        .fixed("a, b {\r\n}"),
    ],
  );
}

#[test]
fn selector_list_comma_space_after_never_single_line() {
  let rejected_after_single_line = "Unexpected whitespace after \",\" in a single-line list";
  check(
    "@stylistic/selector-list-comma-space-after",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a,b {}"),
      ok("a,b {\n}"),
      ok("a,b {\r\n}"),
      bad("a, b {}")
        .at(1, 2, rejected_after_single_line)
        .fixed("a,b {}"),
      bad("a, b {\n}")
        .at(1, 2, rejected_after_single_line)
        .fixed("a,b {\n}"),
      bad("a, b {\r\n}")
        .at(1, 2, rejected_after_single_line)
        .fixed("a,b {\r\n}"),
    ],
  );
}

#[test]
fn selector_list_comma_space_after_always_less() {
  check(
    "@stylistic/selector-list-comma-space-after",
    json!(["always"]),
    Less,
    &[ok(".col( @a,@b ) {}"), ok(".col3( @a,@b ) {}")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_space_after_always_gale_cases() {
  check(
    "@stylistic/selector-list-comma-space-after",
    json!("always"),
    Css,
    &[
      ok("a, b { color: red; }"),
      bad("a,b { color: red; }")
        .at(1, 2, "Expected single space after \",\"")
        .fixed("a, b { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_space_after_never_gale_cases() {
  check(
    "@stylistic/selector-list-comma-space-after",
    json!("never"),
    Css,
    &[
      ok("a,b { color: red; }"),
      bad("a, b { color: red; }")
        .at(1, 2, "Unexpected whitespace after \",\"")
        .fixed("a,b { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_space_after_always_single_line_gale_cases() {
  check(
    "@stylistic/selector-list-comma-space-after",
    json!("always-single-line"),
    Css,
    &[ok("a, b { color: red; }"), ok("a,\nb { color: red; }")],
  );
}

#[test]
fn selector_list_comma_space_before_always() {
  let expected_before = "Expected single space before \",\"";
  check(
    "@stylistic/selector-list-comma-space-before",
    json!(["always"]),
    Css,
    &[
      ok("a ,b {}"),
      ok("a ,b ,c {}"),
      ok("a , b {}"),
      ok("a ,\nb {}"),
      ok("a ,\r\nb {}"),
      ok("a ,b[data-foo=\"tr,tr\"] {}"),
      ok("a:matches(:hover, :focus) {}"),
      ok(":not(:hover, :focus) {}"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a/*comment,comment*/ ,/*comment*/b {}"),
      bad("a,b {}").at(1, 2, expected_before).fixed("a ,b {}"),
      bad("a  ,b {}").at(1, 4, expected_before).fixed("a ,b {}"),
      bad("a\n,b {}").at(2, 1, expected_before).fixed("a ,b {}"),
      bad("a\r\n,b {}").at(2, 1, expected_before).fixed("a ,b {}"),
      bad("a\t,b {}").at(1, 3, expected_before).fixed("a ,b {}"),
      bad("a ,b,c {}")
        .at(1, 5, expected_before)
        .fixed("a ,b ,c {}"),
      bad("a ,b  ,c {}")
        .at(1, 7, expected_before)
        .fixed("a ,b ,c {}"),
      bad("a,b,c {}")
        .at(1, 2, expected_before)
        .at(1, 4, expected_before)
        .fixed("a ,b ,c {}"),
      bad("a/*comment*/,/*comment*/b {}")
        .at(1, 13, expected_before)
        .fixed("a/*comment*/ ,/*comment*/b {}"),
      bad("a,b,c,d,e,f,g {}")
        .at(1, 2, expected_before)
        .at(1, 4, expected_before)
        .at(1, 6, expected_before)
        .at(1, 8, expected_before)
        .at(1, 10, expected_before)
        .at(1, 12, expected_before)
        .fixed("a ,b ,c ,d ,e ,f ,g {}"),
    ],
  );
}

#[test]
fn selector_list_comma_space_before_never() {
  let rejected_before = "Unexpected whitespace before \",\"";
  check(
    "@stylistic/selector-list-comma-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a,b {}"),
      ok("a,b,c {}"),
      ok("a, b {}"),
      ok("a,\nb {}"),
      ok("a,\r\nb {}"),
      ok("a,b[data-foo=\"tr ,tr\"] {}"),
      ok("a:matches(:hover , :focus) {}"),
      ok(":not(:hover , :focus) {}"),
      ok("a/*comment ,comment*/,/*comment*/b {}"),
      bad("a ,b {}").at(1, 3, rejected_before).fixed("a,b {}"),
      bad("a  ,b {}").at(1, 4, rejected_before).fixed("a,b {}"),
      bad("a\n,b {}").at(2, 1, rejected_before).fixed("a,b {}"),
      bad("a\r\n,b {}").at(2, 1, rejected_before).fixed("a,b {}"),
      bad("a\t,b {}").at(1, 3, rejected_before).fixed("a,b {}"),
      bad("a,b ,c {}").at(1, 5, rejected_before).fixed("a,b,c {}"),
      bad("a,b  ,c {}")
        .at(1, 6, rejected_before)
        .fixed("a,b,c {}"),
      bad("a/*comment*/ ,/*comment*/b {}")
        .at(1, 14, rejected_before)
        .fixed("a/*comment*/,/*comment*/b {}"),
    ],
  );
}

#[test]
fn selector_list_comma_space_before_always_single_line() {
  let expected_before_single_line = "Expected single space before \",\" in a single-line list";
  check(
    "@stylistic/selector-list-comma-space-before",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a ,b {}"),
      ok("a ,b {\n}"),
      ok("a ,b {\r\n}"),
      bad("a,b {}")
        .at(1, 2, expected_before_single_line)
        .fixed("a ,b {}"),
      bad("a,b {\n}")
        .at(1, 2, expected_before_single_line)
        .fixed("a ,b {\n}"),
      bad("a,b {\r\n}")
        .at(1, 2, expected_before_single_line)
        .fixed("a ,b {\r\n}"),
      bad("a/*comment*/,/*comment*/b {\n}")
        .at(1, 13, expected_before_single_line)
        .fixed("a/*comment*/ ,/*comment*/b {\n}"),
    ],
  );
}

#[test]
fn selector_list_comma_space_before_never_single_line() {
  let rejected_before_single_line = "Unexpected whitespace before \",\" in a single-line list";
  check(
    "@stylistic/selector-list-comma-space-before",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a,b {}"),
      ok("a,b {\n}"),
      ok("a,b {\r\n}"),
      bad("a ,b {}")
        .at(1, 3, rejected_before_single_line)
        .fixed("a,b {}"),
      bad("a ,b {\n}")
        .at(1, 3, rejected_before_single_line)
        .fixed("a,b {\n}"),
      bad("a ,b {\r\n}")
        .at(1, 3, rejected_before_single_line)
        .fixed("a,b {\r\n}"),
      bad("a/*comment*/ ,/*comment*/b {\n}")
        .at(1, 14, rejected_before_single_line)
        .fixed("a/*comment*/,/*comment*/b {\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_space_before_never_gale_cases() {
  check(
    "@stylistic/selector-list-comma-space-before",
    json!("never"),
    Css,
    &[
      ok("a,\nb { color: red; }"),
      bad("a ,\nb { color: red; }")
        .at(1, 3, "Unexpected whitespace before \",\"")
        .fixed("a,\nb { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_space_before_always_gale_cases() {
  check(
    "@stylistic/selector-list-comma-space-before",
    json!("always"),
    Css,
    &[
      ok("a ,\nb { color: red; }"),
      bad("a,\nb { color: red; }")
        .at(1, 2, "Expected single space before \",\"")
        .fixed("a ,\nb { color: red; }"),
    ],
  );
}

#[test]
fn selector_list_comma_newline_after_always() {
  let expected_after = "Expected newline after \",\"";
  check("@stylistic/selector-list-comma-newline-after", json!(["always"]), Css, &[
    ok("a,\nb {}"),
    ok("a,\n\nb {}"),
    ok("a,\r\nb {}"),
    ok("a,\r\n\r\nb {}"),
    ok("a,\nb,\nc {}"),
    ok("a ,\nb {}"),
    ok("a\n,\nb {}"),
    ok("a\r\n,\r\nb {}"),
    ok("a,\nb[data-foo=\"tr,tr\"] {}"),
    ok("a {\n  &:hover,\n  &:focus {\n    color: pink; }\n}"),
    ok("@media (min-width: 10px) {\n  a,\n  b {}\n}"),
    ok("@media (min-width: 10px) {\r\n  a,\r\n  b {}\r\n}"),
    ok("\ta,\n\tb {}"),
    ok("a, /* comment */\nb {}"),
    ok("a,   /* comment */\nb {}"),
    ok("a,\t/* comment */\nb {}"),
    ok("a,\t\t/* comment */\nb {}"),
    ok("a, \t \t /* comment */\nb {}"),
    ok("a, /* comment\n       commentline2 */\nb {}"),
    ok("a,   /* comment\n       commentline2 */\nb {}"),
    ok("a,\t/* comment\n       commentline2 */\nb {}"),
    ok("a,\t\t/* comment\n       commentline2 */\nb {}"),
    ok("a, \t \t /* comment\n       commentline2 */\nb {}"),
    ok("a:matches(:hover, :focus) {}"),
    ok(":not(:hover, :focus) {}"),
    ok(":root { --foo: 1px; }"),
    ok("html { --foo: 1px; }"),
    ok(":root { --custom-property-set: {} }"),
    ok("html { --custom-property-set: {} }"),
    bad("a,b {}").at(1, 2, expected_after).fixed("a,\nb {}"),
    bad("a, b {}").at(1, 2, expected_after).fixed("a,\n b {}"),
    bad("a,  b {}").at(1, 2, expected_after).fixed("a,\n  b {}"),
    bad("a,\tb {}").at(1, 2, expected_after).fixed("a,\n\tb {}"),
    bad("a,\nb,c {}").at(2, 2, expected_after).fixed("a,\nb,\nc {}"),
    bad("a,\r\nb,c {}").at(2, 2, expected_after).fixed("a,\r\nb,\r\nc {}"),
    bad("a, /* comment */ b {}").at(1, 2, expected_after).fixed("a, /* comment */\n b {}"),
    bad("a, /* comment\n       commentline2 */b {}").at(1, 2, expected_after).fixed("a, /* comment\n       commentline2 */\nb {}"),
    bad("a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p,q,r,s,t,u,v,w,x,y,z {\n}").at(1, 2, expected_after).at(1, 4, expected_after).at(1, 6, expected_after).at(1, 8, expected_after).at(1, 10, expected_after).at(1, 12, expected_after).at(1, 14, expected_after).at(1, 16, expected_after).at(1, 18, expected_after).at(1, 20, expected_after).at(1, 22, expected_after).at(1, 24, expected_after).at(1, 26, expected_after).at(1, 28, expected_after).at(1, 30, expected_after).at(1, 32, expected_after).at(1, 34, expected_after).at(1, 36, expected_after).at(1, 38, expected_after).at(1, 40, expected_after).at(1, 42, expected_after).at(1, 44, expected_after).at(1, 46, expected_after).at(1, 48, expected_after).at(1, 50, expected_after).fixed("a,\nb,\nc,\nd,\ne,\nf,\ng,\nh,\ni,\nj,\nk,\nl,\nm,\nn,\no,\np,\nq,\nr,\ns,\nt,\nu,\nv,\nw,\nx,\ny,\nz {\n}"),
  ]);
}

#[test]
fn selector_list_comma_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \",\" in a multi-line list";
  check(
    "@stylistic/selector-list-comma-newline-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a,\nb {}"),
      ok("a,\r\nb {}"),
      ok("a, b {}"),
      ok("a, b {\n}"),
      ok("a, b {\r\n}"),
      ok("\ta,\n\tb {\n}"),
      bad("a,\nb, c {}")
        .at(2, 2, expected_after_multi_line)
        .fixed("a,\nb,\n c {}"),
      bad("a,\nb, c {\n}")
        .at(2, 2, expected_after_multi_line)
        .fixed("a,\nb,\n c {\n}"),
      bad("a,\r\nb, c {\r\n}")
        .at(2, 2, expected_after_multi_line)
        .fixed("a,\r\nb,\r\n c {\r\n}"),
    ],
  );
}

#[test]
fn selector_list_comma_newline_after_never_multi_line() {
  let rejected_after_multi_line = "Unexpected whitespace after \",\" in a multi-line list";
  check("@stylistic/selector-list-comma-newline-after", json!(["never-multi-line"]), Css, &[
    ok("a\n,b {}"),
    ok("a ,b {}"),
    ok("a ,b {\n}"),
    ok("a:matches(:hover, :focus) {}"),
    ok(":not(:hover, :focus) {}"),
    bad("a,\nb ,c {}").at(1, 2, rejected_after_multi_line).fixed("a,b ,c {}"),
    bad("a,\r\nb ,c {}").at(1, 2, rejected_after_multi_line).fixed("a,b ,c {}"),
    bad("a,\nb ,c {\n}").at(1, 2, rejected_after_multi_line).fixed("a,b ,c {\n}"),
    bad("a,\n\n   \t b ,c {\n}").at(1, 2, rejected_after_multi_line).fixed("a,b ,c {\n}"),
    bad("a,\n/*comment*/\nb ,\nc {\n}").at(1, 2, rejected_after_multi_line).at(3, 3, rejected_after_multi_line).fixed("a,\n/*comment*/b ,c {\n}"),
    bad("a,\nb,\nc,\nd,\ne,\nf,\ng,\nh,\ni,\nj,\nk,\nl,\nm,\nn,\no,\np,\nq,\nr,\ns,\nt,\nu,\nv,\nw,\nx,\ny,\nz {\n}").at(1, 2, rejected_after_multi_line).at(2, 2, rejected_after_multi_line).at(3, 2, rejected_after_multi_line).at(4, 2, rejected_after_multi_line).at(5, 2, rejected_after_multi_line).at(6, 2, rejected_after_multi_line).at(7, 2, rejected_after_multi_line).at(8, 2, rejected_after_multi_line).at(9, 2, rejected_after_multi_line).at(10, 2, rejected_after_multi_line).at(11, 2, rejected_after_multi_line).at(12, 2, rejected_after_multi_line).at(13, 2, rejected_after_multi_line).at(14, 2, rejected_after_multi_line).at(15, 2, rejected_after_multi_line).at(16, 2, rejected_after_multi_line).at(17, 2, rejected_after_multi_line).at(18, 2, rejected_after_multi_line).at(19, 2, rejected_after_multi_line).at(20, 2, rejected_after_multi_line).at(21, 2, rejected_after_multi_line).at(22, 2, rejected_after_multi_line).at(23, 2, rejected_after_multi_line).at(24, 2, rejected_after_multi_line).at(25, 2, rejected_after_multi_line).fixed("a,b,c,d,e,f,g,h,i,j,k,l,m,n,o,p,q,r,s,t,u,v,w,x,y,z {\n}"),
  ]);
}

#[test]
fn selector_list_comma_newline_after_always_scss() {
  check(
    "@stylistic/selector-list-comma-newline-after",
    json!(["always"]),
    Scss,
    &[
      ok("a, // comment\nb {}"),
      ok("a,   // comment\nb {}"),
      ok("a,\t// comment\nb {}"),
      ok("a,\t\t// comment\nb {}"),
      ok("a, \t \t // comment\nb {}"),
    ],
  );
}

#[test]
fn selector_list_comma_newline_after_always_less() {
  check(
    "@stylistic/selector-list-comma-newline-after",
    json!(["always"]),
    Less,
    &[
      ok("a, // comment\nb {}"),
      ok("a,   // comment\nb {}"),
      ok("a,\t// comment\nb {}"),
      ok("a,\t\t// comment\nb {}"),
      ok("a, \t \t // comment\nb {}"),
      ok(".col( @a, @b ) {}"),
      ok(".col3( @a, @b ) {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_newline_after_always_multi_line_gale_cases() {
  check(
    "@stylistic/selector-list-comma-newline-after",
    json!("always-multi-line"),
    Css,
    &[
      ok("a,\nb,\nc {}"),
      ok("a, /* comment */\nb,\nc {}"),
      ok("a, b, c {}"),
      bad("a { b,\n  c, d {} }")
        .at(2, 4, "Expected newline after \",\" in a multi-line list")
        .fixed("a { b,\n  c,\n d {} }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_newline_after_always_multi_line_scss_gale_cases() {
  check(
    "@stylistic/selector-list-comma-newline-after",
    json!("always-multi-line"),
    Scss,
    &[ok(
      "&[aria-disabled=\"true\"]:enabled, // This catches a situation\n&[aria-disabled=\"true\"]:active:enabled {}",
    )],
  );
}

#[test]
fn selector_list_comma_newline_before_always() {
  let expected_before = "Expected newline before \",\"";
  check(
    "@stylistic/selector-list-comma-newline-before",
    json!(["always"]),
    Css,
    &[
      ok("a\n,b {}"),
      ok("a\n\n,b {}"),
      ok("a\n,b\n,c {}"),
      ok("a\r\n,b\r\n,c {}"),
      ok("a\r\n\r\n,b\r\n,c {}"),
      ok("a\n, b {}"),
      ok("a\n,\nb {}"),
      ok("a\r\n,\r\nb {}"),
      ok("a\n,b[data-foo=\"tr,tr\"] {}"),
      ok("a\n    ,b {}"),
      ok("a\r\n    ,b {}"),
      ok("a\n\t\t,b {}"),
      ok("\ta\n\t, b {}"),
      ok("a:matches(:hover, :focus) {}"),
      ok(":not(:hover, :focus) {}"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a/*comment,comment*/\n,/*comment*/b {}"),
      bad("a,b {}").at(1, 2, expected_before).fixed("a\n,b {}"),
      bad("a ,b {}").at(1, 3, expected_before).fixed("a\n ,b {}"),
      bad("a  ,b {}")
        .at(1, 4, expected_before)
        .fixed("a\n  ,b {}"),
      bad("a\t,b {}")
        .at(1, 3, expected_before)
        .fixed("a\n\t,b {}"),
      bad("a\n,b,c {}")
        .at(2, 3, expected_before)
        .fixed("a\n,b\n,c {}"),
      bad("a\r\n,b,c {}")
        .at(2, 3, expected_before)
        .fixed("a\r\n,b\r\n,c {}"),
      bad("a/*comment*/,/*comment*/b {}")
        .at(1, 13, expected_before)
        .fixed("a/*comment*/\n,/*comment*/b {}"),
      bad("a,b,c,d,e,f,g {}")
        .at(1, 2, expected_before)
        .at(1, 4, expected_before)
        .at(1, 6, expected_before)
        .at(1, 8, expected_before)
        .at(1, 10, expected_before)
        .at(1, 12, expected_before)
        .fixed("a\n,b\n,c\n,d\n,e\n,f\n,g {}"),
    ],
  );
}

#[test]
fn selector_list_comma_newline_before_always_multi_line() {
  let expected_before_multi_line = "Expected newline before \",\" in a multi-line list";
  check(
    "@stylistic/selector-list-comma-newline-before",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a\n,b {}"),
      ok("a\r\n,b {}"),
      ok("a, b {}"),
      ok("a, b {\n}"),
      ok("\ta\n\t, b {\n}"),
      bad("a\n,b, c {}")
        .at(2, 3, expected_before_multi_line)
        .fixed("a\n,b\n, c {}"),
      bad("a\r\n,b, c {}")
        .at(2, 3, expected_before_multi_line)
        .fixed("a\r\n,b\r\n, c {}"),
      bad("a\n,b, c {\n}")
        .at(2, 3, expected_before_multi_line)
        .fixed("a\n,b\n, c {\n}"),
    ],
  );
}

#[test]
fn selector_list_comma_newline_before_never_multi_line() {
  let rejected_before_multi_line = "Unexpected whitespace before \",\" in a multi-line list";
  check(
    "@stylistic/selector-list-comma-newline-before",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a,\nb {}"),
      ok("a ,b {}"),
      ok("a ,b {\n}"),
      ok("a ,b {\r\n}"),
      ok("a:matches(:hover, :focus) {}"),
      ok(":not(:hover, :focus) {}"),
      ok("a/*comment\n,comment*/,/*comment*/b {\n}"),
      bad("a,\nb , c {}")
        .at(2, 3, rejected_before_multi_line)
        .fixed("a,\nb, c {}"),
      bad("a,\nb , c {\n}")
        .at(2, 3, rejected_before_multi_line)
        .fixed("a,\nb, c {\n}"),
      bad("a,\r\nb , c {\r\n}")
        .at(2, 3, rejected_before_multi_line)
        .fixed("a,\r\nb, c {\r\n}"),
      bad("a/*comment*/\n,/*comment*/b {\n}")
        .at(2, 1, rejected_before_multi_line)
        .fixed("a/*comment*/,/*comment*/b {\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_newline_before_always_gale_cases() {
  check(
    "@stylistic/selector-list-comma-newline-before",
    json!("always"),
    Css,
    &[
      ok("a\n, b { color: red; }"),
      bad("a, b { color: red; }")
        .at(1, 2, "Expected newline before \",\"")
        .fixed("a\n, b { color: red; }"),
    ],
  );
}

// Gale's own `"never"`, which the plugin does not take: no line break
// next to the character.  Gale adds no fix for it.
#[test]
fn selector_list_comma_newline_before_never_gale_only() {
  check(
    "@stylistic/selector-list-comma-newline-before",
    json!("never"),
    Css,
    &[
      ok("a, b { color: red; }"),
      bad("a\n, b { color: red; }").at(2, 1, "Unexpected whitespace before \",\""),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_list_comma_newline_before_always_multi_line_gale_cases() {
  check(
    "@stylistic/selector-list-comma-newline-before",
    json!("always-multi-line"),
    Css,
    &[ok("a, b { color: red; }")],
  );
}

#[test]
fn value_list_comma_space_after_always() {
  let expected_after = "Expected single space after \",\"";
  check(
    "@stylistic/value-list-comma-space-after",
    json!(["always"]),
    Css,
    &[
      ok("a { background-size: 0 , 0; }"),
      ok("a { background-size: 0, 0; }"),
      ok("a::before { content: \"foo,bar,baz\"; }"),
      ok("a { transform: translate(1,1); }"),
      bad("a { background-size: 0,0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size:\n\t0,  0; }")
        .at(2, 3, expected_after)
        .fixed("a { background-size:\n\t0, 0; }"),
      bad("a { background-size: 0,\n0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0,\r\n0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0,\t0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0,/*1*/0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0, /*1*/0; }"),
      bad("a { background-size: 0,0,0,0; }")
        .at(1, 23, expected_after)
        .at(1, 25, expected_after)
        .at(1, 27, expected_after)
        .fixed("a { background-size: 0, 0, 0, 0; }"),
      bad(":root { --variable: 0,0; }")
        .at(1, 22, expected_after)
        .fixed(":root { --variable: 0, 0; }"),
      // Upstream skips this case as an edge case; Stylelint 17 with the plugin passes
      // it, as gale does.
      bad("a { pr,op: 0,0; }")
        .at(1, 7, expected_after)
        .at(1, 13, expected_after)
        .fixed("a { pr,op: 0, 0; }"),
      bad("a{b: 0,0,0,0,0,0,0,0; }")
        .at(1, 7, expected_after)
        .at(1, 9, expected_after)
        .at(1, 11, expected_after)
        .at(1, 13, expected_after)
        .at(1, 15, expected_after)
        .at(1, 17, expected_after)
        .at(1, 19, expected_after)
        .fixed("a{b: 0, 0, 0, 0, 0, 0, 0, 0; }"),
    ],
  );
}

#[test]
fn value_list_comma_space_after_never() {
  let rejected_after = "Unexpected whitespace after \",\"";
  check(
    "@stylistic/value-list-comma-space-after",
    json!(["never"]),
    Css,
    &[
      ok("a { background-size: 0 ,0; }"),
      ok("a { background-size: 0,0; }"),
      ok("a::before { content: \"foo, bar, baz\"; }"),
      ok("a { transform: translate(1, 1); }"),
      bad("a { background-size: 0, 0; }")
        .at(1, 23, rejected_after)
        .fixed("a { background-size: 0,0; }"),
      bad("a { background-size: 0,  0; }")
        .at(1, 23, rejected_after)
        .fixed("a { background-size: 0,0; }"),
      bad("a { background-size: 0,\n0; }")
        .at(1, 23, rejected_after)
        .fixed("a { background-size: 0,0; }"),
      bad("a { background-size: 0,\r\n0; }")
        .at(1, 23, rejected_after)
        .fixed("a { background-size: 0,0; }"),
      bad("a { background-size: 0,\t0; }")
        .at(1, 23, rejected_after)
        .fixed("a { background-size: 0,0; }"),
      bad("a { background-size: 0, /*1*/ 0; }")
        .at(1, 23, rejected_after)
        .fixed("a { background-size: 0,/*1*/ 0; }"),
      bad("a { background-size: 0, 0, 0, 0 ; }")
        .at(1, 23, rejected_after)
        .at(1, 26, rejected_after)
        .at(1, 29, rejected_after)
        .fixed("a { background-size: 0,0,0,0 ; }"),
      bad(":root { --variable: 0, 0; }")
        .at(1, 22, rejected_after)
        .fixed(":root { --variable: 0,0; }"),
    ],
  );
}

#[test]
fn value_list_comma_space_after_always_single_line() {
  let expected_after_single_line = "Expected single space after \",\" in a single-line list";
  check(
    "@stylistic/value-list-comma-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { background-size: 0 , 0; }"),
      ok("a { background-size: 0, 0; }"),
      ok("a { background-size: 0, 0;\n}"),
      ok("a { background-size: 0, 0;\r\n}"),
      ok("a { background-size: 0\n,0}"),
      ok("a { background-size: 0\r\n,0}"),
      ok("a::before { content: \"foo,bar,baz\"; }"),
      ok("a { transform: translate(1,1); }"),
      bad("a { background-size: 0,0; }")
        .at(1, 23, expected_after_single_line)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0,0;\n}")
        .at(1, 23, expected_after_single_line)
        .fixed("a { background-size: 0, 0;\n}"),
      bad("a { background-size: 0,0;\r\n}")
        .at(1, 23, expected_after_single_line)
        .fixed("a { background-size: 0, 0;\r\n}"),
      bad("a { background-size: 0,  0; }")
        .at(1, 23, expected_after_single_line)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0,\t0; }")
        .at(1, 23, expected_after_single_line)
        .fixed("a { background-size: 0, 0; }"),
    ],
  );
}

#[test]
fn value_list_comma_space_after_never_single_line() {
  let rejected_after_single_line = "Unexpected whitespace after \",\" in a single-line list";
  check(
    "@stylistic/value-list-comma-space-after",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a { background-size: 0 ,0; }"),
      ok("a { background-size: 0,0; }"),
      ok("a { background-size: 0,0;\n}"),
      ok("a { background-size: 0,0;\r\n}"),
      ok("a { background-size: 0\n,  0}"),
      ok("a { background-size: 0\r\n,  0}"),
      ok("a::before { content: \"foo, bar, baz\"; }"),
      ok("a { transform: translate(1, 1); }"),
      bad("a { background-size: 0, 0; }")
        .at(1, 23, rejected_after_single_line)
        .fixed("a { background-size: 0,0; }"),
      bad("a { background-size: 0, 0;\n}")
        .at(1, 23, rejected_after_single_line)
        .fixed("a { background-size: 0,0;\n}"),
      bad("a { background-size: 0, 0;\r\n}")
        .at(1, 23, rejected_after_single_line)
        .fixed("a { background-size: 0,0;\r\n}"),
      bad("a { background-size: 0,  0; }")
        .at(1, 23, rejected_after_single_line)
        .fixed("a { background-size: 0,0; }"),
      bad("a { background-size: 0,\t0; }")
        .at(1, 23, rejected_after_single_line)
        .fixed("a { background-size: 0,0; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_space_after_always_gale_cases() {
  check(
    "@stylistic/value-list-comma-space-after",
    json!("always"),
    Css,
    &[
      ok("a { font-family: Arial, sans-serif; }"),
      bad("a { font-family: Arial,sans-serif; }")
        .at(1, 23, "Expected single space after \",\"")
        .fixed("a { font-family: Arial, sans-serif; }"),
      ok("a { background: rgb(0,0,0); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_space_after_never_gale_cases() {
  check(
    "@stylistic/value-list-comma-space-after",
    json!("never"),
    Css,
    &[
      ok("a { font-family: Arial,sans-serif; }"),
      bad("a { font-family: Arial, sans-serif; }")
        .at(1, 23, "Unexpected whitespace after \",\"")
        .fixed("a { font-family: Arial,sans-serif; }"),
    ],
  );
}

#[test]
fn value_list_comma_space_before_always() {
  let expected_before = "Expected single space before \",\"";
  check(
    "@stylistic/value-list-comma-space-before",
    json!(["always"]),
    Css,
    &[
      ok("a { background-size: 0 , 0; }"),
      ok("a { background-size: 0 ,0; }"),
      ok("a::before { content: \"foo,bar,baz\"; }"),
      ok("a { transform: translate(1,1); }"),
      bad("a { background-size: 0, 0; }")
        .at(1, 23, expected_before)
        .fixed("a { background-size: 0 , 0; }"),
      bad("a { background-size: 0  , 0; }")
        .at(1, 25, expected_before)
        .fixed("a { background-size: 0 , 0; }"),
      bad("a { background-size: 0\n, 0; }")
        .at(2, 1, expected_before)
        .fixed("a { background-size: 0 , 0; }"),
      bad("a { background-size: 0\r\n, 0; }")
        .at(2, 1, expected_before)
        .fixed("a { background-size: 0 , 0; }"),
      bad("a { background-size: 0\t, 0; }")
        .at(1, 24, expected_before)
        .fixed("a { background-size: 0 , 0; }"),
      bad("a { background-size: 0/*comment*/, 0; }")
        .at(1, 34, expected_before)
        .fixed("a { background-size: 0/*comment*/ , 0; }"),
      bad("a{b: 0,0,0,0,0,0,0,0; }")
        .at(1, 7, expected_before)
        .at(1, 9, expected_before)
        .at(1, 11, expected_before)
        .at(1, 13, expected_before)
        .at(1, 15, expected_before)
        .at(1, 17, expected_before)
        .at(1, 19, expected_before)
        .fixed("a{b: 0 ,0 ,0 ,0 ,0 ,0 ,0 ,0; }"),
    ],
  );
}

#[test]
fn value_list_comma_space_before_never() {
  let rejected_before = "Unexpected whitespace before \",\"";
  check(
    "@stylistic/value-list-comma-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a { background-size: 0, 0; }"),
      ok("a { background-size: 0,0; }"),
      ok("a::before { content: \"foo ,bar ,baz\"; }"),
      ok("a { transform: translate(1 ,1); }"),
      bad("a { background-size: 0 , 0; }")
        .at(1, 24, rejected_before)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0  , 0; }")
        .at(1, 25, rejected_before)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0\n, 0; }")
        .at(2, 1, rejected_before)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0\r\n, 0; }")
        .at(2, 1, rejected_before)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0\t, 0; }")
        .at(1, 24, rejected_before)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0 /*comment*/ , 0; }")
        .at(1, 36, rejected_before)
        .fixed("a { background-size: 0 /*comment*/, 0; }"),
    ],
  );
}

#[test]
fn value_list_comma_space_before_always_single_line() {
  let expected_before_single_line = "Unexpected whitespace before \",\" in a single-line list";
  check(
    "@stylistic/value-list-comma-space-before",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { background-size: 0 , 0; }"),
      ok("a { background-size: 0 ,0; }"),
      ok("a { background-size: 0 ,0;\n}"),
      ok("a { background-size: 0 ,0;\r\n}"),
      ok("a { background-size: 0,\n0; }"),
      ok("a { background-size: 0,\r\n0; }"),
      ok("a::before { content: \"foo,bar,baz\"; }"),
      ok("a { transform: translate(1,1); }"),
      bad("a { background-size: 0, 0; }")
        .at(1, 23, expected_before_single_line)
        .fixed("a { background-size: 0 , 0; }"),
      bad("a { background-size: 0, 0;\n}")
        .at(1, 23, expected_before_single_line)
        .fixed("a { background-size: 0 , 0;\n}"),
      bad("a { background-size: 0, 0;\r\n}")
        .at(1, 23, expected_before_single_line)
        .fixed("a { background-size: 0 , 0;\r\n}"),
      bad("a { background-size: 0  , 0; }")
        .at(1, 25, expected_before_single_line)
        .fixed("a { background-size: 0 , 0; }"),
      bad("a { background-size: 0\t, 0; }")
        .at(1, 24, expected_before_single_line)
        .fixed("a { background-size: 0 , 0; }"),
    ],
  );
}

#[test]
fn value_list_comma_space_before_never_single_line() {
  let expected_before_single_line = "Unexpected whitespace before \",\" in a single-line list";
  check(
    "@stylistic/value-list-comma-space-before",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a { background-size: 0, 0; }"),
      ok("a { background-size: 0,0; }"),
      ok("a { background-size: 0,0;\n}"),
      ok("a { background-size: 0,0;\r\n}"),
      ok("a { background-size: 0 ,\n0; }"),
      ok("a { background-size: 0 ,\r\n0; }"),
      ok("a::before { content: \"foo ,bar ,baz\"; }"),
      ok("a { transform: translate(1 ,1); }"),
      bad("a { background-size: 0 , 0; }")
        .at(1, 24, expected_before_single_line)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0 , 0;\n}")
        .at(1, 24, expected_before_single_line)
        .fixed("a { background-size: 0, 0;\n}"),
      bad("a { background-size: 0 , 0;\r\n}")
        .at(1, 24, expected_before_single_line)
        .fixed("a { background-size: 0, 0;\r\n}"),
      bad("a { background-size: 0  , 0; }")
        .at(1, 25, expected_before_single_line)
        .fixed("a { background-size: 0, 0; }"),
      bad("a { background-size: 0\t, 0; }")
        .at(1, 24, expected_before_single_line)
        .fixed("a { background-size: 0, 0; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_space_before_never_gale_cases() {
  check(
    "@stylistic/value-list-comma-space-before",
    json!("never"),
    Css,
    &[
      ok("a { background: url(foo.png), url(bar.png); }"),
      bad("a { background: url(foo.png) , url(bar.png); }")
        .at(1, 30, "Unexpected whitespace before \",\"")
        .fixed("a { background: url(foo.png), url(bar.png); }"),
      ok("a { content: \"foo , bar\"; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_space_before_always_gale_cases() {
  check(
    "@stylistic/value-list-comma-space-before",
    json!("always"),
    Css,
    &[
      ok("a { background: url(foo.png) , url(bar.png); }"),
      bad("a { background: url(foo.png), url(bar.png); }")
        .at(1, 29, "Expected single space before \",\"")
        .fixed("a { background: url(foo.png) , url(bar.png); }"),
    ],
  );
}

#[test]
fn value_list_comma_newline_after_always() {
  let expected_after = "Expected newline after \",\"";
  check(
    "@stylistic/value-list-comma-newline-after",
    json!(["always"]),
    Css,
    &[
      ok("a { background-size: 0,\n0; }"),
      ok("a { background-size: 0,\n\n0; }"),
      ok("a { background-size: 0 ,\n  0; }"),
      ok("a { background-size: 0 ,\r\n  0; }"),
      ok("a { background-size: 0 ,\r\n\r\n  0; }"),
      ok("a::before { content: \"foo,bar,baz\"; }"),
      ok("a { transform: translate(1,1); }"),
      ok("$grid-breakpoints: (\n(xs),\n(sm, 768px)\n) !default;"),
      ok("a { background-size: 0, //\n0; }"),
      ok("a { background-size: 0, /**/\n0; }"),
      bad("a { background-size: 0, 0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0,\n 0; }"),
      bad("a { background-size: 0,  0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0,\n  0; }"),
      bad("a { background-size: 0,\t0; }")
        .at(1, 23, expected_after)
        .fixed("a { background-size: 0,\n\t0; }"),
      bad("a { background-size: 0, /**/0; }")
        .at(1, 28, expected_after)
        .fixed("a { background-size: 0, /**/\n0; }"),
    ],
  );
}

#[test]
fn value_list_comma_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \",\" in a multi-line list";
  check(
    "@stylistic/value-list-comma-newline-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a { background-size: 0,\n0,\n0; }"),
      ok("a { background-size: 0, //\n0, /**/\n0; }"),
      ok("a { background-size: 0 ,\n  0,\n0; }"),
      ok("a { background-size: 0 ,\r\n  0,\r\n0; }"),
      ok("a { background-size: 0, 0; }"),
      ok("a { background-size: 0, 0;\n}"),
      ok("a { background-size: 0, 0;\r\n}"),
      ok("a { background-size: 0, /**/ 0; }"),
      bad("a { background-size: 0,\n0, 0; }")
        .at(2, 2, expected_after_multi_line)
        .fixed("a { background-size: 0,\n0,\n 0; }"),
      bad("a { background-size: 0, //\n0, /**/0; }")
        .at(2, 7, expected_after_multi_line)
        .fixed("a { background-size: 0, //\n0, /**/\n0; }"),
      bad("a { background-size: 0,\n0,  0; }")
        .at(2, 2, expected_after_multi_line)
        .fixed("a { background-size: 0,\n0,\n  0; }"),
      bad("a { background-size: 0,\n0,\t0; }")
        .at(2, 2, expected_after_multi_line)
        .fixed("a { background-size: 0,\n0,\n\t0; }"),
      bad("a { background-size: 0,\r\n0,\t0; }")
        .at(2, 2, expected_after_multi_line)
        .fixed("a { background-size: 0,\r\n0,\r\n\t0; }"),
    ],
  );
}

#[test]
fn value_list_comma_newline_after_never_multi_line() {
  let rejected_after_multi_line = "Unexpected whitespace after \",\" in a multi-line list";
  check(
    "@stylistic/value-list-comma-newline-after",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a { background-size: 0\n,0\n,0; }"),
      ok("a { background-size: 0 //\n,0 /**/\n,0; }"),
      ok("a { background-size: 0\r\n,0\r\n,0; }"),
      ok("a { background-size: 0, 0; }"),
      ok("a { background-size: 0, 0;\n}"),
      ok("a { background-size: 0, 0;\r\n}"),
      bad("a { background-size: 0\n,0\n, 0; }")
        .at(3, 1, rejected_after_multi_line)
        .fixed("a { background-size: 0\n,0\n,0; }"),
      bad("a { background-size: 0\n,0\n,  0; }")
        .at(3, 1, rejected_after_multi_line)
        .fixed("a { background-size: 0\n,0\n,0; }"),
      bad("a { background-size: 0\r\n,0\r\n,  0; }")
        .at(3, 1, rejected_after_multi_line)
        .fixed("a { background-size: 0\r\n,0\r\n,0; }"),
      bad("a { background-size: 0\n,0\n,\t0; }")
        .at(3, 1, rejected_after_multi_line)
        .fixed("a { background-size: 0\n,0\n,0; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_newline_after_always_gale_cases() {
  check(
    "@stylistic/value-list-comma-newline-after",
    json!("always"),
    Css,
    &[
      ok("a { background: url(foo.png),\n  url(bar.png); }"),
      bad("a { background: url(foo.png), url(bar.png); }")
        .at(1, 29, "Expected newline after \",\"")
        .fixed("a { background: url(foo.png),\n url(bar.png); }"),
      ok("a:hover, a:focus { color: red; }"),
      ok("a { background: url(foo.png), /* comment */\n  url(bar.png); }"),
    ],
  );
}

// Gale's own `"never"`, which the plugin does not take: no line break
// next to the character.  Gale adds no fix for it.
#[test]
fn value_list_comma_newline_after_never_gale_only() {
  check(
    "@stylistic/value-list-comma-newline-after",
    json!("never"),
    Css,
    &[ok("a { background: url(foo.png), url(bar.png); }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_newline_after_always_multi_line_gale_cases() {
  check(
    "@stylistic/value-list-comma-newline-after",
    json!("always-multi-line"),
    Css,
    &[ok("a { background: url(foo.png), url(bar.png); }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_newline_after_always_scss_gale_cases() {
  check(
    "@stylistic/value-list-comma-newline-after",
    json!("always"),
    Scss,
    &[
      ok("a { background: url(foo.png), // comment\n  url(bar.png); }"),
      bad("a { prop: #{$a, $b}; }")
        .at(1, 15, "Expected newline after \",\"")
        .fixed("a { prop: #{$a,\n $b}; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_newline_after_never_multi_line_gale_cases() {
  check(
    "@stylistic/value-list-comma-newline-after",
    json!("never-multi-line"),
    Css,
    &[
      bad("a { transition: a 1s, b 1s,\n  c 1s; }")
        .at(
          1,
          21,
          "Unexpected whitespace after \",\" in a multi-line list",
        )
        .at(
          1,
          27,
          "Unexpected whitespace after \",\" in a multi-line list",
        )
        .fixed("a { transition: a 1s,b 1s,c 1s; }"),
      ok("a { transition: a 1s, b 1s; }"),
    ],
  );
}

#[test]
fn value_list_comma_newline_before_always() {
  let expected_before = "Expected newline before \",\"";
  check(
    "@stylistic/value-list-comma-newline-before",
    json!(["always"]),
    Css,
    &[
      ok("a { background-size: 0\n,0\n,0; }"),
      ok("a { background-size: 0\n\n,0\n\n,0; }"),
      ok("a { background-size: 0\n,  0\n,\t0; }"),
      ok("a { background-size: 0\r\n,  0\r\n,\t0; }"),
      ok("a { background-size: 0\r\n\r\n,  0\r\n,\t0; }"),
      ok("a { background-size: 0\n    ,0\n,0; }"),
      ok("a { background-size: 0\n\t\t,0\n,0; }"),
      ok("a { background-size: 0\r\n\t\t,0\r\n,0; }"),
      ok("a::before { content: \"foo,bar,baz\"; }"),
      ok("a { transform: translate(1,1); }"),
      bad("a { background-size: 0, 0; }")
        .at(1, 23, expected_before)
        .unfixed(),
      bad("a { background-size: 0 , 0; }")
        .at(1, 24, expected_before)
        .unfixed(),
      bad("a { background-size: 0  , 0; }")
        .at(1, 25, expected_before)
        .unfixed(),
      bad("a { background-size: 0\t, 0; }")
        .at(1, 24, expected_before)
        .unfixed(),
    ],
  );
}

#[test]
fn value_list_comma_newline_before_always_multi_line() {
  let expected_before_multi_line = "Expected newline before \",\" in a multi-line list";
  check(
    "@stylistic/value-list-comma-newline-before",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a { background-size: 0\n,0\n,0; }"),
      ok("a { background-size: 0\n,  0\n,\t0; }"),
      ok("a { background-size: 0\r\n,  0\r\n,\t0; }"),
      ok("a { background-size: 0, 0; }"),
      ok("a { background-size: 0, 0;\n}"),
      ok("a { background-size: 0, 0;\r\n}"),
      bad("a { background-size: 0\n, 0, 0; }")
        .at(2, 4, expected_before_multi_line)
        .unfixed(),
      bad("a { background-size: 0\n, 0 , 0; }")
        .at(2, 5, expected_before_multi_line)
        .unfixed(),
      bad("a { background-size: 0\r\n, 0 , 0; }")
        .at(2, 5, expected_before_multi_line)
        .unfixed(),
      bad("a { background-size: 0\n, 0\t, 0; }")
        .at(2, 5, expected_before_multi_line)
        .unfixed(),
    ],
  );
}

#[test]
fn value_list_comma_newline_before_never_multi_line() {
  let rejected_before_multi_line = "Unexpected whitespace before \",\" in a multi-line list";
  check(
    "@stylistic/value-list-comma-newline-before",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a { background-size: 0,\n0,\n0; }"),
      ok("a { background-size: 0 ,0; }"),
      ok("a { background-size: 0 ,0;\n}"),
      bad("a { background-size: 0,\n0\n, 0; }")
        .at(3, 1, rejected_before_multi_line)
        .unfixed(),
      bad("a { background-size: 0,\r\n0\r\n, 0; }")
        .at(3, 1, rejected_before_multi_line)
        .unfixed(),
      bad("a { background-size: 0\n,\t0,\n0; }")
        .at(2, 1, rejected_before_multi_line)
        .unfixed(),
      bad("a { background-size: 0\r\n,\t0,\r\n0; }")
        .at(2, 1, rejected_before_multi_line)
        .unfixed(),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_newline_before_always_gale_cases() {
  check(
    "@stylistic/value-list-comma-newline-before",
    json!("always"),
    Css,
    &[
      ok("a { font-family: Arial\n, sans-serif; }"),
      bad("a { font-family: Arial, sans-serif; }")
        .at(1, 23, "Expected newline before \",\"")
        .unfixed(),
    ],
  );
}

// Gale's own `"never"`, which the plugin does not take: no line break
// next to the character.  Gale adds no fix for it.
#[test]
fn value_list_comma_newline_before_never_gale_only() {
  check(
    "@stylistic/value-list-comma-newline-before",
    json!("never"),
    Css,
    &[
      ok("a { font-family: Arial, sans-serif; }"),
      bad("a { font-family: Arial\n, sans-serif; }").at(2, 1, "Unexpected whitespace before \",\""),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_newline_before_always_multi_line_gale_cases() {
  check(
    "@stylistic/value-list-comma-newline-before",
    json!("always-multi-line"),
    Css,
    &[ok("a { font-family: Arial, sans-serif; }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn value_list_comma_newline_before_never_multi_line_gale_cases() {
  check(
    "@stylistic/value-list-comma-newline-before",
    json!("never-multi-line"),
    Css,
    &[
      bad("a { transition: a 1s ,b 1s\n  , c 1s; }")
        .at(
          1,
          22,
          "Unexpected whitespace before \",\" in a multi-line list",
        )
        .at(
          2,
          3,
          "Unexpected whitespace before \",\" in a multi-line list",
        )
        .unfixed(),
      ok("a { transition: a 1s ,b 1s; }"),
    ],
  );
}

#[test]
fn function_comma_space_after_always() {
  let expected_after = "Expected single space after \",\"";
  check(
    "@stylistic/function-comma-space-after",
    json!(["always"]),
    Css,
    &[
      ok("a::before { content: \"func(foo,bar,baz)\"; }"),
      ok("a::before { background: url('func(foo,bar,baz)'); }"),
      ok("a { background-size: 0,0,0; }"),
      ok("a { transform: translate(1 , 1); }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { transform: color(rgb(0 , 0, 0) lightness(50%)); }"),
      ok("a { background: url(data:image/svg+xml;charset=utf8,%3Csvg%20xmlns); }"),
      ok("a { transform: translate(1, /* comment */1); }"),
      bad("a { transform: translate(1,1); }")
        .at(1, 27, expected_after)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1,  1); }")
        .at(1, 27, expected_after)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1,\n1); }")
        .at(1, 27, expected_after)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1,\r\n1); }")
        .at(1, 27, expected_after)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1,\t1); }")
        .at(1, 27, expected_after)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: color(rgb(0 , 0 ,0) lightness(50%)); }")
        .at(1, 32, expected_after)
        .fixed("a { transform: color(rgb(0 , 0 , 0) lightness(50%)); }"),
      bad("a { transform: color(lightness(50%) rgb(0 , 0 ,0)); }")
        .at(1, 47, expected_after)
        .fixed("a { transform: color(lightness(50%) rgb(0 , 0 , 0)); }"),
      bad("a { transform: translate(1,/* comment */1); }")
        .at(1, 27, expected_after)
        .fixed("a { transform: translate(1, /* comment */1); }"),
      bad("a { color: rgba(0,0,0,0); }")
        .at(1, 18, expected_after)
        .at(1, 20, expected_after)
        .at(1, 22, expected_after)
        .fixed("a { color: rgba(0, 0, 0, 0); }"),
      bad("a { background: linear-gradient(45deg,rgba(0,0,0,1),red); }")
        .at(1, 38, expected_after)
        .at(1, 52, expected_after)
        .at(1, 45, expected_after)
        .at(1, 47, expected_after)
        .at(1, 49, expected_after)
        .fixed("a { background: linear-gradient(45deg, rgba(0, 0, 0, 1), red); }"),
    ],
  );
}

#[test]
fn function_comma_space_after_always_scss() {
  check(
    "@stylistic/function-comma-space-after",
    json!(["always"]),
    Scss,
    &[
      ok("$map: (key: value,key2: value2)"),
      ok("$list: (value,value2)"),
    ],
  );
}

#[test]
fn function_comma_space_after_never() {
  let rejected_after = "Unexpected whitespace after \",\"";
  check(
    "@stylistic/function-comma-space-after",
    json!(["never"]),
    Css,
    &[
      ok("a::before { content: \"func(foo, bar, baz)\"; }"),
      ok("a::before { background: url('func(foo, bar, baz)'); }"),
      ok("a { background-size: 0, 0, 0; }"),
      ok("a { transform: translate(1 ,1); }"),
      ok("a { transform: translate(1,1); }"),
      ok("a { transform: color(rgb(0 ,0,0) lightness(50%)); }"),
      ok("a { transform: translate(1,/* comment */1); }"),
      bad("a { transform: translate(1, 1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,1); }"),
      bad("a { transform: translate(1,  1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,1); }"),
      bad("a { transform: translate(1,\n1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,1); }"),
      bad("a { transform: translate(1,\r\n1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,1); }"),
      bad("a { transform: translate(1,\t1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,1); }"),
      bad("a { transform: color(rgb(0 , 0 ,0) lightness(50%)); }")
        .at(1, 28, rejected_after)
        .fixed("a { transform: color(rgb(0 ,0 ,0) lightness(50%)); }"),
      bad("a { transform: lightness(50%) color(rgb(0 , 0 ,0) ); }")
        .at(1, 43, rejected_after)
        .fixed("a { transform: lightness(50%) color(rgb(0 ,0 ,0) ); }"),
      bad("a { transform: translate(1, /* comment */1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,/* comment */1); }"),
      bad("a { transform: translate(1, /* comment */ 1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,/* comment */1); }"),
      bad("a { transform: translate(1, /* 1 */\n/* 2 */ /* 3 */ 1); }")
        .at(1, 27, rejected_after)
        .fixed("a { transform: translate(1,/* 1 *//* 2 *//* 3 */1); }"),
      bad("a { background: linear-gradient(45deg , rgba(0 , 0 , 0 , 1) , red); }")
        .at(1, 39, rejected_after)
        .at(1, 61, rejected_after)
        .at(1, 48, rejected_after)
        .at(1, 52, rejected_after)
        .at(1, 56, rejected_after)
        .fixed("a { background: linear-gradient(45deg ,rgba(0 ,0 ,0 ,1) ,red); }"),
    ],
  );
}

#[test]
fn function_comma_space_after_never_scss() {
  check(
    "@stylistic/function-comma-space-after",
    json!(["never"]),
    Scss,
    &[ok("$map: (key: value, key2: value2)")],
  );
}

#[test]
fn function_comma_space_after_always_single_line() {
  let expected_after_single_line = "Expected single space after \",\" in a single-line function";
  check(
    "@stylistic/function-comma-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a::before { content: \"func(foo,bar,baz)\"; }"),
      ok("a::before { background: url('func(foo,bar,baz)'); }"),
      ok("a { background-size: 0,0,0; }"),
      ok("a { transform: translate(1 , 1); }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { transform: color(rgb(0 , 0, 0) lightness(50%)); }"),
      ok("a { transform: translate(1,\n1); }"),
      ok("a { transform: translate(1\n,1); }"),
      ok("a { transform: translate(1,\r\n1); }"),
      ok("a { color: rgba(0,0\n,0); }"),
      ok("a { color: rgba(0\n,0,0); }"),
      ok("a { background: linear-gradient(45deg\n,rgba(0, 0, 0, 1)\n,red); }"),
      bad("a { transform: color(rgb(0 , 0 ,0) lightness(50%)); }")
        .at(1, 32, expected_after_single_line)
        .fixed("a { transform: color(rgb(0 , 0 , 0) lightness(50%)); }"),
      bad("a { transform: color(lightness(50%) rgb(0 , 0 ,0)); }")
        .at(1, 47, expected_after_single_line)
        .fixed("a { transform: color(lightness(50%) rgb(0 , 0 , 0)); }"),
      bad("a { background: linear-gradient(45deg\n,rgba(0, 0,0, 1),red); }")
        .at(2, 11, expected_after_single_line)
        .fixed("a { background: linear-gradient(45deg\n,rgba(0, 0, 0, 1),red); }"),
    ],
  );
}

#[test]
fn function_comma_space_after_always_single_line_scss() {
  check(
    "@stylistic/function-comma-space-after",
    json!(["always-single-line"]),
    Scss,
    &[ok("$map: (key: value,key2: value2)")],
  );
}

#[test]
fn function_comma_space_after_never_single_line() {
  let rejected_after_single_line = "Unexpected whitespace after \",\" in a single-line function";
  check(
    "@stylistic/function-comma-space-after",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a::before { content: \"func(foo, bar, baz)\"; }"),
      ok("a::before { background: url('func(foo, bar, baz)'); }"),
      ok("a { background-size: 0, 0, 0; }"),
      ok("a { transform: translate(1 ,1); }"),
      ok("a { transform: translate(1,1); }"),
      ok("a { transform: color(rgb(0 ,0,0) lightness(50%)); }"),
      ok("a { transform: translate(1,\n1); }"),
      ok("a { transform: translate(1\n, 1); }"),
      ok("a { transform: translate(1\r\n, 1); }"),
      ok("a { color: rgba(0, 0\n, 0); }"),
      ok("a { color: rgba(0\n, 0, 0); }"),
      bad("a { transform: color(rgb(0 , 0 ,0) lightness(50%)); }")
        .at(1, 28, rejected_after_single_line)
        .fixed("a { transform: color(rgb(0 ,0 ,0) lightness(50%)); }"),
      bad("a { transform: lightness(50%) color(rgb(0 , 0 ,0) ); }")
        .at(1, 43, rejected_after_single_line)
        .fixed("a { transform: lightness(50%) color(rgb(0 ,0 ,0) ); }"),
      bad("a { transform: lightness(50%)\ncolor(rgb(0 , 0 ,0) ); }")
        .at(2, 13, rejected_after_single_line)
        .fixed("a { transform: lightness(50%)\ncolor(rgb(0 ,0 ,0) ); }"),
    ],
  );
}

#[test]
fn function_comma_space_after_never_single_line_scss() {
  check(
    "@stylistic/function-comma-space-after",
    json!(["never-single-line"]),
    Scss,
    &[ok("$map: (key: value, key2: value2)")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_space_after_always_gale_cases() {
  check(
    "@stylistic/function-comma-space-after",
    json!("always"),
    Css,
    &[
      ok("a { transform: translate(1px, 2px); }"),
      bad("a { transform: translate(1px,2px); }")
        .at(1, 29, "Expected single space after \",\"")
        .fixed("a { transform: translate(1px, 2px); }"),
      ok("a:not(.foo,.bar) { color: red; }"),
      ok("a { @include mixin($a,$b); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_space_after_never_gale_cases() {
  check(
    "@stylistic/function-comma-space-after",
    json!("never"),
    Css,
    &[
      ok("a { transform: translate(1px,2px); }"),
      bad("a { transform: translate(1px, 2px); }")
        .at(1, 29, "Unexpected whitespace after \",\"")
        .fixed("a { transform: translate(1px,2px); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_space_after_always_scss_gale_cases() {
  check(
    "@stylistic/function-comma-space-after",
    json!("always"),
    Scss,
    &[ok("a { background: url(#{$var, $other}); }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_space_after_never_single_line_gale_cases() {
  check(
    "@stylistic/function-comma-space-after",
    json!("never-single-line"),
    Css,
    &[
      bad("a { color: rgb(0, 0,0); }")
        .at(
          1,
          17,
          "Unexpected whitespace after \",\" in a single-line function",
        )
        .fixed("a { color: rgb(0,0,0); }"),
      ok("a { color: rgb(0,\n  0, 0); }"),
    ],
  );
}

#[test]
fn function_comma_space_before_always() {
  let expected_before = "Expected single space before \",\"";
  check("@stylistic/function-comma-space-before", json!(["always"]), Css, &[
    ok("a::before { content: \"func(foo,bar,baz)\"; }"),
    ok("a::before { background: url('func(foo,bar,baz)'); }"),
    ok("a { background-size: 0, 0, 0; }"),
    ok("a { transform: translate(1 , 1); }"),
    ok("a { transform: translate(1 ,1); }"),
    ok("a { transform: color(rgb(0 , 0 ,0) lightness(50%)); }"),
    ok("a { background: url(data:image/svg+xml;charset=utf8,%3Csvg%20xmlns); }"),
    bad("a { transform: translate(1, 1); }").at(1, 27, expected_before).fixed("a { transform: translate(1 , 1); }"),
    bad("a { transform: translate(1  , 1); }").at(1, 29, expected_before).fixed("a { transform: translate(1 , 1); }"),
    bad("a { transform: translate(1\n, 1); }").at(2, 1, expected_before).fixed("a { transform: translate(1 , 1); }"),
    bad("a { transform: translate(1\r\n, 1); }").at(2, 1, expected_before).fixed("a { transform: translate(1 , 1); }"),
    bad("a { transform: translate(1\t, 1); }").at(1, 28, expected_before).fixed("a { transform: translate(1 , 1); }"),
    bad("a { transform: color(rgb(0 , 0, 0) lightness(50%)); }").at(1, 31, expected_before).fixed("a { transform: color(rgb(0 , 0 , 0) lightness(50%)); }"),
    bad("a { transform: color(lightness(50%) rgb(0 , 0, 0)); }").at(1, 46, expected_before).fixed("a { transform: color(lightness(50%) rgb(0 , 0 , 0)); }"),
    bad("\n      a {\n        transform: translate(\n          1px /* comment */\n          ,1px\n        );\n      }\n    ").at(5, 11, expected_before).fixed("\n      a {\n        transform: translate(\n          1px /* comment */ ,1px\n        );\n      }\n    "),
  ]);
}

#[test]
fn function_comma_space_before_always_scss() {
  check(
    "@stylistic/function-comma-space-before",
    json!(["always"]),
    Scss,
    &[
      ok("$map: (key: value,key2: value2)"),
      ok("$list: (value, value2)"),
    ],
  );
}

#[test]
fn function_comma_space_before_never() {
  let rejected_before = "Unexpected whitespace before \",\"";
  check(
    "@stylistic/function-comma-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a::before { content: \"func(foo ,bar ,baz)\"; }"),
      ok("a::before { background: url('func(foo ,bar ,baz)'); }"),
      ok("a { background-size: 0 , 0 , 0; }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { transform: translate(1,1); }"),
      ok("a { transform: color(rgb(0, 0,0) lightness(50%)); }"),
      bad("a { transform: translate(1 , 1); }")
        .at(1, 28, rejected_before)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1  , 1); }")
        .at(1, 29, rejected_before)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1\n, 1); }")
        .at(2, 1, rejected_before)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1\r\n, 1); }")
        .at(2, 1, rejected_before)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1\t, 1); }")
        .at(1, 28, rejected_before)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: color(rgb(0, 0 , 0) lightness(50%)); }")
        .at(1, 31, rejected_before)
        .fixed("a { transform: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { transform: color(lightness(50%) rgb(0, 0 , 0)); }")
        .at(1, 46, rejected_before)
        .fixed("a { transform: color(lightness(50%) rgb(0, 0, 0)); }"),
      bad("a { transform: translate(1 /*comment*/ , 1); }")
        .at(1, 40, rejected_before)
        .fixed("a { transform: translate(1 /*comment*/, 1); }"),
      bad("a { transform: translate(1 /*c*/ /*c*/ , 1); }")
        .at(1, 40, rejected_before)
        .fixed("a { transform: translate(1 /*c*/ /*c*/, 1); }"),
    ],
  );
}

#[test]
fn function_comma_space_before_never_scss() {
  check(
    "@stylistic/function-comma-space-before",
    json!(["never"]),
    Scss,
    &[ok("$map: (key: value ,key2: value2)")],
  );
}

#[test]
fn function_comma_space_before_always_single_line() {
  let expected_before_single_line = "Expected single space before \",\" in a single-line function";
  check(
    "@stylistic/function-comma-space-before",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a::before { content: \"func(foo,bar,baz)\"; }"),
      ok("a::before { background: url('func(foo,bar,baz)'); }"),
      ok("a { background-size: 0, 0, 0; }"),
      ok("a { transform: translate(1 , 1); }"),
      ok("a { transform: translate(1 ,1); }"),
      ok("a { transform: color(rgb(0 , 0 ,0) lightness(50%)); }"),
      ok("a { transform: translate(1,\n1); }"),
      ok("a { transform: translate(1  ,\n1); }"),
      ok("a { transform: translate(1\t,\r\n1); }"),
      ok("a { transform: translate(1\n, 1); }"),
      ok("a { transform: translate(1\n,\n1); }"),
      ok("a { background: linear-gradient(45deg,\nrgba(0 , 0 , 0 ,1)\n,red); }"),
      bad("a { transform: color(rgb(0 , 0, 0) lightness(50%)); }")
        .at(1, 31, expected_before_single_line)
        .fixed("a { transform: color(rgb(0 , 0 , 0) lightness(50%)); }"),
      bad("a { transform: color(lightness(50%) rgb(0 , 0, 0)); }")
        .at(1, 46, expected_before_single_line)
        .fixed("a { transform: color(lightness(50%) rgb(0 , 0 , 0)); }"),
      bad("a { background: linear-gradient(45deg,\nrgba(0 , 0,0 ,1),red); }")
        .at(2, 11, expected_before_single_line)
        .fixed("a { background: linear-gradient(45deg,\nrgba(0 , 0 ,0 ,1),red); }"),
    ],
  );
}

#[test]
fn function_comma_space_before_always_single_line_scss() {
  check(
    "@stylistic/function-comma-space-before",
    json!(["always-single-line"]),
    Scss,
    &[ok("$map: (key: value,key2: value2)")],
  );
}

#[test]
fn function_comma_space_before_never_single_line() {
  let rejected_before_single_line = "Unexpected whitespace before \",\" in a single-line function";
  check(
    "@stylistic/function-comma-space-before",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a::before { content: \"func(foo ,bar ,baz)\"; }"),
      ok("a::before { background: url('func(foo ,bar ,baz)'); }"),
      ok("a { background-size: 0 , 0 , 0; }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { transform: translate(1,1); }"),
      ok("a { transform: color(rgb(0, 0,0) lightness(50%)); }"),
      ok("a { transform: translate(1 ,\n1); }"),
      ok("a { transform: translate(1  ,\n1); }"),
      ok("a { transform: translate(1\t,\r\n1); }"),
      ok("a { transform: translate(1\n, 1); }"),
      ok("a { transform: translate(1\n,\n1); }"),
      bad("a { transform: color(rgb(0, 0 , 0) lightness(50%)); }")
        .at(1, 31, rejected_before_single_line)
        .fixed("a { transform: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { transform: color(lightness(50%) rgb(0, 0 , 0)); }")
        .at(1, 46, rejected_before_single_line)
        .fixed("a { transform: color(lightness(50%) rgb(0, 0, 0)); }"),
    ],
  );
}

#[test]
fn function_comma_space_before_never_single_line_scss() {
  check(
    "@stylistic/function-comma-space-before",
    json!(["never-single-line"]),
    Scss,
    &[ok("$map: (key: value ,key2: value2)")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_space_before_never_gale_cases() {
  check(
    "@stylistic/function-comma-space-before",
    json!("never"),
    Css,
    &[
      ok("a { transform: translate(1px, 2px); }"),
      bad("a { transform: translate(1px , 2px); }")
        .at(1, 30, "Unexpected whitespace before \",\"")
        .fixed("a { transform: translate(1px, 2px); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_space_before_always_gale_cases() {
  check(
    "@stylistic/function-comma-space-before",
    json!("always"),
    Css,
    &[
      ok("a { transform: translate(1px ,2px); }"),
      bad("a { transform: translate(1px,2px); }")
        .at(1, 29, "Expected single space before \",\"")
        .fixed("a { transform: translate(1px ,2px); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_space_before_always_single_line_gale_cases() {
  check(
    "@stylistic/function-comma-space-before",
    json!("always-single-line"),
    Css,
    &[
      bad("a { color: rgb(0, 0 ,0); }")
        .at(
          1,
          17,
          "Expected single space before \",\" in a single-line function",
        )
        .fixed("a { color: rgb(0 , 0 ,0); }"),
      ok("a { color: rgb(0,\n  0, 0); }"),
      bad("a { color: rgb(0 , 0,0); }")
        .at(
          1,
          21,
          "Expected single space before \",\" in a single-line function",
        )
        .fixed("a { color: rgb(0 , 0 ,0); }"),
      ok("a { color: rgb(0 ,\n  0, 0); }"),
    ],
  );
}

#[test]
fn function_comma_newline_after_always() {
  let expected_after = "Expected newline after \",\"";
  check("@stylistic/function-comma-newline-after", json!(["always"]), Css, &[
    ok("a::before { content: \"func(foo, bar, baz)\"; }"),
    ok("a::before { background: url('func(foo,bar,baz)'); }"),
    ok("a { background-size: 0,\n  0,\n  0; }"),
    ok("a { transform: translate(1 ,\n1); }"),
    ok("a { transform: translate(1 ,\n\n1); }"),
    ok("a { transform: translate(\n  1,\n  1\n); }"),
    ok("a { transform: translate(1,\r\n1); }"),
    ok("a { transform: translate(1,\r\n\r\n1); }"),
    ok("a { transform: color(rgb(0 ,\n\t0,\n\t0) lightness(50%)); }"),
    ok("a { background: linear-gradient(45deg,\n rgba(0,\n 0,\n 0,\n 1),\n red); }"),
    ok("\n      a {\n        transform: translate(\n          1px, /* comment */\n          1px\n        );\n      }\n    "),
    bad("a { transform: translate(1,1); }").at(1, 27, expected_after).fixed("a { transform: translate(1,\n1); }"),
    bad("a { transform: translate(1,  1); }").at(1, 27, expected_after).fixed("a { transform: translate(1,\n1); }"),
    bad("a { transform: translate(1, 1); }").at(1, 27, expected_after).fixed("a { transform: translate(1,\n1); }"),
    bad("a\r\n{ transform: translate(1, 1); }").at(2, 25, expected_after).fixed("a\r\n{ transform: translate(1,\r\n1); }"),
    bad("a { transform: translate(1,\t1); }").at(1, 27, expected_after).fixed("a { transform: translate(1,\n1); }"),
    bad("a { transform: color(rgb(0 , 0 ,\n0) lightness(50%)); }").at(1, 28, expected_after).fixed("a { transform: color(rgb(0 ,\n0 ,\n0) lightness(50%)); }"),
    bad("a { transform: color(lightness(50%) rgb(0 ,\n 0 ,0)); }").at(2, 4, expected_after).fixed("a { transform: color(lightness(50%) rgb(0 ,\n 0 ,\n0)); }"),
    bad("a { background: linear-gradient(45deg,\n rgba(0,\n 0, 0,\n 1),\n red); }").at(3, 3, expected_after).fixed("a { background: linear-gradient(45deg,\n rgba(0,\n 0,\n0,\n 1),\n red); }"),
    bad("a { transform: translate(\n  1,1\n); }").at(2, 4, expected_after).fixed("a { transform: translate(\n  1,\n1\n); }"),
    bad("a {\n  transform: translate(\n    1px,  /* comment (with trailing space) */  \n    1px\n  );\n}").at(3, 8, expected_after).fixed("a {\n  transform: translate(\n    1px,\n/* comment (with trailing space) */  \n    1px\n  );\n}"),
  ]);
}

#[test]
fn function_comma_newline_after_always_scss() {
  check(
    "@stylistic/function-comma-newline-after",
    json!(["always"]),
    Scss,
    &[
      ok("$map: (key: value, key2: value2)"),
      ok("$list: (value, value2)"),
      ok(
        "\n      a {\n        transform: translate(\n          1px, // line comment\n          1px\n        );\n      }\n    ",
      ),
    ],
  );
}

#[test]
fn function_comma_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \",\" in a multi-line function";
  check("@stylistic/function-comma-newline-after", json!(["always-multi-line"]), Css, &[
    ok("a::before { content: \"func(foo, bar, baz)\"; }"),
    ok("a::before { background: url('func(foo,bar,baz)'); }"),
    ok("a { background-size: 0,\n  0,\n  0; }"),
    ok("a { transform: translate(1 ,\n1); }"),
    ok("a { transform: translate(1,\r\n1); }"),
    ok("a { transform: color(rgb(0 ,\n\t0,\n\t0) lightness(50%)); }"),
    ok("a { transform: translate(1,1); }"),
    ok("a { transform: translate(1,  1); }"),
    ok("a { transform: translate(1, 1); }"),
    ok("a { transform: translate(1,\t1); }"),
    ok("a {\r\n  transform:\r\n    translate(1,1)\r\n  scale(3);\r\n}"),
    ok("\n      .foo {\n        box-shadow:\n          inset 0 8px 8px -8px rgba(0, 0, 0, 1)\n          inset 0 -10px 12px 0 #f00;\n      }\n    "),
    ok("\n      .foo {\n        background-image:\n          repeating-linear-gradient(\n            -45deg,\n            transparent,\n            rgba(0, 0, 0, 1) 5px\n          );\n        }\n    "),
    ok("\n      a {\n        transform: translate(\n          1px, /* comment */\n          1px\n        );\n      }\n    "),
    bad("a { transform: color(rgb(0 , 0 ,\n0) lightness(50%)); }").at(1, 28, expected_after_multi_line).fixed("a { transform: color(rgb(0 ,\n0 ,\n0) lightness(50%)); }"),
    bad("a\r\n{ transform: color(rgb(0 , 0 ,\r\n0) lightness(50%)); }").at(2, 26, expected_after_multi_line).fixed("a\r\n{ transform: color(rgb(0 ,\r\n0 ,\r\n0) lightness(50%)); }"),
    bad("a { transform: color(lightness(50%) rgb(0 ,\n 0 ,0)); }").at(2, 4, expected_after_multi_line).fixed("a { transform: color(lightness(50%) rgb(0 ,\n 0 ,\n0)); }"),
    bad("a { background-image: repeating-linear-gradient(\n-45deg,\ntransparent, rgba(0, 0, 0, 1) 5px\n);}").at(3, 12, expected_after_multi_line).fixed("a { background-image: repeating-linear-gradient(\n-45deg,\ntransparent,\nrgba(0, 0, 0, 1) 5px\n);}"),
    bad("a { background: linear-gradient(45deg,rgba(0,\n0 ,\n 0 ,\n 1)); }").at(1, 38, expected_after_multi_line).fixed("a { background: linear-gradient(45deg,\nrgba(0,\n0 ,\n 0 ,\n 1)); }"),
    bad("a { transform: translate(\n  1,1\n); }").at(2, 4, expected_after_multi_line).fixed("a { transform: translate(\n  1,\n1\n); }"),
    bad("a {\n  transform: translate(\n    1px,  /* comment (with trailing space) */  \n    1px\n  );\n}").at(3, 8, expected_after_multi_line).fixed("a {\n  transform: translate(\n    1px,\n/* comment (with trailing space) */  \n    1px\n  );\n}"),
  ]);
}

#[test]
fn function_comma_newline_after_always_multi_line_scss() {
  check(
    "@stylistic/function-comma-newline-after",
    json!(["always-multi-line"]),
    Scss,
    &[
      ok("$map: (key: value\n, key2: value2)"),
      ok(
        "\n      a {\n        transform: translate(\n          1px, // line comment\n          1px\n        );\n      }\n    ",
      ),
    ],
  );
}

#[test]
fn function_comma_newline_after_never_multi_line() {
  let rejected_after_multi_line = "Unexpected whitespace after \",\" in a multi-line function";
  check(
    "@stylistic/function-comma-newline-after",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a::before { content: \"func(foo,\n bar,\n baz)\"; }"),
      ok("a::before { background: url('func(foo,\nbar,\nbaz)'); }"),
      ok("a { background-size: 0\n, 0\n, 0; }"),
      ok("a { transform: translate(1\r\n,1); }"),
      ok("a { transform: color(rgb(0\n\t,0\n\t,0) lightness(50%)); }"),
      ok("a { transform: translate(1,1); }"),
      ok("a { transform: translate(1,  1); }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { transform: translate(1,\t1); }"),
      ok("a { background: linear-gradient(45deg\n,rgba(0, 0, 0, 1),red); }"),
      ok("\n      a {\n        transform: translate(1px, 1px); /* comment */\n      }\n    "),
      bad("a { transform: color(rgb(0 ,0 ,\n0) lightness(50%)); }")
        .at(1, 31, rejected_after_multi_line)
        .fixed("a { transform: color(rgb(0 ,0 ,0) lightness(50%)); }"),
      bad("a { transform: color(rgb(0 ,0 ,\r\n0) lightness(50%)); }")
        .at(1, 31, rejected_after_multi_line)
        .fixed("a { transform: color(rgb(0 ,0 ,0) lightness(50%)); }"),
      bad("a { transform: color(lightness(50%) rgb(0 ,\n 0 ,0)); }")
        .at(1, 43, rejected_after_multi_line)
        .fixed("a { transform: color(lightness(50%) rgb(0 ,0 ,0)); }"),
      bad("a { transform: color(rgb(0\n,0 ,\n0) lightness(50%)); }")
        .at(2, 4, rejected_after_multi_line)
        .fixed("a { transform: color(rgb(0\n,0 ,0) lightness(50%)); }"),
      bad("a { transform: color(lightness(50%) rgb(0 ,\n 0\n,0)); }")
        .at(1, 43, rejected_after_multi_line)
        .fixed("a { transform: color(lightness(50%) rgb(0 ,0\n,0)); }"),
      bad("a { background: linear-gradient(45deg\n,rgba(0,0 , 0, 1), red); }")
        .at(2, 18, rejected_after_multi_line)
        .fixed("a { background: linear-gradient(45deg\n,rgba(0,0 , 0, 1),red); }"),
    ],
  );
}

#[test]
fn function_comma_newline_after_never_multi_line_scss() {
  check(
    "@stylistic/function-comma-newline-after",
    json!(["never-multi-line"]),
    Scss,
    &[
      ok("$map: (key: value,\nkey2: value2)"),
      ok("\n      a {\n        transform: translate(1px, 1px); // line comment\n      }\n    "),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_newline_after_always_gale_cases() {
  check(
    "@stylistic/function-comma-newline-after",
    json!("always"),
    Css,
    &[
      ok("a { transform: translate(1px,\n2px); }"),
      bad("a { transform: translate(1px, 2px); }")
        .at(1, 29, "Expected newline after \",\"")
        .fixed("a { transform: translate(1px,\n2px); }"),
      ok("a:not(.foo,\n.bar) { color: red; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_newline_after_always_multi_line_gale_cases() {
  check(
    "@stylistic/function-comma-newline-after",
    json!("always-multi-line"),
    Css,
    &[
      ok("a { transform: translate(1px, 2px); }"),
      bad("a { transform: translate(1px, \n2px); }")
        .at(
          1,
          29,
          "Expected newline after \",\" in a multi-line function",
        )
        .fixed("a { transform: translate(1px,\n2px); }"),
      bad("a {\n  transform: translate(1px, 2px,\n3px);\n}")
        .at(
          2,
          27,
          "Expected newline after \",\" in a multi-line function",
        )
        .fixed("a {\n  transform: translate(1px,\n2px,\n3px);\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_comma_newline_after_never_multi_line_gale_cases() {
  check(
    "@stylistic/function-comma-newline-after",
    json!("never-multi-line"),
    Css,
    &[ok("a { transform: translate(1px, 2px); }")],
  );
}

#[test]
fn media_query_list_comma_space_after_always() {
  let expected_after = "Expected single space after \",\"";
  check(
    "@stylistic/media-query-list-comma-space-after",
    json!(["always"]),
    Css,
    &[
      ok("@import url(x.com?a=b,c=d)"),
      ok("@media (max-width: 600px) {}"),
      ok("@mEdIa (max-width: 600px) {}"),
      ok("@MEDIA (max-width: 600px) {}"),
      ok("@media screen and (color), projection and (color) {}"),
      ok("@media screen and (color) , projection and (color) {}"),
      ok("@media screen and (color)\n, projection and (color) {}"),
      ok("@media screen and (color)\r\n, projection and (color) {}"),
      ok("@non-media screen and (color),projection and (color) {}"),
      ok("@media-non screen and (color),projection and (color) {}"),
      bad("@media screen and (color),projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@mEdIa screen and (color),projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@mEdIa screen and (color), projection and (color) {}"),
      bad("@MEDIA screen and (color),projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@MEDIA screen and (color), projection and (color) {}"),
      bad("@media screen and (color),  projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color),\nprojection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color),\r\nprojection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color),\tprojection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color),/*comment*/projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color), /*comment*/projection and (color) {}"),
      bad("@media tv,tv,tv,, print {}")
        .at(1, 10, expected_after)
        .at(1, 13, expected_after)
        .at(1, 16, expected_after)
        .fixed("@media tv, tv, tv, , print {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_space_after_never() {
  let rejected_after = "Unexpected whitespace after \",\"";
  check(
    "@stylistic/media-query-list-comma-space-after",
    json!(["never"]),
    Css,
    &[
      ok("@import url(x.com?a=b, c=d)"),
      ok("@media (max-width: 600px) {}"),
      ok("@mEdIa (max-width: 600px) {}"),
      ok("@MEDIA (max-width: 600px) {}"),
      ok("@media screen and (color),projection and (color) {}"),
      ok("@media screen and (color) ,projection and (color) {}"),
      ok("@media screen and (color)\n,projection and (color) {}"),
      ok("@media screen and (color)\r\n,projection and (color) {}"),
      ok("@non-media screen and (color), projection and (color) {}"),
      ok("@media-non screen and (color), projection and (color) {}"),
      bad("@media screen and (color), projection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@mEdIa screen and (color), projection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@mEdIa screen and (color),projection and (color) {}"),
      bad("@MEDIA screen and (color), projection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@MEDIA screen and (color),projection and (color) {}"),
      bad("@media screen and (color),  projection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@media screen and (color),\nprojection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@media screen and (color),\r\nprojection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@media screen and (color),\tprojection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@media screen and (color), /*comment*/ projection and (color) {}")
        .at(1, 26, rejected_after)
        .fixed("@media screen and (color),/*comment*/ projection and (color) {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_space_after_always_single_line() {
  let expected_after_single_line = "Expected single space after \",\" in a single-line list";
  check(
    "@stylistic/media-query-list-comma-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("@media screen and (color), projection and (color) {}"),
      ok("@mEdIa screen and (color), projection and (color) {}"),
      ok("@MEDIA screen and (color), projection and (color) {}"),
      ok("@media screen and (color), projection and (color) {\n}"),
      ok("@media screen and (color), projection and (color) {\r\n}"),
      ok("@media screen and (color)\n,projection and (color) {}"),
      ok("@media screen and (color)\r\n,projection and (color) {}"),
      ok("@non-media screen and (color) ,projection and (color) {}"),
      ok("@media-non screen and (color) ,projection and (color) {}"),
      bad("@media screen and (color) ,projection and (color) {}")
        .at(1, 27, expected_after_single_line)
        .fixed("@media screen and (color) , projection and (color) {}"),
      bad("@mEdIa screen and (color) ,projection and (color) {}")
        .at(1, 27, expected_after_single_line)
        .fixed("@mEdIa screen and (color) , projection and (color) {}"),
      bad("@MEDIA screen and (color) ,projection and (color) {}")
        .at(1, 27, expected_after_single_line)
        .fixed("@MEDIA screen and (color) , projection and (color) {}"),
      bad("@media screen and (color) ,projection and (color) {\n}")
        .at(1, 27, expected_after_single_line)
        .fixed("@media screen and (color) , projection and (color) {\n}"),
      bad("@media screen and (color) ,projection and (color) {\r\n}")
        .at(1, 27, expected_after_single_line)
        .fixed("@media screen and (color) , projection and (color) {\r\n}"),
    ],
  );
}

#[test]
fn media_query_list_comma_space_after_never_single_line() {
  let rejected_after_single_line = "Unexpected whitespace after \",\" in a single-line list";
  check(
    "@stylistic/media-query-list-comma-space-after",
    json!(["never-single-line"]),
    Css,
    &[
      ok("@media screen and (color) ,projection and (color) {}"),
      ok("@mEdIa screen and (color) ,projection and (color) {}"),
      ok("@MEDIA screen and (color) ,projection and (color) {}"),
      ok("@media screen and (color) ,projection and (color) {\n}"),
      ok("@media screen and (color) ,projection and (color) {\r\n}"),
      ok("@media screen and (color),\nprojection and (color) {}"),
      ok("@media screen and (color),\r\nprojection and (color) {}"),
      ok("@non-media screen and (color), projection and (color) {}"),
      ok("@media-non screen and (color), projection and (color) {}"),
      bad("@media screen and (color), projection and (color) {}")
        .at(1, 26, rejected_after_single_line)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@mEdIa screen and (color), projection and (color) {}")
        .at(1, 26, rejected_after_single_line)
        .fixed("@mEdIa screen and (color),projection and (color) {}"),
      bad("@MEDIA screen and (color), projection and (color) {}")
        .at(1, 26, rejected_after_single_line)
        .fixed("@MEDIA screen and (color),projection and (color) {}"),
      bad("@media screen and (color), projection and (color) {\n}")
        .at(1, 26, rejected_after_single_line)
        .fixed("@media screen and (color),projection and (color) {\n}"),
      bad("@media screen and (color), projection and (color) {\r\n}")
        .at(1, 26, rejected_after_single_line)
        .fixed("@media screen and (color),projection and (color) {\r\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_space_after_always_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-space-after",
    json!("always"),
    Css,
    &[
      ok("@media screen, print { }"),
      bad("@media screen,print { }")
        .at(1, 14, "Expected single space after \",\"")
        .fixed("@media screen, print { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_space_after_never_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-space-after",
    json!("never"),
    Css,
    &[bad("@media screen, print { }")
      .at(1, 14, "Unexpected whitespace after \",\"")
      .fixed("@media screen,print { }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_space_after_never_single_line_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-space-after",
    json!("never-single-line"),
    Css,
    &[
      bad("@media screen, print {}")
        .at(
          1,
          14,
          "Unexpected whitespace after \",\" in a single-line list",
        )
        .fixed("@media screen,print {}"),
      ok("@media screen, print,\nprojection {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_space_before_always() {
  let expected_before = "Expected single space before \",\"";
  check(
    "@stylistic/media-query-list-comma-space-before",
    json!(["always"]),
    Css,
    &[
      ok("@import url(x.com?a=b,c=d)"),
      ok("@media (max-width: 600px) {}"),
      ok("@mEdIa (max-width: 600px) {}"),
      ok("@MEDIA (max-width: 600px) {}"),
      ok("@media screen and (color) , projection and (color) {}"),
      ok("@media screen and (color) ,  projection and (color) {}"),
      ok("@media screen and (color) ,\nprojection and (color) {}"),
      ok("@media screen and (color) ,\r\nprojection and (color) {}"),
      ok("@non-media screen and (color), projection and (color) {}"),
      ok("@media-non screen and (color), projection and (color) {}"),
      bad("@media screen and (color), projection and (color) {}")
        .at(1, 26, expected_before)
        .fixed("@media screen and (color) , projection and (color) {}"),
      bad("@mEdIa screen and (color), projection and (color) {}")
        .at(1, 26, expected_before)
        .fixed("@mEdIa screen and (color) , projection and (color) {}"),
      bad("@MEDIA screen and (color), projection and (color) {}")
        .at(1, 26, expected_before)
        .fixed("@MEDIA screen and (color) , projection and (color) {}"),
      bad("@media screen and (color)  , projection and (color) {}")
        .at(1, 28, expected_before)
        .fixed("@media screen and (color) , projection and (color) {}"),
      bad("@media screen and (color)\n, projection and (color) {}")
        .at(2, 1, expected_before)
        .fixed("@media screen and (color) , projection and (color) {}"),
      bad("@media screen and (color)\r\n, projection and (color) {}")
        .at(2, 1, expected_before)
        .fixed("@media screen and (color) , projection and (color) {}"),
      bad("@media screen and (color)\t, projection and (color) {}")
        .at(1, 27, expected_before)
        .fixed("@media screen and (color) , projection and (color) {}"),
      bad("@media screen and (color)/*comment*/, projection and (color) {}")
        .at(1, 37, expected_before)
        .fixed("@media screen and (color)/*comment*/ , projection and (color) {}"),
      bad("@media tv,tv,tv,print {}")
        .at(1, 10, expected_before)
        .at(1, 13, expected_before)
        .at(1, 16, expected_before)
        .fixed("@media tv ,tv ,tv ,print {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_space_before_never() {
  let rejected_before = "Unexpected whitespace before \",\"";
  check(
    "@stylistic/media-query-list-comma-space-before",
    json!(["never"]),
    Css,
    &[
      ok("@import url(x.com?a=b ,c=d)"),
      ok("@media (max-width: 600px) {}"),
      ok("@mEdIa (max-width: 600px) {}"),
      ok("@MEDIA (max-width: 600px) {}"),
      ok("@media screen and (color),projection and (color) {}"),
      ok("@media screen and (color), projection and (color) {}"),
      ok("@media screen and (color),\nprojection and (color) {}"),
      ok("@media screen and (color),\r\nprojection and (color) {}"),
      ok("@non-media screen and (color) , projection and (color) {}"),
      ok("@media-non screen and (color) , projection and (color) {}"),
      bad("@media screen and (color) , projection and (color) {}")
        .at(1, 27, rejected_before)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@mEdIa screen and (color) , projection and (color) {}")
        .at(1, 27, rejected_before)
        .fixed("@mEdIa screen and (color), projection and (color) {}"),
      bad("@MEDIA screen and (color) , projection and (color) {}")
        .at(1, 27, rejected_before)
        .fixed("@MEDIA screen and (color), projection and (color) {}"),
      bad("@media screen and (color)  , projection and (color) {}")
        .at(1, 28, rejected_before)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color)\n, projection and (color) {}")
        .at(2, 1, rejected_before)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color)\r\n, projection and (color) {}")
        .at(2, 1, rejected_before)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color)\t, projection and (color) {}")
        .at(1, 27, rejected_before)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color) /*comment*/ , projection and (color) {}")
        .at(1, 39, rejected_before)
        .fixed("@media screen and (color) /*comment*/, projection and (color) {}"),
      bad("@media tv ,tv ,tv ,print {}")
        .at(1, 11, rejected_before)
        .at(1, 15, rejected_before)
        .at(1, 19, rejected_before)
        .fixed("@media tv,tv,tv,print {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_space_before_always_single_line() {
  let expected_before_single_line = "Expected single space before \",\" in a single-line list";
  check(
    "@stylistic/media-query-list-comma-space-before",
    json!(["always-single-line"]),
    Css,
    &[
      ok("@media screen and (color) ,projection and (color) {}"),
      ok("@mEdIa screen and (color) ,projection and (color) {}"),
      ok("@MEDIA screen and (color) ,projection and (color) {}"),
      ok("@media screen and (color) ,projection and (color) {\n}"),
      ok("@media screen and (color) ,projection and (color) {\r\n}"),
      ok("@media screen and (color),\nprojection and (color) {}"),
      ok("@media screen and (color),\r\nprojection and (color) {}"),
      ok("@non-media screen and (color), projection and (color) {}"),
      ok("@media-non screen and (color), projection and (color) {}"),
      bad("@media screen and (color), projection and (color) {}")
        .at(1, 26, expected_before_single_line)
        .fixed("@media screen and (color) , projection and (color) {}"),
      bad("@mEdIa screen and (color), projection and (color) {}")
        .at(1, 26, expected_before_single_line)
        .fixed("@mEdIa screen and (color) , projection and (color) {}"),
      bad("@MEDIA screen and (color), projection and (color) {}")
        .at(1, 26, expected_before_single_line)
        .fixed("@MEDIA screen and (color) , projection and (color) {}"),
      bad("@media screen and (color), projection and (color) {\n}")
        .at(1, 26, expected_before_single_line)
        .fixed("@media screen and (color) , projection and (color) {\n}"),
      bad("@media screen and (color), projection and (color) {\r\n}")
        .at(1, 26, expected_before_single_line)
        .fixed("@media screen and (color) , projection and (color) {\r\n}"),
    ],
  );
}

#[test]
fn media_query_list_comma_space_before_never_single_line() {
  let rejected_before_single_line = "Unexpected whitespace before \",\" in a single-line list";
  check(
    "@stylistic/media-query-list-comma-space-before",
    json!(["never-single-line"]),
    Css,
    &[
      ok("@media screen and (color), projection and (color) {}"),
      ok("@mEdIa screen and (color), projection and (color) {}"),
      ok("@MEDIA screen and (color), projection and (color) {}"),
      ok("@media screen and (color), projection and (color) {\n}"),
      ok("@media screen and (color), projection and (color) {\r\n}"),
      ok("@media screen and (color)\n,projection and (color) {}"),
      ok("@media screen and (color)\r\n,projection and (color) {}"),
      ok("@non-media screen and (color) ,projection and (color) {}"),
      ok("@media-non screen and (color) ,projection and (color) {}"),
      bad("@media screen and (color) ,projection and (color) {}")
        .at(1, 27, rejected_before_single_line)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@mEdIa screen and (color) ,projection and (color) {}")
        .at(1, 27, rejected_before_single_line)
        .fixed("@mEdIa screen and (color),projection and (color) {}"),
      bad("@MEDIA screen and (color) ,projection and (color) {}")
        .at(1, 27, rejected_before_single_line)
        .fixed("@MEDIA screen and (color),projection and (color) {}"),
      bad("@media screen and (color) ,projection and (color) {\n}")
        .at(1, 27, rejected_before_single_line)
        .fixed("@media screen and (color),projection and (color) {\n}"),
      bad("@media screen and (color) ,projection and (color) {\r\n}")
        .at(1, 27, rejected_before_single_line)
        .fixed("@media screen and (color),projection and (color) {\r\n}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_space_before_never_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-space-before",
    json!("never"),
    Css,
    &[
      ok("@media screen, print { }"),
      bad("@media screen , print { }")
        .at(1, 15, "Unexpected whitespace before \",\"")
        .fixed("@media screen, print { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_space_before_always_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-space-before",
    json!("always"),
    Css,
    &[bad("@media screen, print { }")
      .at(1, 14, "Expected single space before \",\"")
      .fixed("@media screen , print { }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_space_before_always_single_line_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-space-before",
    json!("always-single-line"),
    Css,
    &[
      bad("@media screen, print ,tv {}")
        .at(
          1,
          14,
          "Expected single space before \",\" in a single-line list",
        )
        .fixed("@media screen , print ,tv {}"),
      ok("@media screen, print ,\ntv {}"),
      bad("@media screen , print,tv {}")
        .at(
          1,
          22,
          "Expected single space before \",\" in a single-line list",
        )
        .fixed("@media screen , print ,tv {}"),
      ok("@media screen , print,\ntv {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_newline_after_always() {
  let expected_after = "Expected newline after \",\"";
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!(["always"]),
    Css,
    &[
      ok("@import url(x.com?a=b,c=d)"),
      ok("@media (max-width: 600px) {}"),
      ok("@mEdIa (max-width: 600px) {}"),
      ok("@MEDIA (max-width: 600px) {}"),
      ok("@media screen and (color),\nprojection and (color) {}"),
      ok("@media screen and (color),\n\nprojection and (color) {}"),
      ok("@media screen and (color) ,\n  projection and (color) {}"),
      ok("@media screen and (color) ,\r\n  projection and (color) {}"),
      ok("@media screen and (color) ,\r\n\r\n  projection and (color) {}"),
      ok("@media screen and (color)\n,\n\t\t\tprojection and (color) {}"),
      ok("@media screen and (color)\r\n,\r\n\t\t\tprojection and (color) {}"),
      ok("@non-media screen and (color),projection and (color) {}"),
      ok("@media-non screen and (color),projection and (color) {}"),
      ok("@media screen and (color),/*comment*/\nprojection and (color) {}"),
      ok("@media screen and (color), /*comment*/\nprojection and (color) {}"),
      ok("@media screen and (color), /*comment1*/ /*comment2*/\nprojection and (color) {}"),
      ok("@media screen and (color),/*comment*/\r\nprojection and (color) {}"),
      ok("@media screen and (color), /*comment*/\r\nprojection and (color) {}"),
      ok("@media screen and (color), /*comment1*/ /*com\r\nment2*/\r\nprojection and (color) {}"),
      bad("@media screen and (color),projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\nprojection and (color) {}"),
      bad("@mEdIa screen and (color),projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@mEdIa screen and (color),\nprojection and (color) {}"),
      bad("@MEDIA screen and (color),projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@MEDIA screen and (color),\nprojection and (color) {}"),
      bad("@media screen and (color), projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\n projection and (color) {}"),
      bad("@media screen and (color),  projection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\n  projection and (color) {}"),
      bad("@media screen and (color),\tprojection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\n\tprojection and (color) {}"),
      bad("@media screen and (color), \n\tprojection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\n\tprojection and (color) {}"),
      bad("@media screen and (color),projection and (color) {\r\n}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\r\nprojection and (color) {\r\n}"),
      bad("@media screen and (color), \r\n\tprojection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\r\n\tprojection and (color) {}"),
      bad("@media screen and (color),/**/projection and (color) {}")
        .at(1, 30, expected_after)
        .fixed("@media screen and (color),/**/\nprojection and (color) {}"),
      bad("@media screen and (color),/**/ \nprojection and (color) {}")
        .at(1, 30, expected_after)
        .fixed("@media screen and (color),/**/\nprojection and (color) {}"),
      bad("@media screen and (color), /*comment1*/ /*comment2*/ projection and (color) {}")
        .at(1, 52, expected_after)
        .fixed("@media screen and (color), /*comment1*/ /*comment2*/\n projection and (color) {}"),
      bad("@media screen and (color),\t/*comment1*/\t/*comment2*/ \nprojection and (color) {}")
        .at(1, 52, expected_after)
        .fixed("@media screen and (color),\t/*comment1*/\t/*comment2*/\nprojection and (color) {}"),
      bad("@media screen and (color),\t   \n/*comment*/\nprojection and (color) {}")
        .at(1, 26, expected_after)
        .fixed("@media screen and (color),\n/*comment*/\nprojection and (color) {}"),
      bad("@media tv,tv,tv,print {}")
        .at(1, 10, expected_after)
        .at(1, 13, expected_after)
        .at(1, 16, expected_after)
        .fixed("@media tv,\ntv,\ntv,\nprint {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_newline_after_always_multi_line() {
  let expected_after_multi_line = "Expected newline after \",\" in a multi-line list";
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("@media screen and (color),\nprojection and (color) {}"),
      ok("@mEdIa screen and (color),\nprojection and (color) {}"),
      ok("@MEDIA screen and (color),\nprojection and (color) {}"),
      ok("@media screen and (color),\r\nprojection and (color) {}"),
      ok("@media screen and (color),\nprojection and (color) {\n}"),
      ok("@media screen and (color),projection and (color) {}"),
      ok("@media screen and (color),projection and (color) {\n}"),
      ok("@media screen and (color),projection and (color) {\r\n}"),
      ok("@non-media screen and (color),projection and (color),\nprint {}"),
      ok("@media-non screen and (color),projection and (color),\nprint {}"),
      bad("@media screen and (color),projection and (color),\nprint {}")
        .at(1, 26, expected_after_multi_line)
        .fixed("@media screen and (color),\nprojection and (color),\nprint {}"),
      bad("@mEdIa screen and (color),projection and (color),\nprint {}")
        .at(1, 26, expected_after_multi_line)
        .fixed("@mEdIa screen and (color),\nprojection and (color),\nprint {}"),
      bad("@MEDIA screen and (color),projection and (color),\nprint {}")
        .at(1, 26, expected_after_multi_line)
        .fixed("@MEDIA screen and (color),\nprojection and (color),\nprint {}"),
      bad("@media screen and (color),projection and (color),\nprint {\n}")
        .at(1, 26, expected_after_multi_line)
        .fixed("@media screen and (color),\nprojection and (color),\nprint {\n}"),
      bad("@media screen and (color),projection and (color),\r\nprint {\r\n}")
        .at(1, 26, expected_after_multi_line)
        .fixed("@media screen and (color),\r\nprojection and (color),\r\nprint {\r\n}"),
    ],
  );
}

#[test]
fn media_query_list_comma_newline_after_never_multi_line() {
  let rejected_after_multi_line = "Unexpected whitespace after \",\" in a multi-line list";
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("@media screen and (color)\n,projection and (color) {}"),
      ok("@mEdIa screen and (color)\n,projection and (color) {}"),
      ok("@MEDIA screen and (color)\n,projection and (color) {}"),
      ok("@media screen and (color)\r\n,projection and (color) {}"),
      ok("@media screen and (color)\n,projection and (color) {\n}"),
      ok("@media screen and (color)\r\n,projection and (color) {\r\n}"),
      ok("@media screen and (color), projection and (color) {}"),
      ok("@media screen and (color), projection and (color) {\n}"),
      ok("@non-media screen and (color) ,projection and (color),\nprint {}"),
      ok("@media-non screen and (color) ,projection and (color),\nprint {}"),
      bad("@media screen and (color) ,projection and (color),\nprint {}")
        .at(1, 50, rejected_after_multi_line)
        .fixed("@media screen and (color) ,projection and (color),print {}"),
      bad("@mEdIa screen and (color) ,projection and (color),\nprint {}")
        .at(1, 50, rejected_after_multi_line)
        .fixed("@mEdIa screen and (color) ,projection and (color),print {}"),
      bad("@MEDIA screen and (color) ,projection and (color),\nprint {}")
        .at(1, 50, rejected_after_multi_line)
        .fixed("@MEDIA screen and (color) ,projection and (color),print {}"),
      bad("@media screen and (color) ,projection and (color),\nprint {\n}")
        .at(1, 50, rejected_after_multi_line)
        .fixed("@media screen and (color) ,projection and (color),print {\n}"),
      bad("@media screen and (color) ,projection and (color),\r\nprint {\r\n}")
        .at(1, 50, rejected_after_multi_line)
        .fixed("@media screen and (color) ,projection and (color),print {\r\n}"),
      bad("@media screen and (color),\t\n projection and (color) {}")
        .at(1, 26, rejected_after_multi_line)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@media screen and (color),\t\r\n projection and (color) {}")
        .at(1, 26, rejected_after_multi_line)
        .fixed("@media screen and (color),projection and (color) {}"),
      bad("@media screen and (color),\n/**/\nprojection and (color) {}")
        .at(1, 26, rejected_after_multi_line)
        .fixed("@media screen and (color),/**/\nprojection and (color) {}"),
      bad("@media screen and (color), /*comment1*/\n/*comment2*/\nprojection and (color) {}")
        .at(1, 26, rejected_after_multi_line)
        .fixed("@media screen and (color),/*comment1*/\n/*comment2*/\nprojection and (color) {}"),
      bad("@media tv,\ntv,\ntv,\nprint {\n}")
        .at(1, 10, rejected_after_multi_line)
        .at(2, 3, rejected_after_multi_line)
        .at(3, 3, rejected_after_multi_line)
        .fixed("@media tv,tv,tv,print {\n}"),
    ],
  );
}

#[test]
fn media_query_list_comma_newline_after_always_scss() {
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!(["always"]),
    Scss,
    &[
      ok("@media screen and (color),// scss\n projection and (color) {}"),
      ok("@media screen and (color),\t // scss\n projection and (color) {}"),
      ok("@media screen and (color),\n// scss\n projection and (color) {}"),
    ],
  );
}

#[test]
fn media_query_list_comma_newline_after_always_less() {
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!(["always"]),
    Less,
    &[
      ok("@media screen and (color),// less\n projection and (color) {}"),
      ok("@media screen and (color),\t // less\r\n projection and (color) {}"),
      ok("@media screen and (color),\n// less\n projection and (color) {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_newline_after_always_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!("always"),
    Css,
    &[
      ok("@media screen,\nprint { }"),
      bad("@media screen, print { }")
        .at(1, 14, "Expected newline after \",\"")
        .fixed("@media screen,\n print { }"),
    ],
  );
}

// Gale's own `"never"`, which the plugin does not take: no line break
// next to the character.  Gale adds no fix for it.
#[test]
fn media_query_list_comma_newline_after_never_gale_only() {
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!("never"),
    Css,
    &[ok("@media screen, print { }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_query_list_comma_newline_after_never_multi_line_gale_cases() {
  check(
    "@stylistic/media-query-list-comma-newline-after",
    json!("never-multi-line"),
    Css,
    &[
      bad("@media screen, print,\nprojection {}")
        .at(
          1,
          14,
          "Unexpected whitespace after \",\" in a multi-line list",
        )
        .at(
          1,
          21,
          "Unexpected whitespace after \",\" in a multi-line list",
        )
        .fixed("@media screen,print,projection {}"),
      ok("@media screen, print {}"),
    ],
  );
}
