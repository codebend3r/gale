//! The `combinator` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn selector_combinator_space_after_always() {
  check(
    "@stylistic/selector-combinator-space-after",
    json!(["always"]),
    Css,
    &[
      ok("a + a {}"),
      ok("a > a {}"),
      ok("a ~ a {}"),
      ok("a >>> a {}"),
      ok(".foo ~ a + bar {}"),
      ok("a+ a {}"),
      ok("a> a {}"),
      ok("a~ a {}"),
      ok("a>>> a {}"),
      ok("a\n+ a {}"),
      ok("a\r\n+ a {}"),
      ok("a\n> a {}"),
      ok("a\r\n> a {}"),
      ok("a\n~ a {}"),
      ok("a\n>>> a {}"),
      ok("a\r\n>>> a {}"),
      ok("a~ a+ bar {}"),
      ok(".foo:nth-child(2n+1) {}"),
      ok(".foo:nth-child(2n-1) {}"),
      ok("a[rel~='copyright'] {}"),
      ok(".foo\\+bar {}"),
      ok("a [type='button'] {}"),
      ok("a  a {}"),
      ok("a\na {}"),
      ok("a\r\na {}"),
      ok("a\n\na {}"),
      ok("a\r\n\r\na {}"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("namespace|type#id > .foo {}"),
      ok(".a { &.b {} }"),
      ok(".a { & .b {} }"),
      ok(".a { &:first-child {} }"),
      ok("a[b=#{c}] { }"),
      bad("a+  a {}")
        .at(1, 2, "Expected single space after \"+\"")
        .fixed("a+ a {}"),
      bad("a+\na {}")
        .at(1, 2, "Expected single space after \"+\"")
        .fixed("a+ a {}"),
      bad("a+a {}")
        .at(1, 2, "Expected single space after \"+\"")
        .fixed("a+ a {}"),
      bad("a>a {}")
        .at(1, 2, "Expected single space after \">\"")
        .fixed("a> a {}"),
      bad("a~a {}")
        .at(1, 2, "Expected single space after \"~\"")
        .fixed("a~ a {}"),
      bad("a + .foo.bar ~a {}")
        .at(1, 14, "Expected single space after \"~\"")
        .fixed("a + .foo.bar ~ a {}"),
      bad("#foo +.foo.bar ~ a {}")
        .at(1, 6, "Expected single space after \"+\"")
        .fixed("#foo + .foo.bar ~ a {}"),
      bad("a >>>a {}")
        .at(1, 3, "Expected single space after \">>>\"")
        .fixed("a >>> a {}"),
      bad("namespace|type#id >.foo {}")
        .at(1, 19, "Expected single space after \">\"")
        .fixed("namespace|type#id > .foo {}"),
      bad("a >a >a {}")
        .at(1, 3, "Expected single space after \">\"")
        .at(1, 6, "Expected single space after \">\"")
        .fixed("a > a > a {}"),
      bad(".a { &>.b {} }")
        .at(1, 7, "Expected single space after \">\"")
        .fixed(".a { &> .b {} }"),
      bad(".a { &+.b {} }")
        .at(1, 7, "Expected single space after \"+\"")
        .fixed(".a { &+ .b {} }"),
      bad("a/*comment*/>/*comment*/a {}")
        .at(1, 13, "Expected single space after \">\"")
        .fixed("a/*comment*/> /*comment*/a {}"),
      bad("a/*comment*/>/*comment*/a, b/*comment*/>/*comment*/b {}")
        .at(1, 13, "Expected single space after \">\"")
        .at(1, 40, "Expected single space after \">\"")
        .fixed("a/*comment*/> /*comment*/a, b/*comment*/> /*comment*/b {}"),
    ],
  );
}

#[test]
fn selector_combinator_space_after_never() {
  check(
    "@stylistic/selector-combinator-space-after",
    json!(["never"]),
    Css,
    &[
      ok("a +a {}"),
      ok("a >a {}"),
      ok("a ~a {}"),
      ok("a >>>a {}"),
      ok(".foo ~a +bar {}"),
      ok("a+a {}"),
      ok("a>a {}"),
      ok("a~a {}"),
      ok("a\n+a {}"),
      ok("a\r\n+a {}"),
      ok("a\n>a {}"),
      ok("a\n~a {}"),
      ok("a\r\n~a {}"),
      ok("a\n>>>a {}"),
      ok("a\r\n>>>a {}"),
      ok(".foo:nth-child(2n + 1) {}"),
      ok(".foo:nth-child(2n - 1) {}"),
      ok("a[rel~='copyright'] {}"),
      ok("a [type='button'] {}"),
      ok("a  a {}"),
      ok("a\na {}"),
      ok("a\r\na {}"),
      ok("a\n\na {}"),
      ok("a\r\n\r\na {}"),
      ok("namespace|type#id >.foo {}"),
      bad("a+ a {}")
        .at(1, 2, "Unexpected whitespace after \"+\"")
        .fixed("a+a {}"),
      bad("a> a {}")
        .at(1, 2, "Unexpected whitespace after \">\"")
        .fixed("a>a {}"),
      bad("a~ a {}")
        .at(1, 2, "Unexpected whitespace after \"~\"")
        .fixed("a~a {}"),
      bad("a+\na{}")
        .at(1, 2, "Unexpected whitespace after \"+\"")
        .fixed("a+a{}"),
      bad("a+\r\na{}")
        .at(1, 2, "Unexpected whitespace after \"+\"")
        .fixed("a+a{}"),
      bad("a>\na{}")
        .at(1, 2, "Unexpected whitespace after \">\"")
        .fixed("a>a{}"),
      bad("a>\r\na{}")
        .at(1, 2, "Unexpected whitespace after \">\"")
        .fixed("a>a{}"),
      bad("a~\na{}")
        .at(1, 2, "Unexpected whitespace after \"~\"")
        .fixed("a~a{}"),
      bad("a~\r\na{}")
        .at(1, 2, "Unexpected whitespace after \"~\"")
        .fixed("a~a{}"),
      bad("a + .foo.bar ~a {}")
        .at(1, 3, "Unexpected whitespace after \"+\"")
        .fixed("a +.foo.bar ~a {}"),
      bad("#foo +.foo.bar ~ a {}")
        .at(1, 16, "Unexpected whitespace after \"~\"")
        .fixed("#foo +.foo.bar ~a {}"),
      bad("a >>> a {}")
        .at(1, 3, "Unexpected whitespace after \">>>\"")
        .fixed("a >>>a {}"),
      bad("a/*comment*/ > /*comment*/a {}")
        .at(1, 14, "Unexpected whitespace after \">\"")
        .fixed("a/*comment*/ >/*comment*/a {}"),
      bad("a/*comment*/ > /*comment*/a, b/*comment*/ > /*comment*/b {}")
        .at(1, 14, "Unexpected whitespace after \">\"")
        .at(1, 43, "Unexpected whitespace after \">\"")
        .fixed("a/*comment*/ >/*comment*/a, b/*comment*/ >/*comment*/b {}"),
    ],
  );
}

#[test]
fn selector_combinator_space_after_always_less() {
  check(
    "@stylistic/selector-combinator-space-after",
    json!(["always"]),
    Less,
    &[
      ok(".a when (@size>=60) and (@size<102) {}"),
      bad("a+  a {}")
        .at(1, 2, "Expected single space after \"+\"")
        .fixed("a+ a {}"),
    ],
  );
}

#[test]
fn selector_combinator_space_after_always_scss() {
  check(
    "@stylistic/selector-combinator-space-after",
    json!(["always"]),
    Scss,
    &[
      ok("a { > /*comment*/a, > /*comment*/.b{} }"),
      ok("a ~, b {}"),
    ],
  );
}

#[test]
fn selector_combinator_space_after_never_scss() {
  check(
    "@stylistic/selector-combinator-space-after",
    json!(["never"]),
    Scss,
    &[
      ok("a { >/*comment*/a, >/*comment*/.b {} }"),
      ok("a ~, b {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_combinator_space_after_always_gale_cases() {
  check(
    "@stylistic/selector-combinator-space-after",
    json!("always"),
    Css,
    &[
      ok("a > b { }"),
      bad("a >b { }")
        .at(1, 3, "Expected single space after \">\"")
        .fixed("a > b { }"),
      bad("a ~b { }")
        .at(1, 3, "Expected single space after \"~\"")
        .fixed("a ~ b { }"),
      bad("a +b { }")
        .at(1, 3, "Expected single space after \"+\"")
        .fixed("a + b { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_combinator_space_after_never_gale_cases() {
  check(
    "@stylistic/selector-combinator-space-after",
    json!("never"),
    Css,
    &[bad("a > b { }")
      .at(1, 3, "Unexpected whitespace after \">\"")
      .fixed("a >b { }")],
  );
}

#[test]
fn selector_combinator_space_before_always() {
  check(
    "@stylistic/selector-combinator-space-before",
    json!(["always"]),
    Css,
    &[
      ok("a + a {}"),
      ok("a > a {}"),
      ok("a ~ a {}"),
      ok("a >>> a {}"),
      ok(".foo ~ a + bar {}"),
      ok("a +a {}"),
      ok("a >a {}"),
      ok("a ~a {}"),
      ok("a >>>a {}"),
      ok("a +\na {}"),
      ok("a +\r\na {}"),
      ok("a >\na {}"),
      ok("a ~\na {}"),
      ok("a ~\r\na {}"),
      ok("a >>>\na {}"),
      ok("a >>>\r\na {}"),
      ok("a ~a +bar {}"),
      ok(".foo:nth-child(2n+1) {}"),
      ok(".foo:nth-child(2n-1) {}"),
      ok("a[rel~='copyright'] {}"),
      ok(".foo {\n\t> span,\n\t> b { color:pink; } }"),
      ok(".foo\\+bar {}"),
      ok("a [type='button'] {}"),
      ok("a  a {}"),
      ok("a\na {}"),
      ok("a\r\na {}"),
      ok("a\n\na {}"),
      ok("a\r\n\r\na {}"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a[b=#{c}] { }"),
      ok("namespace|type#id > .foo {}"),
      ok("namespace|type#id > .foo {}, space|customtype#id_withunder > a {}"),
      bad("a  +a {}")
        .at(1, 4, "Expected single space before \"+\"")
        .fixed("a +a {}"),
      bad("a\n+ a {}")
        .at(2, 1, "Expected single space before \"+\"")
        .fixed("a + a {}"),
      bad("a\r\n+ a {}")
        .at(2, 1, "Expected single space before \"+\"")
        .fixed("a + a {}"),
      bad("a+a {}")
        .at(1, 2, "Expected single space before \"+\"")
        .fixed("a +a {}"),
      bad("a>a {}")
        .at(1, 2, "Expected single space before \">\"")
        .fixed("a >a {}"),
      bad("a~a {}")
        .at(1, 2, "Expected single space before \"~\"")
        .fixed("a ~a {}"),
      bad("a + .foo.bar~ a {}")
        .at(1, 13, "Expected single space before \"~\"")
        .fixed("a + .foo.bar ~ a {}"),
      bad("#foo+ .foo.bar ~ a {}")
        .at(1, 5, "Expected single space before \"+\"")
        .fixed("#foo + .foo.bar ~ a {}"),
      bad("a>>> a {}")
        .at(1, 2, "Expected single space before \">>>\"")
        .fixed("a >>> a {}"),
    ],
  );
}

#[test]
fn selector_combinator_space_before_never() {
  check(
    "@stylistic/selector-combinator-space-before",
    json!(["never"]),
    Css,
    &[
      ok("a+ a {}"),
      ok("a> a {}"),
      ok("a~ a {}"),
      ok("a>>> a {}"),
      ok(".foo~ a+ bar {}"),
      ok("a+a {}"),
      ok("a>a {}"),
      ok("a~a {}"),
      ok("a+\na {}"),
      ok("a>\na {}"),
      ok("a>\r\na {}"),
      ok("a~\na {}"),
      ok("a>>>\na {}"),
      ok("a>>>\r\na {}"),
      ok(".foo:nth-child(2n + 1) {}"),
      ok(".foo:nth-child(2n - 1) {}"),
      ok("a[rel~='copyright'] {}"),
      ok("a [type='button'] {}"),
      ok("a  a {}"),
      ok("a\na {}"),
      ok("a\r\na {}"),
      ok("a\n\na {}"),
      ok("a\r\n\r\na {}"),
      ok("namespace|type#id> .foo {}"),
      bad("a +a {}")
        .at(1, 3, "Unexpected whitespace before \"+\"")
        .fixed("a+a {}"),
      bad("a >a {}")
        .at(1, 3, "Unexpected whitespace before \">\"")
        .fixed("a>a {}"),
      bad("a ~a {}")
        .at(1, 3, "Unexpected whitespace before \"~\"")
        .fixed("a~a {}"),
      bad("a\n+a {}")
        .at(2, 1, "Unexpected whitespace before \"+\"")
        .fixed("a+a {}"),
      bad("a\r\n+a {}")
        .at(2, 1, "Unexpected whitespace before \"+\"")
        .fixed("a+a {}"),
      bad("a\n>a {}")
        .at(2, 1, "Unexpected whitespace before \">\"")
        .fixed("a>a {}"),
      bad("a\r\n>a {}")
        .at(2, 1, "Unexpected whitespace before \">\"")
        .fixed("a>a {}"),
      bad("a\n~a {}")
        .at(2, 1, "Unexpected whitespace before \"~\"")
        .fixed("a~a {}"),
      bad("a\r\n~a {}")
        .at(2, 1, "Unexpected whitespace before \"~\"")
        .fixed("a~a {}"),
      bad("a + .foo.bar~ a {}")
        .at(1, 3, "Unexpected whitespace before \"+\"")
        .fixed("a+ .foo.bar~ a {}"),
      bad("#foo+ .foo.bar ~ a {}")
        .at(1, 16, "Unexpected whitespace before \"~\"")
        .fixed("#foo+ .foo.bar~ a {}"),
      bad("a >>> a {}")
        .at(1, 3, "Unexpected whitespace before \">>>\"")
        .fixed("a>>> a {}"),
    ],
  );
}

#[test]
fn selector_combinator_space_before_always_less() {
  check(
    "@stylistic/selector-combinator-space-before",
    json!(["always"]),
    Less,
    &[
      ok(".a when (@size>=60) and (@size<102) {}"),
      bad("a  +a {}")
        .at(1, 4, "Expected single space before \"+\"")
        .fixed("a +a {}"),
    ],
  );
}

#[test]
fn selector_combinator_space_before_always_scss() {
  check(
    "@stylistic/selector-combinator-space-before",
    json!(["always"]),
    Scss,
    &[
      ok("a {> a {}}"),
      ok("a {> a,> .b {}}"),
      ok("a {> /*comment*/ a,> /*comment*/ .b {}}"),
      bad("a {> /*comment*/ a,> /*comment*/ .b> .c {}}")
        .at(1, 36, "Expected single space before \">\"")
        .fixed("a {> /*comment*/ a,> /*comment*/ .b > .c {}}"),
    ],
  );
}

#[test]
fn selector_combinator_space_before_never_scss() {
  check(
    "@stylistic/selector-combinator-space-before",
    json!(["never"]),
    Scss,
    &[
      ok("a { > a {}}"),
      ok("a { > a, > .b {}}"),
      ok("a { > /*comment*/ a, > /*commenttest*/ .b {}}"),
      bad("a { > /*comment*/ a, > /*comment*/ .b >.c {}}")
        .at(1, 39, "Unexpected whitespace before \">\"")
        .fixed("a { > /*comment*/ a, > /*comment*/ .b>.c {}}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_combinator_space_before_always_gale_cases() {
  check(
    "@stylistic/selector-combinator-space-before",
    json!("always"),
    Css,
    &[
      ok("a > b { }"),
      bad("a> b { }")
        .at(1, 2, "Expected single space before \">\"")
        .fixed("a > b { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_combinator_space_before_never_gale_cases() {
  check(
    "@stylistic/selector-combinator-space-before",
    json!("never"),
    Css,
    &[bad("a > b { }")
      .at(1, 3, "Unexpected whitespace before \">\"")
      .fixed("a> b { }")],
  );
}
