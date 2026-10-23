//! The `inside` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn function_parentheses_space_inside_always() {
  let expected_closing = "Expected single space before \")\"";
  let expected_opening = "Expected single space after \"(\"";
  check(
    "@stylistic/function-parentheses-space-inside",
    json!(["always"]),
    Css,
    &[
      ok("a { filter: grayscale(); }"),
      ok("a { filter: grayscale( ); }"),
      ok("a::before { content: \"(a) ( a )\"; }"),
      ok("a::before { background: url( 'asdf(Vcxvsd)ASD' ); }"),
      ok("a { transform: translate( 1, 1 ); }"),
      ok("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      ok("$map: (key: value, key2: value2)"),
      ok("$list: (value, value2)"),
      bad("a { transform: translate(1, 1 ); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { transform: translate( 1, 1); }")
        .at(1, 30, expected_closing)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { transform: translate(  1, 1 ); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { transform: translate( 1, 1  ); }")
        .at(1, 32, expected_closing)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { color: color(rgb( 0, 0, 0 ) lightness( 50% ) ); }")
        .at(1, 18, expected_opening)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb(0, 0, 0 ) lightness( 50% ) ); }")
        .at(1, 23, expected_opening)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0) lightness( 50% ) ); }")
        .at(1, 30, expected_closing)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0 ) lightness(50% ) ); }")
        .at(1, 44, expected_opening)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0 ) lightness( 50%) ); }")
        .at(1, 47, expected_closing)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0 ) lightness( 50% )); }")
        .at(1, 49, expected_closing)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a::before { content: attr(data-foo ); }")
        .at(1, 27, expected_opening)
        .fixed("a::before { content: attr( data-foo ); }"),
      bad("a::before { content: attr( data-foo); }")
        .at(1, 35, expected_closing)
        .fixed("a::before { content: attr( data-foo ); }"),
      bad("a { transform: translate(\n  1,\n  1 ); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate( 1,\n  1 ); }"),
      bad("a { transform: translate( 1,\n  1\n\t); }")
        .at(3, 1, expected_closing)
        .fixed("a { transform: translate( 1,\n  1 ); }"),
      bad("a { transform: translate(1,\r\n1 ); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate( 1,\r\n1 ); }"),
      bad("a { transform: translate(/*comment*/1, 1/*comment*/); }")
        .at(1, 26, expected_opening)
        .at(1, 51, expected_closing)
        .fixed("a { transform: translate( /*comment*/1, 1/*comment*/ ); }"),
    ],
  );
}

#[test]
fn function_parentheses_space_inside_always_single_line() {
  let expected_closing_single_line = "Expected single space before \")\" in a single-line function";
  let expected_opening_single_line = "Expected single space after \"(\" in a single-line function";
  check(
    "@stylistic/function-parentheses-space-inside",
    json!(["always-single-line"]),
    Css,
    &[
      ok("a { filter: grayscale(); }"),
      ok("a { filter: grayscale( ); }"),
      ok("a::before { content: \"(a) ( a )\"; }"),
      ok("a::before { background: url( 'asdf(Vcxvsd)ASD' ); }"),
      ok("a { transform: translate( 1, 1 ); }"),
      ok("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      ok("a { transform: translate(\n  1,\n  1\n); }"),
      ok("a { transform: translate(  \n  1,\n  1\n\t); }"),
      ok("a { transform: translate(1,\r\n1); }"),
      ok("a { color: color(rgb(0,\n0,\n0 ) lightness( 50% )); }"),
      ok("$map: (key: value, key2: value2)"),
      bad("a { color: color(rgb(0,\n0,\n0 ) lightness(50% )); }")
        .at(3, 15, expected_opening_single_line)
        .fixed("a { color: color(rgb(0,\n0,\n0 ) lightness( 50% )); }"),
      bad("a { transform: translate(1, 1 ); }")
        .at(1, 26, expected_opening_single_line)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { transform: translate( 1, 1); }")
        .at(1, 30, expected_closing_single_line)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { transform: translate(  1, 1 ); }")
        .at(1, 26, expected_opening_single_line)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { transform: translate( 1, 1  ); }")
        .at(1, 32, expected_closing_single_line)
        .fixed("a { transform: translate( 1, 1 ); }"),
      bad("a { color: color(rgb( 0, 0, 0 ) lightness( 50% ) ); }")
        .at(1, 18, expected_opening_single_line)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb(0, 0, 0 ) lightness( 50% ) ); }")
        .at(1, 23, expected_opening_single_line)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0) lightness( 50% ) ); }")
        .at(1, 30, expected_closing_single_line)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0 ) lightness(50% ) ); }")
        .at(1, 44, expected_opening_single_line)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0 ) lightness( 50%) ); }")
        .at(1, 47, expected_closing_single_line)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a { color: color( rgb( 0, 0, 0 ) lightness( 50% )); }")
        .at(1, 49, expected_closing_single_line)
        .fixed("a { color: color( rgb( 0, 0, 0 ) lightness( 50% ) ); }"),
      bad("a::before { content: attr(data-foo ); }")
        .at(1, 27, expected_opening_single_line)
        .fixed("a::before { content: attr( data-foo ); }"),
      bad("a::before { content: attr( data-foo); }")
        .at(1, 35, expected_closing_single_line)
        .fixed("a::before { content: attr( data-foo ); }"),
    ],
  );
}

#[test]
fn function_parentheses_space_inside_never() {
  let rejected_closing = "Unexpected whitespace before \")\"";
  let rejected_opening = "Unexpected whitespace after \"(\"";
  check(
    "@stylistic/function-parentheses-space-inside",
    json!(["never"]),
    Css,
    &[
      ok("a { filter: grayscale(); }"),
      ok("a { filter: grayscale( ); }"),
      ok("a::before { content: \"(a) ( a )\"; }"),
      ok("a::before { background: url('asdf( Vcxvsd )ASD'); }"),
      ok("a::before { content: \"(a) ( a )\"; }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      ok("$map: ( key: value, key2: value2 )"),
      bad("a { transform: translate( 1, 1); }")
        .at(1, 26, rejected_opening)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(  1, 1); }")
        .at(1, 26, rejected_opening)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1, 1 ); }")
        .at(1, 30, rejected_closing)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1, 1  ); }")
        .at(1, 31, rejected_closing)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { color: color( rgb(0, 0, 0) lightness(50%)); }")
        .at(1, 18, rejected_opening)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb( 0, 0, 0) lightness(50%)); }")
        .at(1, 22, rejected_opening)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0 ) lightness(50%)); }")
        .at(1, 29, rejected_closing)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0) lightness( 50%)); }")
        .at(1, 41, rejected_opening)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0) lightness(50% )); }")
        .at(1, 44, rejected_closing)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0) lightness(50%) ); }")
        .at(1, 45, rejected_closing)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a::before { content: attr(data-foo ); }")
        .at(1, 35, rejected_closing)
        .fixed("a::before { content: attr(data-foo); }"),
      bad("a::before { content: attr( data-foo); }")
        .at(1, 27, rejected_opening)
        .fixed("a::before { content: attr(data-foo); }"),
      bad("a { transform: translate( 1,\n1); }")
        .at(1, 26, rejected_opening)
        .fixed("a { transform: translate(1,\n1); }"),
      bad("a { transform: translate(1,\r\n  1\r\n); }")
        .at(2, 5, rejected_closing)
        .fixed("a { transform: translate(1,\r\n  1); }"),
      bad("a { color: color(rgb(0,\n0,\n0 ) lightness(50%)); }")
        .at(3, 2, rejected_closing)
        .fixed("a { color: color(rgb(0,\n0,\n0) lightness(50%)); }"),
      bad("a { transform: translate( /*comment*/ 1, 1 /*comment*/ ); }")
        .at(1, 26, rejected_opening)
        .at(1, 55, rejected_closing)
        .fixed("a { transform: translate(/*comment*/ 1, 1 /*comment*/); }"),
    ],
  );
}

#[test]
fn function_parentheses_space_inside_never_single_line() {
  let rejected_closing_single_line = "Unexpected whitespace before \")\" in a single-line function";
  let rejected_opening_single_line = "Unexpected whitespace after \"(\" in a single-line function";
  check(
    "@stylistic/function-parentheses-space-inside",
    json!(["never-single-line"]),
    Css,
    &[
      ok("a { filter: grayscale(); }"),
      ok("a { filter: grayscale( ); }"),
      ok("a::before { content: \"(a) ( a )\"; }"),
      ok("a::before { background: url('asdf( Vcxvsd )ASD'); }"),
      ok("a::before { content: \"(a) ( a )\"; }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      ok("a { transform: translate( 1,\n1 ); }"),
      ok("a { transform: translate(\r\n  1,\r\n  1\r\n); }"),
      ok("a { color: color(rgb(0,\n0,\n0 ) lightness(50%)); }"),
      ok("$map: ( key: value, key2: value2 )"),
      bad("a { color: color(rgb(0,\n0,\n0) lightness( 50%)); }")
        .at(3, 14, rejected_opening_single_line)
        .fixed("a { color: color(rgb(0,\n0,\n0) lightness(50%)); }"),
      bad("a { transform: translate( 1, 1); }")
        .at(1, 26, rejected_opening_single_line)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(  1, 1); }")
        .at(1, 26, rejected_opening_single_line)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1, 1 ); }")
        .at(1, 30, rejected_closing_single_line)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { transform: translate(1, 1  ); }")
        .at(1, 31, rejected_closing_single_line)
        .fixed("a { transform: translate(1, 1); }"),
      bad("a { color: color( rgb(0, 0, 0) lightness(50%)); }")
        .at(1, 18, rejected_opening_single_line)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb( 0, 0, 0) lightness(50%)); }")
        .at(1, 22, rejected_opening_single_line)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0 ) lightness(50%)); }")
        .at(1, 29, rejected_closing_single_line)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0) lightness( 50%)); }")
        .at(1, 41, rejected_opening_single_line)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0) lightness(50% )); }")
        .at(1, 44, rejected_closing_single_line)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0) lightness(50%) ); }")
        .at(1, 45, rejected_closing_single_line)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a::before { content: attr(data-foo ); }")
        .at(1, 35, rejected_closing_single_line)
        .fixed("a::before { content: attr(data-foo); }"),
      bad("a::before { content: attr( data-foo); }")
        .at(1, 27, rejected_opening_single_line)
        .fixed("a::before { content: attr(data-foo); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_parentheses_space_inside_never_gale_cases() {
  check(
    "@stylistic/function-parentheses-space-inside",
    json!("never"),
    Css,
    &[
      ok("a { transform: translate(1px, 2px); }"),
      bad("a { transform: translate( 1px, 2px ); }")
        .at(1, 26, "Unexpected whitespace after \"(\"")
        .at(1, 35, "Unexpected whitespace before \")\"")
        .fixed("a { transform: translate(1px, 2px); }"),
      ok("a:not(.foo) { color: red; }"),
      ok("a { @include mixin( $a ); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_parentheses_space_inside_always_gale_cases() {
  check(
    "@stylistic/function-parentheses-space-inside",
    json!("always"),
    Css,
    &[
      ok("a { transform: translate( 1px, 2px ); }"),
      bad("a { transform: translate(1px, 2px); }")
        .at(1, 26, "Expected single space after \"(\"")
        .at(1, 33, "Expected single space before \")\"")
        .fixed("a { transform: translate( 1px, 2px ); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_parentheses_space_inside_never_scss_gale_cases() {
  check(
    "@stylistic/function-parentheses-space-inside",
    json!("never"),
    Scss,
    &[ok("a { color: #{fn( $a )}; }")],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_parentheses_space_inside_never_single_line_gale_cases() {
  check(
    "@stylistic/function-parentheses-space-inside",
    json!("never-single-line"),
    Css,
    &[
      bad("a { width: calc( 1px + 2px ); }")
        .at(
          1,
          17,
          "Unexpected whitespace after \"(\" in a single-line function",
        )
        .at(
          1,
          27,
          "Unexpected whitespace before \")\" in a single-line function",
        )
        .fixed("a { width: calc(1px + 2px); }"),
      ok("a { width: calc( 1px +\n 2px ); }"),
    ],
  );
}

#[test]
fn function_parentheses_newline_inside_always() {
  let expected_closing = "Expected newline before \")\"";
  let expected_opening = "Expected newline after \"(\"";
  check(
    "@stylistic/function-parentheses-newline-inside",
    json!(["always"]),
    Css,
    &[
      ok("a::before { content: \"(a) ( a)\"; }"),
      ok("a::before { background: url(\n'asdf( Vcxvsd)ASD'\n); }"),
      ok("a { transform: translate(\n1, 1\n); }"),
      ok("a { transform: translate(\n\n1, 1\n\n); }"),
      ok("a { transform: translate(\r\n1, 1\r\n); }"),
      ok("a { transform: translate(\r\n\r\n1, 1\r\n\r\n); }"),
      ok("a { color: color(\nrgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }"),
      ok("$map: (key: value,key2: value2)"),
      ok("$list: (value, value2)"),
      ok("a { transform: translate( /*comment*/\n1,\n  1\n); }"),
      bad("a { transform: translate(1, 1\n); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate(\n1, 1\n); }"),
      bad("a { transform: translate(\n1, 1); }")
        .at(2, 4, expected_closing)
        .fixed("a { transform: translate(\n1, 1\n); }"),
      bad("a { transform: translate(  1, 1\n); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate(\n  1, 1\n); }"),
      bad("a { transform: translate(\n1,\n1\t); }")
        .at(3, 2, expected_closing)
        .fixed("a { transform: translate(\n1,\n1\n\t); }"),
      bad("a { color: color(rgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }")
        .at(1, 18, expected_opening)
        .fixed("a { color: color(\nrgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }"),
      bad("a { color: color(\nrgb(\n0, 0, 0\n) lightness(50%\n)\n); }")
        .at(4, 13, expected_opening)
        .fixed("a { color: color(\nrgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }"),
      bad("a::before { content: attr(data-foo\n); }")
        .at(1, 27, expected_opening)
        .fixed("a::before { content: attr(\ndata-foo\n); }"),
      bad("a::before { content: attr(\n\tdata-foo); }")
        .at(2, 9, expected_closing)
        .fixed("a::before { content: attr(\n\tdata-foo\n); }"),
      bad("a { transform: translate(  1,\n  1\n); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate(\n  1,\n  1\n); }"),
      bad("a { transform: translate(1,\r\n1\n); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate(\r\n1,\r\n1\n); }"),
      bad("a { transform: translate( /*comment*/ 1,\n  1\n); }")
        .at(1, 26, expected_opening)
        .fixed("a { transform: translate( /*comment*/\n 1,\n  1\n); }"),
      bad("a { transform: translate( /*c1*/ /*c2*/ 1,1 /*c3*/ /*c4*/ ); }")
        .at(1, 26, expected_opening)
        .at(1, 58, expected_closing)
        .fixed("a { transform: translate( /*c1*/ /*c2*/\n 1,1 /*c3*/ /*c4*/\n ); }"),
    ],
  );
}

#[test]
fn function_parentheses_newline_inside_always_multi_line() {
  let expected_closing_multi_line = "Expected newline before \")\" in a multi-line function";
  let expected_opening_multi_line = "Expected newline after \"(\" in a multi-line function";
  check(
    "@stylistic/function-parentheses-newline-inside",
    json!(["always-multi-line"]),
    Css,
    &[
      ok("a::before { content: \"(a) ( a)\"; }"),
      ok("a::before { background: url(\n'asdf( Vcxvsd)ASD'\n); }"),
      ok("a { transform: translate(\n1, 1\n); }"),
      ok("a { transform: translate(\r\n1, 1\r\n); }"),
      ok("a { color: color(\nrgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { transform: translate(  1, 1\t); }"),
      ok("$map: (key: value,\nkey2: value2)"),
      ok("a { transform: translate(\n/*comment*/ 1,1\n); }"),
      ok("a { transform: translate( /*comment*/\n1,1\n); }"),
      ok("a { transform: translate(\n1,1\n/*comment*/ ); }"),
      ok("a { transform: translate(\n1,1 /*comment*/\n); }"),
      bad("a { transform: translate(1, 1\n); }")
        .at(1, 26, expected_opening_multi_line)
        .fixed("a { transform: translate(\n1, 1\n); }"),
      bad("a { transform: translate(\n1, 1); }")
        .at(2, 4, expected_closing_multi_line)
        .fixed("a { transform: translate(\n1, 1\n); }"),
      bad("a { transform: translate(  1, 1\n); }")
        .at(1, 26, expected_opening_multi_line)
        .fixed("a { transform: translate(\n  1, 1\n); }"),
      bad("a { transform: translate(\n1,\n1\t); }")
        .at(3, 2, expected_closing_multi_line)
        .fixed("a { transform: translate(\n1,\n1\n\t); }"),
      bad("a { color: color(rgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }")
        .at(1, 18, expected_opening_multi_line)
        .fixed("a { color: color(\nrgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }"),
      bad("a { color: color(\nrgb(\n0, 0, 0\n) lightness(50%\n)\n); }")
        .at(4, 13, expected_opening_multi_line)
        .fixed("a { color: color(\nrgb(\n0, 0, 0\n) lightness(\n50%\n)\n); }"),
      bad("a::before { content: attr(data-foo\n); }")
        .at(1, 27, expected_opening_multi_line)
        .fixed("a::before { content: attr(\ndata-foo\n); }"),
      bad("a::before { content: attr(\n\tdata-foo); }")
        .at(2, 9, expected_closing_multi_line)
        .fixed("a::before { content: attr(\n\tdata-foo\n); }"),
      bad("a { transform: translate(  1,\n  1\n); }")
        .at(1, 26, expected_opening_multi_line)
        .fixed("a { transform: translate(\n  1,\n  1\n); }"),
      bad("a { transform: translate(1,\r\n1\n); }")
        .at(1, 26, expected_opening_multi_line)
        .fixed("a { transform: translate(\r\n1,\r\n1\n); }"),
    ],
  );
}

#[test]
fn function_parentheses_newline_inside_never_multi_line() {
  let rejected_closing_multi_line = "Unexpected whitespace before \")\" in a multi-line function";
  let rejected_opening_multi_line = "Unexpected whitespace after \"(\" in a multi-line function";
  check(
    "@stylistic/function-parentheses-newline-inside",
    json!(["never-multi-line"]),
    Css,
    &[
      ok("a::before { content: \"(a) ( a)\"; }"),
      ok("a::before { background: url('asdf(Vcxv\nsd\n)ASD'); }"),
      ok("a { transform: translate(1, 1); }"),
      ok("a { transform: translate(1,\r\n1); }"),
      ok("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      ok("a { transform: translate(1,\n  1); }"),
      ok("a { transform: translate(1,\n\t\t1); }"),
      ok("$map: (\nkey: value,\nkey2: value2\n)"),
      ok("a { transform: translate(/*comment*/1,\n1); }"),
      ok("a { transform: translate(1,\n1/*comment*/); }"),
      bad("a { transform: translate(\n  1,\n  1); }")
        .at(1, 26, rejected_opening_multi_line)
        .fixed("a { transform: translate(1,\n  1); }"),
      bad("a { transform: translate(  \n  1,\n  1); }")
        .at(1, 26, rejected_opening_multi_line)
        .fixed("a { transform: translate(1,\n  1); }"),
      bad("a { transform: translate(1,\n  1\n); }")
        .at(2, 4, rejected_closing_multi_line)
        .fixed("a { transform: translate(1,\n  1); }"),
      bad("a { transform: translate(1,\r\n1\t); }")
        .at(2, 2, rejected_closing_multi_line)
        .fixed("a { transform: translate(1,\r\n1); }"),
      bad("a { color: color(rgb(0,\r\n  0,\r\n  0\r\n) lightness(50%)); }")
        .at(3, 5, rejected_closing_multi_line)
        .fixed("a { color: color(rgb(0,\r\n  0,\r\n  0) lightness(50%)); }"),
      bad("a { color: color(rgb(0, 0, 0) lightness(\n50%)); }")
        .at(1, 41, rejected_opening_multi_line)
        .fixed("a { color: color(rgb(0, 0, 0) lightness(50%)); }"),
      bad("a { transform: translate( /*c1*/ /*c2*/ 1,\n1 /*c3*/ /*c4*/ ); }")
        .at(1, 26, rejected_opening_multi_line)
        .at(2, 16, rejected_closing_multi_line)
        .fixed("a { transform: translate(/*c1*//*c2*/1,\n1/*c3*//*c4*/); }"),
      bad("a { transform: translate( /*c1*//*c2*/1,\n1/*c3*//*c4*/ ); }")
        .at(1, 26, rejected_opening_multi_line)
        .at(2, 14, rejected_closing_multi_line)
        .fixed("a { transform: translate(/*c1*//*c2*/1,\n1/*c3*//*c4*/); }"),
      bad("a { transform: translate(/*c1*//*c2*/ 1,\n1 /*c3*//*c4*/); }")
        .at(1, 26, rejected_opening_multi_line)
        .at(2, 14, rejected_closing_multi_line)
        .fixed("a { transform: translate(/*c1*//*c2*/1,\n1/*c3*//*c4*/); }"),
      bad("a { transform: translate(/*c1*/ /*c2*/1,\n1/*c3*/ /*c4*/); }")
        .at(1, 26, rejected_opening_multi_line)
        .at(2, 14, rejected_closing_multi_line)
        .fixed("a { transform: translate(/*c1*//*c2*/1,\n1/*c3*//*c4*/); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_parentheses_newline_inside_always_multi_line_gale_cases() {
  check(
    "@stylistic/function-parentheses-newline-inside",
    json!("always-multi-line"),
    Css,
    &[
      ok("a { background: url(\n  \"foo.png\"\n); }"),
      ok("got: {:?}"),
      bad("a { background: url(\"foo.png\"\n); }")
        .at(
          1,
          21,
          "Expected newline after \"(\" in a multi-line function",
        )
        .fixed("a { background: url(\n\"foo.png\"\n); }"),
      bad("a { background: url(\n  \"foo.png\"); }")
        .at(
          2,
          11,
          "Expected newline before \")\" in a multi-line function",
        )
        .fixed("a { background: url(\n  \"foo.png\"\n); }"),
      ok("a { background: url(\"foo.png\"); }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn function_parentheses_newline_inside_never_multi_line_gale_cases() {
  check(
    "@stylistic/function-parentheses-newline-inside",
    json!("never-multi-line"),
    Css,
    &[
      bad("a { transform: translate(\n  1px,\n  2px); }")
        .at(
          1,
          26,
          "Unexpected whitespace after \"(\" in a multi-line function",
        )
        .fixed("a { transform: translate(1px,\n  2px); }"),
      ok("a { transform: translate(1px,\n  2px); }"),
      ok("got: {:?}"),
    ],
  );
}

#[test]
fn media_feature_parentheses_space_inside_always() {
  let expected_closing = "Expected single space before \")\"";
  let expected_opening = "Expected single space after \"(\"";
  check(
    "@stylistic/media-feature-parentheses-space-inside",
    json!(["always"]),
    Css,
    &[
      ok("@media ( max-width: 300px ) {}"),
      ok("@mEdIa ( max-width: 300px ) {}"),
      ok("@MEDIA ( max-width: 300px ) {}"),
      ok("@media screen and ( color ), projection and ( color ) {}"),
      ok("@media ( grid ) and ( max-width: 15em ) {}"),
      ok("@media ( max-width: /*comment*/ ) {}"),
      bad("@media (max-width: 300px ) {}")
        .at(1, 9, expected_opening)
        .fixed("@media ( max-width: 300px ) {}"),
      bad("@mEdIa (max-width: 300px ) {}")
        .at(1, 9, expected_opening)
        .fixed("@mEdIa ( max-width: 300px ) {}"),
      bad("@MEDIA (max-width: /*comment*/ ) {}")
        .at(1, 9, expected_opening)
        .fixed("@MEDIA ( max-width: /*comment*/ ) {}"),
      bad("@MEDIA (max-width: 300px ) {}")
        .at(1, 9, expected_opening)
        .fixed("@MEDIA ( max-width: 300px ) {}"),
      bad("@media ( max-width: 300px) {}")
        .at(1, 25, expected_closing)
        .fixed("@media ( max-width: 300px ) {}"),
      bad("@media ( max-width: /*comment*/) {}")
        .at(1, 31, expected_closing)
        .fixed("@media ( max-width: /*comment*/ ) {}"),
      bad("@media screen and (color ), projection and ( color ) {}")
        .at(1, 20, expected_opening)
        .fixed("@media screen and ( color ), projection and ( color ) {}"),
      bad("@media screen and ( color), projection and ( color ) {}")
        .at(1, 25, expected_closing)
        .fixed("@media screen and ( color ), projection and ( color ) {}"),
      bad("@media screen and ( color ), projection and (color ) {}")
        .at(1, 46, expected_opening)
        .fixed("@media screen and ( color ), projection and ( color ) {}"),
      bad("@media screen and ( color ), projection and ( color) {}")
        .at(1, 51, expected_closing)
        .fixed("@media screen and ( color ), projection and ( color ) {}"),
      bad("@media ( grid ) and (max-width: 15em ) {}")
        .at(1, 22, expected_opening)
        .fixed("@media ( grid ) and ( max-width: 15em ) {}"),
    ],
  );
}

#[test]
fn media_feature_parentheses_space_inside_never() {
  let rejected_closing = "Unexpected whitespace before \")\"";
  let rejected_opening = "Unexpected whitespace after \"(\"";
  check(
    "@stylistic/media-feature-parentheses-space-inside",
    json!(["never"]),
    Css,
    &[
      ok("@media (max-width: 300px) {}"),
      ok("@mEdIa (max-width: 300px) {}"),
      ok("@MEDIA (max-width: 300px) {}"),
      ok("@MEDIA (max-width: /*comment*/) {}"),
      ok("@media screen and (color), projection and (color) {}"),
      ok("@media (grid) and (max-width: 15em) {}"),
      bad("@media (  min-width: 700px) {}")
        .at(1, 9, rejected_opening)
        .fixed("@media (min-width: 700px) {}"),
      bad("@media (min-width: 700px  ) {}")
        .at(1, 26, rejected_closing)
        .fixed("@media (min-width: 700px) {}"),
      bad("@media (\t  min-width: 700px) {}")
        .at(1, 9, rejected_opening)
        .fixed("@media (min-width: 700px) {}"),
      bad("@media (min-width: 700px\t) {}")
        .at(1, 25, rejected_closing)
        .fixed("@media (min-width: 700px) {}"),
      bad("@media (max-width: 300px ) {}")
        .at(1, 25, rejected_closing)
        .fixed("@media (max-width: 300px) {}"),
      bad("@mEdIa (max-width: 300px ) {}")
        .at(1, 25, rejected_closing)
        .fixed("@mEdIa (max-width: 300px) {}"),
      bad("@MEDIA (max-width: /*comment*/ ) {}")
        .at(1, 31, rejected_closing)
        .fixed("@MEDIA (max-width: /*comment*/) {}"),
      bad("@MEDIA (max-width: 300px ) {}")
        .at(1, 25, rejected_closing)
        .fixed("@MEDIA (max-width: 300px) {}"),
      bad("@media ( max-width: 300px) {}")
        .at(1, 9, rejected_opening)
        .fixed("@media (max-width: 300px) {}"),
      bad("@media ( max-width: /*comment*/) {}")
        .at(1, 9, rejected_opening)
        .fixed("@media (max-width: /*comment*/) {}"),
      bad("@media screen and (color ), projection and (color) {}")
        .at(1, 25, rejected_closing)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and ( color), projection and (color) {}")
        .at(1, 20, rejected_opening)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color), projection and (color ) {}")
        .at(1, 49, rejected_closing)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media screen and (color), projection and ( color) {}")
        .at(1, 44, rejected_opening)
        .fixed("@media screen and (color), projection and (color) {}"),
      bad("@media (grid) and (max-width: 15em ) {}")
        .at(1, 35, rejected_closing)
        .fixed("@media (grid) and (max-width: 15em) {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_parentheses_space_inside_never_gale_cases() {
  check(
    "@stylistic/media-feature-parentheses-space-inside",
    json!("never"),
    Css,
    &[
      ok("@media (min-width: 600px) { }"),
      bad("@media ( min-width: 600px ) { }")
        .at(1, 9, "Unexpected whitespace after \"(\"")
        .at(1, 26, "Unexpected whitespace before \")\"")
        .fixed("@media (min-width: 600px) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_parentheses_space_inside_always_gale_cases() {
  check(
    "@stylistic/media-feature-parentheses-space-inside",
    json!("always"),
    Css,
    &[
      ok("@media ( min-width: 600px ) { }"),
      bad("@media (min-width: 600px) { }")
        .at(1, 9, "Expected single space after \"(\"")
        .at(1, 24, "Expected single space before \")\"")
        .fixed("@media ( min-width: 600px ) { }"),
    ],
  );
}

#[test]
fn selector_attribute_brackets_space_inside_always() {
  let expected_closing = "Expected single space before \"]\"";
  let expected_opening = "Expected single space after \"[\"";
  check(
    "@stylistic/selector-attribute-brackets-space-inside",
    json!(["always"]),
    Css,
    &[
      ok(".foo { }"),
      ok("[ target ] { }"),
      ok("[  target  ] { }"),
      ok("[ target=_blank ] { }"),
      ok("[  target=_blank  ] { }"),
      ok("[ target='_blank' ] { }"),
      ok("[ target=\"_blank\" ] { }"),
      ok("[ title~=flower ] { }"),
      ok("[ title|=flower ] { }"),
      ok("[ title^=flower ] { }"),
      ok("[ title$=flower ] { }"),
      ok("a[ href*=w3schools ] { }"),
      ok("img[ alt~=person ][ src*=lorem ] { }"),
      ok("[ target=_blank ]:hover { }"),
      ok("[ target=_blank ]::before { }"),
      ok("option[ data-hidden=true ] { }"),
      ok("option[ dataHidden ] { }"),
      ok("@media screen and (max-width: 480px) { img[ align=right ] {} }"),
      ok("[ target=_blank ] .foo { }"),
      ok("li[ aria-hidden=false ]:nth-child(1) { }"),
      ok("li[ aria-hidden=false ]:nth-child( 1) { }"),
      ok("li[ aria-hidden=false ]:nth-child(1 ) { }"),
      ok("li[ aria-hidden=false ]:nth-child( 1 ) { }"),
      ok("ul li[ aria-hidden=false ] + li[ aria-hidden=false ] a { }"),
      ok("[ target ] { content: \"[\" }"),
      ok("[ target ] { content: \"]\" }"),
      ok("[ foo=bar i ] { }"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a[b=#{c}] { }"),
      bad("[target ] { }")
        .at(1, 2, expected_opening)
        .fixed("[ target ] { }"),
      bad("[ target] { }")
        .at(1, 8, expected_closing)
        .fixed("[ target ] { }"),
      bad("[target  ] { }")
        .at(1, 2, expected_opening)
        .fixed("[ target  ] { }"),
      bad("[  target] { }")
        .at(1, 9, expected_closing)
        .fixed("[  target ] { }"),
      bad("[target=_blank ] { }")
        .at(1, 2, expected_opening)
        .fixed("[ target=_blank ] { }"),
      bad("[ target=_blank] { }")
        .at(1, 15, expected_closing)
        .fixed("[ target=_blank ] { }"),
      bad("[target=_blank  ] { }")
        .at(1, 2, expected_opening)
        .fixed("[ target=_blank  ] { }"),
      bad("[  target=_blank] { }")
        .at(1, 16, expected_closing)
        .fixed("[  target=_blank ] { }"),
      bad("[target='_blank' ] { }")
        .at(1, 2, expected_opening)
        .fixed("[ target='_blank' ] { }"),
      bad("[ target='_blank'] { }")
        .at(1, 17, expected_closing)
        .fixed("[ target='_blank' ] { }"),
      bad("[target=\"_blank\" ] { }")
        .at(1, 2, expected_opening)
        .fixed("[ target=\"_blank\" ] { }"),
      bad("[ target=\"_blank\"] { }")
        .at(1, 17, expected_closing)
        .fixed("[ target=\"_blank\" ] { }"),
      bad("a[href*=w3schools ] { }")
        .at(1, 3, expected_opening)
        .fixed("a[ href*=w3schools ] { }"),
      bad("a[ href*=w3schools] { }")
        .at(1, 18, expected_closing)
        .fixed("a[ href*=w3schools ] { }"),
      bad("img[alt~=person ][ src*=lorem ] { }")
        .at(1, 5, expected_opening)
        .fixed("img[ alt~=person ][ src*=lorem ] { }"),
      bad("img[ alt~=person][ src*=lorem ] { }")
        .at(1, 16, expected_closing)
        .fixed("img[ alt~=person ][ src*=lorem ] { }"),
      bad("img[ alt~=person ][src*=lorem ] { }")
        .at(1, 20, expected_opening)
        .fixed("img[ alt~=person ][ src*=lorem ] { }"),
      bad("img[ alt~=person ][ src*=lorem] { }")
        .at(1, 30, expected_closing)
        .fixed("img[ alt~=person ][ src*=lorem ] { }"),
      bad("[target=_blank ]:hover { }")
        .at(1, 2, expected_opening)
        .fixed("[ target=_blank ]:hover { }"),
      bad("[ target=_blank]:hover { }")
        .at(1, 15, expected_closing)
        .fixed("[ target=_blank ]:hover { }"),
      bad("[target=_blank ]::before { }")
        .at(1, 2, expected_opening)
        .fixed("[ target=_blank ]::before { }"),
      bad("[ target=_blank]::before { }")
        .at(1, 15, expected_closing)
        .fixed("[ target=_blank ]::before { }"),
      bad("option[data-hidden=true ] { }")
        .at(1, 8, expected_opening)
        .fixed("option[ data-hidden=true ] { }"),
      bad("option[ data-hidden=true] { }")
        .at(1, 24, expected_closing)
        .fixed("option[ data-hidden=true ] { }"),
      bad("option[dataHidden ] { }")
        .at(1, 8, expected_opening)
        .fixed("option[ dataHidden ] { }"),
      bad("option[ dataHidden] { }")
        .at(1, 18, expected_closing)
        .fixed("option[ dataHidden ] { }"),
      bad("@media screen and (max-width: 480px) { img[align=right ] {} }")
        .at(1, 44, expected_opening)
        .fixed("@media screen and (max-width: 480px) { img[ align=right ] {} }"),
      bad("@media screen and (max-width: 480px) { img[ align=right] {} }")
        .at(1, 55, expected_closing)
        .fixed("@media screen and (max-width: 480px) { img[ align=right ] {} }"),
      bad("[target=_blank ] .foo { }")
        .at(1, 2, expected_opening)
        .fixed("[ target=_blank ] .foo { }"),
      bad("[ target=_blank] .foo { }")
        .at(1, 15, expected_closing)
        .fixed("[ target=_blank ] .foo { }"),
      bad("li[aria-hidden=false ]:nth-child(1) { }")
        .at(1, 4, expected_opening)
        .fixed("li[ aria-hidden=false ]:nth-child(1) { }"),
      bad("li[ aria-hidden=false]:nth-child(1) { }")
        .at(1, 21, expected_closing)
        .fixed("li[ aria-hidden=false ]:nth-child(1) { }"),
      bad("li[aria-hidden=false ]:nth-child( 1 ) { }")
        .at(1, 4, expected_opening)
        .fixed("li[ aria-hidden=false ]:nth-child( 1 ) { }"),
      bad("li[ aria-hidden=false]:nth-child( 1 ) { }")
        .at(1, 21, expected_closing)
        .fixed("li[ aria-hidden=false ]:nth-child( 1 ) { }"),
      bad("ul li[aria-hidden=false ] + li[ aria-hidden=false ] a { }")
        .at(1, 7, expected_opening)
        .fixed("ul li[ aria-hidden=false ] + li[ aria-hidden=false ] a { }"),
      bad("ul li[ aria-hidden=false] + li[ aria-hidden=false ] a { }")
        .at(1, 24, expected_closing)
        .fixed("ul li[ aria-hidden=false ] + li[ aria-hidden=false ] a { }"),
      bad("ul li[ aria-hidden=false ] + li[aria-hidden=false ] a { }")
        .at(1, 33, expected_opening)
        .fixed("ul li[ aria-hidden=false ] + li[ aria-hidden=false ] a { }"),
      bad("ul li[ aria-hidden=false ] + li[ aria-hidden=false] a { }")
        .at(1, 50, expected_closing)
        .fixed("ul li[ aria-hidden=false ] + li[ aria-hidden=false ] a { }"),
      bad("[target ] { content: \"[\" }")
        .at(1, 2, expected_opening)
        .fixed("[ target ] { content: \"[\" }"),
      bad("[ target] { content: \"[\" }")
        .at(1, 8, expected_closing)
        .fixed("[ target ] { content: \"[\" }"),
      bad("[target ] { content: \"]\" }")
        .at(1, 2, expected_opening)
        .fixed("[ target ] { content: \"]\" }"),
      bad("[ target] { content: \"]\" }")
        .at(1, 8, expected_closing)
        .fixed("[ target ] { content: \"]\" }"),
      bad("[ foo=bar i] { }")
        .at(1, 11, expected_closing)
        .fixed("[ foo=bar i ] { }"),
      bad("[foo=bar i ] { }")
        .at(1, 2, expected_opening)
        .fixed("[ foo=bar i ] { }"),
      bad("img[alt~=person][src*=lorem] { }")
        .at(1, 5, expected_opening)
        .at(1, 15, expected_closing)
        .at(1, 18, expected_opening)
        .at(1, 27, expected_closing)
        .fixed("img[ alt~=person ][ src*=lorem ] { }"),
      bad("[/**/foo=bar i] { }")
        .at(1, 2, expected_opening)
        .at(1, 14, expected_closing)
        .fixed("[ /**/foo=bar i ] { }"),
      bad("[foo=bar i /**/] { }")
        .at(1, 2, expected_opening)
        .at(1, 15, expected_closing)
        .fixed("[ foo=bar i /**/ ] { }"),
      bad("[foo=bar i/**/] { }")
        .at(1, 2, expected_opening)
        .at(1, 14, expected_closing)
        .fixed("[ foo=bar i/**/ ] { }"),
      bad("[foo=bar/**/] { }")
        .at(1, 2, expected_opening)
        .at(1, 12, expected_closing)
        .fixed("[ foo=bar/**/ ] { }"),
      bad("[foo/**/] { }")
        .at(1, 2, expected_opening)
        .at(1, 8, expected_closing)
        .fixed("[ foo/**/ ] { }"),
    ],
  );
}

#[test]
fn selector_attribute_brackets_space_inside_never() {
  let rejected_closing = "Unexpected whitespace before \"]\"";
  let rejected_opening = "Unexpected whitespace after \"[\"";
  check(
    "@stylistic/selector-attribute-brackets-space-inside",
    json!(["never"]),
    Css,
    &[
      ok(".foo { }"),
      ok("[target] { }"),
      ok("[target] { }"),
      ok("[target=_blank] { }"),
      ok("[target='_blank'] { }"),
      ok("[target=\"_blank\"] { }"),
      ok("[title~=flower] { }"),
      ok("[title|=flower] { }"),
      ok("[title^=flower] { }"),
      ok("[title$=flower] { }"),
      ok("a[href*=w3schools] { }"),
      ok("img[alt~=person][src*=lorem] { }"),
      ok("[target=_blank]:hover { }"),
      ok("[target=_blank]::before { }"),
      ok("option[data-hidden=true] { }"),
      ok("option[dataHidden] { }"),
      ok("@media screen and (max-width: 480px) { img[align=right] {} }"),
      ok("[target=_blank] .foo { }"),
      ok("li[aria-hidden=false]:nth-child(1) { }"),
      ok("li[aria-hidden=false]:nth-child( 1) { }"),
      ok("li[aria-hidden=false]:nth-child(1 ) { }"),
      ok("li[aria-hidden=false]:nth-child( 1 ) { }"),
      ok("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      ok("[target] { content: \"[\" }"),
      ok("[target] { content: \"]\" }"),
      ok("[foo=bar i] { }"),
      bad("[ target] { }")
        .at(1, 2, rejected_opening)
        .fixed("[target] { }"),
      bad("[target ] { }")
        .at(1, 8, rejected_closing)
        .fixed("[target] { }"),
      bad("[  target] { }")
        .at(1, 2, rejected_opening)
        .fixed("[target] { }"),
      bad("[target  ] { }")
        .at(1, 9, rejected_closing)
        .fixed("[target] { }"),
      bad("[ target=_blank] { }")
        .at(1, 2, rejected_opening)
        .fixed("[target=_blank] { }"),
      bad("[target=_blank ] { }")
        .at(1, 15, rejected_closing)
        .fixed("[target=_blank] { }"),
      bad("[  target=_blank] { }")
        .at(1, 2, rejected_opening)
        .fixed("[target=_blank] { }"),
      bad("[target=_blank  ] { }")
        .at(1, 16, rejected_closing)
        .fixed("[target=_blank] { }"),
      bad("[ target='_blank'] { }")
        .at(1, 2, rejected_opening)
        .fixed("[target='_blank'] { }"),
      bad("[target='_blank' ] { }")
        .at(1, 17, rejected_closing)
        .fixed("[target='_blank'] { }"),
      bad("[ target=\"_blank\"] { }")
        .at(1, 2, rejected_opening)
        .fixed("[target=\"_blank\"] { }"),
      bad("[target=\"_blank\" ] { }")
        .at(1, 17, rejected_closing)
        .fixed("[target=\"_blank\"] { }"),
      bad("a[ href*=w3schools] { }")
        .at(1, 3, rejected_opening)
        .fixed("a[href*=w3schools] { }"),
      bad("a[href*=w3schools ] { }")
        .at(1, 18, rejected_closing)
        .fixed("a[href*=w3schools] { }"),
      bad("img[ alt~=person][src*=lorem] { }")
        .at(1, 5, rejected_opening)
        .fixed("img[alt~=person][src*=lorem] { }"),
      bad("img[alt~=person ][src*=lorem] { }")
        .at(1, 16, rejected_closing)
        .fixed("img[alt~=person][src*=lorem] { }"),
      bad("img[alt~=person][ src*=lorem] { }")
        .at(1, 18, rejected_opening)
        .fixed("img[alt~=person][src*=lorem] { }"),
      bad("img[alt~=person][src*=lorem ] { }")
        .at(1, 28, rejected_closing)
        .fixed("img[alt~=person][src*=lorem] { }"),
      bad("[ target=_blank]:hover { }")
        .at(1, 2, rejected_opening)
        .fixed("[target=_blank]:hover { }"),
      bad("[target=_blank ]:hover { }")
        .at(1, 15, rejected_closing)
        .fixed("[target=_blank]:hover { }"),
      bad("[ target=_blank]::before { }")
        .at(1, 2, rejected_opening)
        .fixed("[target=_blank]::before { }"),
      bad("[target=_blank ]::before { }")
        .at(1, 15, rejected_closing)
        .fixed("[target=_blank]::before { }"),
      bad("option[ data-hidden=true] { }")
        .at(1, 8, rejected_opening)
        .fixed("option[data-hidden=true] { }"),
      bad("option[data-hidden=true ] { }")
        .at(1, 24, rejected_closing)
        .fixed("option[data-hidden=true] { }"),
      bad("option[ dataHidden] { }")
        .at(1, 8, rejected_opening)
        .fixed("option[dataHidden] { }"),
      bad("option[dataHidden ] { }")
        .at(1, 18, rejected_closing)
        .fixed("option[dataHidden] { }"),
      bad("@media screen and (max-width: 480px) { img[ align=right] {} }")
        .at(1, 44, rejected_opening)
        .fixed("@media screen and (max-width: 480px) { img[align=right] {} }"),
      bad("@media screen and (max-width: 480px) { img[align=right ] {} }")
        .at(1, 55, rejected_closing)
        .fixed("@media screen and (max-width: 480px) { img[align=right] {} }"),
      bad("[ target=_blank] .foo { }")
        .at(1, 2, rejected_opening)
        .fixed("[target=_blank] .foo { }"),
      bad("[target=_blank ] .foo { }")
        .at(1, 15, rejected_closing)
        .fixed("[target=_blank] .foo { }"),
      bad("li[ aria-hidden=false]:nth-child(1) { }")
        .at(1, 4, rejected_opening)
        .fixed("li[aria-hidden=false]:nth-child(1) { }"),
      bad("li[aria-hidden=false ]:nth-child(1) { }")
        .at(1, 21, rejected_closing)
        .fixed("li[aria-hidden=false]:nth-child(1) { }"),
      bad("li[ aria-hidden=false]:nth-child( 1 ) { }")
        .at(1, 4, rejected_opening)
        .fixed("li[aria-hidden=false]:nth-child( 1 ) { }"),
      bad("li[aria-hidden=false ]:nth-child( 1 ) { }")
        .at(1, 21, rejected_closing)
        .fixed("li[aria-hidden=false]:nth-child( 1 ) { }"),
      bad("ul li[ aria-hidden=false] + li[aria-hidden=false] a { }")
        .at(1, 7, rejected_opening)
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden=false ] + li[aria-hidden=false] a { }")
        .at(1, 24, rejected_closing)
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden=false] + li[ aria-hidden=false] a { }")
        .at(1, 31, rejected_opening)
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden=false ] a { }")
        .at(1, 48, rejected_closing)
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("[ target] { content: \"[\" }")
        .at(1, 2, rejected_opening)
        .fixed("[target] { content: \"[\" }"),
      bad("[target ] { content: \"[\" }")
        .at(1, 8, rejected_closing)
        .fixed("[target] { content: \"[\" }"),
      bad("[ target] { content: \"]\" }")
        .at(1, 2, rejected_opening)
        .fixed("[target] { content: \"]\" }"),
      bad("[target ] { content: \"]\" }")
        .at(1, 8, rejected_closing)
        .fixed("[target] { content: \"]\" }"),
      bad("[ foo=bar i] { }")
        .at(1, 2, rejected_opening)
        .fixed("[foo=bar i] { }"),
      bad("[foo=bar i ] { }")
        .at(1, 11, rejected_closing)
        .fixed("[foo=bar i] { }"),
      bad("img[ alt~=person ][ src*=lorem ] { }")
        .at(1, 5, rejected_opening)
        .at(1, 17, rejected_closing)
        .at(1, 20, rejected_opening)
        .at(1, 31, rejected_closing)
        .fixed("img[alt~=person][src*=lorem] { }"),
      bad("[ /**/foo=bar i ] { }")
        .at(1, 2, rejected_opening)
        .at(1, 16, rejected_closing)
        .fixed("[/**/foo=bar i] { }"),
      bad("[ foo=bar i /**/ ] { }")
        .at(1, 2, rejected_opening)
        .at(1, 17, rejected_closing)
        .fixed("[foo=bar i /**/] { }"),
      bad("[ foo=bar i/**/ ] { }")
        .at(1, 2, rejected_opening)
        .at(1, 16, rejected_closing)
        .fixed("[foo=bar i/**/] { }"),
      bad("[ foo=bar/**/ ] { }")
        .at(1, 2, rejected_opening)
        .at(1, 14, rejected_closing)
        .fixed("[foo=bar/**/] { }"),
      bad("[ foo/**/ ] { }")
        .at(1, 2, rejected_opening)
        .at(1, 10, rejected_closing)
        .fixed("[foo/**/] { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_attribute_brackets_space_inside_never_gale_cases() {
  check(
    "@stylistic/selector-attribute-brackets-space-inside",
    json!("never"),
    Css,
    &[
      ok("a[href] { }"),
      bad("a[ href ] { }")
        .at(1, 3, "Unexpected whitespace after \"[\"")
        .at(1, 8, "Unexpected whitespace before \"]\"")
        .fixed("a[href] { }"),
      ok("a::after { content: \"[foo]\"; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_attribute_brackets_space_inside_always_gale_cases() {
  check(
    "@stylistic/selector-attribute-brackets-space-inside",
    json!("always"),
    Css,
    &[
      ok("a[ href ] { }"),
      bad("a[href] { }")
        .at(1, 3, "Expected single space after \"[\"")
        .at(1, 6, "Expected single space before \"]\"")
        .fixed("a[ href ] { }"),
    ],
  );
}

#[test]
fn selector_pseudo_class_parentheses_space_inside_always() {
  let expected_closing = "Expected single space before \")\"";
  let expected_opening = "Expected single space after \"(\"";
  check(
    "@stylistic/selector-pseudo-class-parentheses-space-inside",
    json!(["always"]),
    Css,
    &[
      ok("input:not( [type='submit'] ) { }"),
      ok("input:not( [type='submit'], [type='text'] ) { }"),
      ok("input:not(  [type='submit'], [type='text']  ) { }"),
      ok("p:lang( it ) { }"),
      ok("section:not( :has( h1, h2 ) ) { }"),
      ok("input:not( [type='radio'] ):not( [type='checkbox'] ) { }"),
      ok("a:hover:not( .active ) { }"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a[b=#{c}] { }"),
      bad("input:not([type='submit'] ) { }")
        .at(1, 11, expected_opening)
        .fixed("input:not( [type='submit'] ) { }"),
      bad("input:not( [type='submit']) { }")
        .at(1, 26, expected_closing)
        .fixed("input:not( [type='submit'] ) { }"),
      bad("input:not([type='submit'], [type='text'] ) { }")
        .at(1, 11, expected_opening)
        .fixed("input:not( [type='submit'], [type='text'] ) { }"),
      bad("input:not( [type='submit'], [type='text']) { }")
        .at(1, 41, expected_closing)
        .fixed("input:not( [type='submit'], [type='text'] ) { }"),
      bad("input:not(  [type='submit']) { }")
        .at(1, 27, expected_closing)
        .fixed("input:not(  [type='submit'] ) { }"),
      bad("section:not(:has( h1, h2 ) ) { }")
        .at(1, 13, expected_opening)
        .fixed("section:not( :has( h1, h2 ) ) { }"),
      bad("section:not( :has( h1, h2 )) { }")
        .at(1, 27, expected_closing)
        .fixed("section:not( :has( h1, h2 ) ) { }"),
      bad("section:not( :has(h1, h2 ) ) { }")
        .at(1, 19, expected_opening)
        .fixed("section:not( :has( h1, h2 ) ) { }"),
      bad(":matches( a, ul, :has(h1, h2 ) ) { }")
        .at(1, 23, expected_opening)
        .fixed(":matches( a, ul, :has( h1, h2 ) ) { }"),
      bad("section:not( :has( h1, h2) ) { }")
        .at(1, 25, expected_closing)
        .fixed("section:not( :has( h1, h2 ) ) { }"),
      bad("section:not(:has(h1, h2)) { }")
        .at(1, 13, expected_opening)
        .at(1, 24, expected_closing)
        .at(1, 18, expected_opening)
        .at(1, 23, expected_closing)
        .fixed("section:not( :has( h1, h2 ) ) { }"),
      bad("input:not([type='radio'] ):not( [type='checkbox'] ) { }")
        .at(1, 11, expected_opening)
        .fixed("input:not( [type='radio'] ):not( [type='checkbox'] ) { }"),
      bad("input:not( [type='radio']):not( [type='checkbox'] ) { }")
        .at(1, 25, expected_closing)
        .fixed("input:not( [type='radio'] ):not( [type='checkbox'] ) { }"),
      bad("input:not( [type='radio'] ):not([type='checkbox'] ) { }")
        .at(1, 33, expected_opening)
        .fixed("input:not( [type='radio'] ):not( [type='checkbox'] ) { }"),
      bad("input:not( [type='radio'] ):not( [type='checkbox']) { }")
        .at(1, 50, expected_closing)
        .fixed("input:not( [type='radio'] ):not( [type='checkbox'] ) { }"),
      bad("a:hover:not(.active ) { }")
        .at(1, 13, expected_opening)
        .fixed("a:hover:not( .active ) { }"),
      bad("a:hover:not( .active) { }")
        .at(1, 20, expected_closing)
        .fixed("a:hover:not( .active ) { }"),
      bad("section:not(/**/:has(/**/h1, h2/**/)/**/) { }")
        .at(1, 13, expected_opening)
        .at(1, 40, expected_closing)
        .at(1, 22, expected_opening)
        .at(1, 35, expected_closing)
        .fixed("section:not( /**/:has( /**/h1, h2/**/ )/**/ ) { }"),
    ],
  );
}

#[test]
fn selector_pseudo_class_parentheses_space_inside_never() {
  let rejected_closing = "Unexpected whitespace before \")\"";
  let rejected_opening = "Unexpected whitespace after \"(\"";
  check(
    "@stylistic/selector-pseudo-class-parentheses-space-inside",
    json!(["never"]),
    Css,
    &[
      ok("input:not([type='submit']) { }"),
      ok("input:not([type='submit'], [type='text']) { }"),
      ok("input:not([type='submit'], [type='text']) { }"),
      ok("p:lang(it) { }"),
      ok("section:not(:has(h1, h2)) { }"),
      ok("input:not([type='radio']):not([type='checkbox']) { }"),
      ok("a:hover:not(.active) { }"),
      ok(":matches(a, ul, :has(h1, h2)) { }"),
      ok(".foo {\n\t&:has(\n\t\th1,\n\t\th2\n\t) {\n\t\tgap: 0;\n\t}\n}"),
      ok(".foo {\n    &:has(\n        h1,\n        h2\n    ) {\n        gap: 0;\n    }\n}"),
      bad("input:not([type='submit'] ) { }")
        .at(1, 26, rejected_closing)
        .fixed("input:not([type='submit']) { }"),
      bad("input:not( [type='submit']) { }")
        .at(1, 11, rejected_opening)
        .fixed("input:not([type='submit']) { }"),
      bad("input:not([type='submit'], [type='text'] ) { }")
        .at(1, 41, rejected_closing)
        .fixed("input:not([type='submit'], [type='text']) { }"),
      bad("input:not( [type='submit'], [type='text']) { }")
        .at(1, 11, rejected_opening)
        .fixed("input:not([type='submit'], [type='text']) { }"),
      bad("input:not(  [type='submit']) { }")
        .at(1, 11, rejected_opening)
        .fixed("input:not([type='submit']) { }"),
      bad("section:not( :has(h1, h2)) { }")
        .at(1, 13, rejected_opening)
        .fixed("section:not(:has(h1, h2)) { }"),
      bad("section:not(:has( h1, h2)) { }")
        .at(1, 18, rejected_opening)
        .fixed("section:not(:has(h1, h2)) { }"),
      bad("section:not(:has(h1, h2) ) { }")
        .at(1, 25, rejected_closing)
        .fixed("section:not(:has(h1, h2)) { }"),
      bad("section:not(:has(h1, h2 )) { }")
        .at(1, 24, rejected_closing)
        .fixed("section:not(:has(h1, h2)) { }"),
      bad("section:not( :has( h1, h2 ) ) { }")
        .at(1, 13, rejected_opening)
        .at(1, 28, rejected_closing)
        .at(1, 19, rejected_opening)
        .at(1, 26, rejected_closing)
        .fixed("section:not(:has(h1, h2)) { }"),
      bad("input:not( [type='radio']):not([type='checkbox']) { }")
        .at(1, 11, rejected_opening)
        .fixed("input:not([type='radio']):not([type='checkbox']) { }"),
      bad("input:not([type='radio'] ):not([type='checkbox']) { }")
        .at(1, 25, rejected_closing)
        .fixed("input:not([type='radio']):not([type='checkbox']) { }"),
      bad("input:not([type='radio']):not( [type='checkbox']) { }")
        .at(1, 31, rejected_opening)
        .fixed("input:not([type='radio']):not([type='checkbox']) { }"),
      bad("input:not([type='radio']):not([type='checkbox'] ) { }")
        .at(1, 48, rejected_closing)
        .fixed("input:not([type='radio']):not([type='checkbox']) { }"),
      bad("a:hover:not( .active) { }")
        .at(1, 13, rejected_opening)
        .fixed("a:hover:not(.active) { }"),
      bad("a:hover:not(.active ) { }")
        .at(1, 20, rejected_closing)
        .fixed("a:hover:not(.active) { }"),
      // Upstream skips this case: the plugin's one fix pass leaves the spaces before the
      // `)`s; gale fixes until nothing changes, as a second Stylelint run would.
      bad("section:not( /**/ :has( /**/ h1, h2 /**/ ) /**/ ) { }")
        .at(1, 13, rejected_opening)
        .at(1, 48, rejected_closing)
        .at(1, 24, rejected_opening)
        .at(1, 41, rejected_closing)
        .fixed("section:not(/**/ :has(/**/ h1, h2 /**/) /**/) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_pseudo_class_parentheses_space_inside_never_gale_cases() {
  check(
    "@stylistic/selector-pseudo-class-parentheses-space-inside",
    json!("never"),
    Css,
    &[
      ok("a:not(.foo) { }"),
      bad("a:not( .foo ) { }")
        .at(1, 7, "Unexpected whitespace after \"(\"")
        .at(1, 12, "Unexpected whitespace before \")\"")
        .fixed("a:not(.foo) { }"),
      ok("a::slotted(.foo) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_pseudo_class_parentheses_space_inside_always_gale_cases() {
  check(
    "@stylistic/selector-pseudo-class-parentheses-space-inside",
    json!("always"),
    Css,
    &[
      ok("a:not( .foo ) { }"),
      bad("a:not(.foo) { }")
        .at(1, 7, "Expected single space after \"(\"")
        .at(1, 10, "Expected single space before \")\"")
        .fixed("a:not( .foo ) { }"),
    ],
  );
}
