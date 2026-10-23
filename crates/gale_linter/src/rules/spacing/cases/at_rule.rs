//! The `at_rule` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn at_rule_name_space_after_always() {
  check(
    "@stylistic/at-rule-name-space-after",
    json!(["always"]),
    Css,
    &[
      ok("@charset \"UTF-8\";"),
      ok("@charset \"UTF-8\";"),
      ok("@import \"x.css\";"),
      ok("@import \"x.css\" screen and (orientation:landscape);"),
      ok("@import url(\"x.css\");"),
      ok("@import url(\"x.css\") screen and (orientation:landscape);"),
      ok("@namespace url(XML-namespace-URL);"),
      ok("@media (min-width: 700px) and (orientation: landscape) { }"),
      ok("@media (min-width: 700px) and (orientation: landscape)  { }"),
      ok("@media (min-width: 700px) and (orientation: landscape)\n{ }"),
      ok("@media (min-width: 700px) and (orientation: landscape)\r\n{ }"),
      ok("@media (min-width: 700px)  and (orientation: landscape) { }"),
      ok("@media (min-width: 700px)\nand (orientation: landscape) { }"),
      ok("@media (min-width: 700px)\r\nand (orientation: landscape) { }"),
      ok("@supports (animation-name: test) { }"),
      ok("@keyframes identifier { }"),
      ok("@-webkit-keyframes identifier { }"),
      ok("@viewport { }"),
      ok("@viewport{ }"),
      ok("@viewport\n{ }"),
      ok("@viewport\r\n{ }"),
      ok("@viewport\n\n{ }"),
      ok("@viewport\r\n\r\n{ }"),
      ok("@counter-style winners-list { }"),
      ok("@font-face { };"),
      ok("@unknown \"ident\";"),
      ok("@unknown ident { };"),
      ok("a { color: pink; @crazy-custom-at-rule; }"),
      bad("@charset\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset  \"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset\n\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset\r\n\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset\n\n\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset\r\n\r\n\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@media(width <= 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media (width <= 100px) { }"),
      bad("@media\n(width <= 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media (width <= 100px) { }"),
      bad("@media\r\n(width <= 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media (width <= 100px) { }"),
      bad("@media  (width <= 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media (width <= 100px) { }"),
      bad("@unknown\"ident\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@unknown\"",
        )
        .fixed("@unknown \"ident\";"),
      bad("@unknown\"ident\" { };")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@unknown\"",
        )
        .fixed("@unknown \"ident\" { };"),
      bad("@unknown  ident { };")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@unknown\"",
        )
        .fixed("@unknown ident { };"),
      bad("@-webkit-keyframes  ident { };")
        .at(
          1,
          18,
          "Expected single space after at-rule name \"@-webkit-keyframes\"",
        )
        .fixed("@-webkit-keyframes ident { };"),
      bad("@media/*comment*/(width <= 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media /*comment*/(width <= 100px) { }"),
    ],
  );
}

#[test]
fn at_rule_name_space_after_always_less() {
  check(
    "@stylistic/at-rule-name-space-after",
    json!(["always"]),
    Less,
    &[
      ok("@nice-blue:#5B83AD;"),
      ok("@nice-blue: #5B83AD;"),
      ok("@nice-blue:\n#5B83AD;"),
      ok("@variable: .bucket; .@{variable} { }"),
      ok("@detached-ruleset: { background: red; }; .top { @detached-ruleset(); }"),
      ok("@my-ruleset: { .my-selector { background-color: black; } };"),
      ok(".class1 { .mixin(#ddd) }"),
      ok(".button { &-ok {} }"),
    ],
  );
}

#[test]
fn at_rule_name_space_after_always_scss() {
  check(
    "@stylistic/at-rule-name-space-after",
    json!(["always"]),
    Scss,
    &[ok(
      "@mixin mixin() { @content; }; .colors { @include mixin { color: $color; }}",
    )],
  );
}

#[test]
fn at_rule_name_space_after_always_single_line() {
  check(
    "@stylistic/at-rule-name-space-after",
    json!(["always-single-line"]),
    Css,
    &[
      ok("@charset \"UTF-8\";"),
      ok("@charset\n\"UTF-8\";"),
      ok("@charset\r\n\"UTF-8\";"),
      ok("@charset\n\n\"UTF-8\";"),
      ok("@charset\r\n\r\n\"UTF-8\";"),
      ok("@charset \"UTF-8\";"),
      ok("@charset\n\"UTF-8\";"),
      ok("@charset\r\n\"UTF-8\";"),
      ok("@charset\n\n\"UTF-8\";"),
      ok("@charset\r\n\r\n\"UTF-8\";"),
      ok("@import \"x.css\";"),
      ok("@import \"x.css\" screen and (orientation:landscape);"),
      ok("@import url(\"x.css\");"),
      ok("@import\nurl(\"x.css\");"),
      ok("@import\r\nurl(\"x.css\");"),
      ok("@import url(\"x.css\") screen and (orientation:landscape);"),
      ok("@namespace url(XML-namespace-URL);"),
      ok("@media (min-width: 700px) and (orientation: landscape) { }"),
      ok("@media (min-width: 700px) and (orientation: landscape)  { }"),
      ok("@media (min-width: 700px) and (orientation: landscape)\n{ }"),
      ok("@media (min-width: 700px) and (orientation: landscape)\r\n{ }"),
      ok("@media (min-width: 700px)  and (orientation: landscape) { }"),
      ok("@media (min-width: 700px)\nand (orientation: landscape) { }"),
      ok("@media (min-width: 700px)\r\nand (orientation: landscape) { }"),
      ok("@media(min-width: 700px)\nand (orientation: landscape) { }"),
      ok("@media(min-width: 700px)\r\nand (orientation: landscape) { }"),
      ok("@media(min-width: 700px) and\n(orientation: landscape) { }"),
      ok("@media(min-width: 700px) and\r\n(orientation: landscape) { }"),
      ok("@media\n(min-width: 700px) and (orientation: landscape) { }"),
      ok("@media\r\n(min-width: 700px) and (orientation: landscape) { }"),
      ok("@supports (animation-name: test) { }"),
      ok("@keyframes identifier { }"),
      ok("@-webkit-keyframes identifier { }"),
      ok("@viewport { }"),
      ok("@viewport{ }"),
      ok("@viewport\n{ }"),
      ok("@viewport\r\n{ }"),
      ok("@viewport\n\n{ }"),
      ok("@viewport\r\n\r\n{ }"),
      ok("@counter-style winners-list { }"),
      ok("@font-face { };"),
      ok("@unknown \"ident\";"),
      ok("@unknown ident { };"),
      bad("@charset\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset\"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@charset  \"UTF-8\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@charset\"",
        )
        .fixed("@charset \"UTF-8\";"),
      bad("@media(width <= 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media (width <= 100px) { }"),
      bad("@media  (width <= 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media (width <= 100px) { }"),
      bad("@unknown\"ident\";")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@unknown\"",
        )
        .fixed("@unknown \"ident\";"),
      bad("@unknown\"ident\" { };")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@unknown\"",
        )
        .fixed("@unknown \"ident\" { };"),
      bad("@unknown  ident { };")
        .at(
          1,
          8,
          "Expected single space after at-rule name \"@unknown\"",
        )
        .fixed("@unknown ident { };"),
      bad("@-webkit-keyframes  ident { };")
        .at(
          1,
          18,
          "Expected single space after at-rule name \"@-webkit-keyframes\"",
        )
        .fixed("@-webkit-keyframes ident { };"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn at_rule_name_space_after_always_gale_cases() {
  check(
    "@stylistic/at-rule-name-space-after",
    json!("always"),
    Css,
    &[
      ok("@media screen { }"),
      bad("@media(min-width: 100px) { }")
        .at(1, 6, "Expected single space after at-rule name \"@media\"")
        .fixed("@media (min-width: 100px) { }"),
    ],
  );
}

#[test]
fn at_rule_semicolon_space_before_always() {
  let expected_before = "Expected single space before \";\"";
  check(
    "@stylistic/at-rule-semicolon-space-before",
    json!(["always"]),
    Css,
    &[
      ok("@import \"styles/mystyle\" ;"),
      ok(
        "@font-face {\n font-family: \"MyFont\"; src: url(\"myfont.woff2\") format(\"woff2\");\n}",
      ),
      ok(
        "@font-face {\n font-family: \"MyFont\"; src: url(\"myfont.woff2\") format(\"woff2\");\n};",
      ),
      ok("a { color: @brand-primary; }"),
      ok("@myatrule \"valuehassemicolon;\" ;"),
      ok("@import url(http://www.example.com/location;withsemicolon) ;"),
      ok("@import /*my styles;*/ \"styles/mystyle\" ;"),
      ok("@import\nurl('landscape.css')\nprojection ;"),
      bad("@import \"styles/mystyle\";")
        .at(1, 24, expected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"  ;")
        .at(1, 26, expected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"\t;")
        .at(1, 25, expected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"\n;")
        .at(1, 25, expected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"\r\n;")
        .at(1, 26, expected_before)
        .unfixed(),
      bad("@import\nurl('landscape.css')\nprojection;")
        .at(3, 10, expected_before)
        .unfixed(),
    ],
  );
}

#[test]
fn at_rule_semicolon_space_before_never() {
  let rejected_before = "Unexpected whitespace before \";\"";
  check(
    "@stylistic/at-rule-semicolon-space-before",
    json!(["never"]),
    Css,
    &[
      ok("@import \"styles/mystyle\";"),
      ok(
        "@font-face {\n font-family: \"MyFont\" ; src: url(\"myfont.woff2\") format(\"woff2\") ;\n}",
      ),
      ok(
        "@font-face {\n font-family: \"MyFont\" ; src: url(\"myfont.woff2\") format(\"woff2\") ;\n} ;",
      ),
      ok("a { color: @brand-primary ; }"),
      ok("@myatrule \"valuehassemicolon ;\";"),
      ok("@import url(http://www.example.com/location+;withsemicolon);"),
      ok("@import /*my styles ;*/ \"styles/mystyle\";"),
      ok("@import\nurl('landscape.css')\nprojection;"),
      bad("@import \"styles/mystyle\" ;")
        .at(1, 25, rejected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"  ;")
        .at(1, 26, rejected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"\t;")
        .at(1, 25, rejected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"\n;")
        .at(1, 25, rejected_before)
        .unfixed(),
      bad("@import \"styles/mystyle\"\r\n;")
        .at(1, 26, rejected_before)
        .unfixed(),
      bad("@import\nurl('landscape.css')\nprojection ;")
        .at(3, 11, rejected_before)
        .unfixed(),
    ],
  );
}

#[test]
fn at_rule_semicolon_space_before_always_less() {
  check(
    "@stylistic/at-rule-semicolon-space-before",
    json!(["always"]),
    Less,
    &[
      ok("\n        .someMixin() { margin: 0; }\n        span { .someMixin(); }\n      "),
      ok(
        "\n        @myVariable: #f7f8f9;\n        span { background-color: @myVariable; }\n      ",
      ),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn at_rule_semicolon_space_before_never_gale_cases() {
  check(
    "@stylistic/at-rule-semicolon-space-before",
    json!("never"),
    Css,
    &[
      ok("@import url(foo.css);"),
      bad("@import url(foo.css) ;")
        .at(1, 21, "Unexpected whitespace before \";\"")
        .unfixed(),
      ok("/* @import url(foo.css) ; */ @import url(bar.css);"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn at_rule_semicolon_space_before_always_gale_cases() {
  check(
    "@stylistic/at-rule-semicolon-space-before",
    json!("always"),
    Css,
    &[
      ok("@import url(foo.css) ;"),
      bad("@import url(foo.css);")
        .at(1, 20, "Expected single space before \";\"")
        .unfixed(),
    ],
  );
}

#[test]
fn at_rule_semicolon_newline_after_always() {
  let expected_after = "Expected newline after \";\"";
  check(
    "@stylistic/at-rule-semicolon-newline-after",
    json!(["always"]),
    Css,
    &[
      ok("@foo;\na {}"),
      ok("@import 'x.css';\na {}"),
      ok("@iMpOrT 'x.css';\na {}"),
      ok("@IMPORT 'x.css';\na {}"),
      ok("@charset 'UTF-8';\na {}"),
      ok("@charset 'UTF-8';\n@import 'x.css'"),
      ok("@charset 'UTF-8';\n@import 'x.css'\na {}"),
      ok("@namespace url(XML-namespace-URL);\na {}"),
      ok("@import 'x.css'); /* comment */\n"),
      ok("@import 'x.css');/* comment */\n"),
      ok("@import 'x.css');   /* comment */\n"),
      ok("@import 'x.css');\t/* comment */\n"),
      ok("@import 'x.css'); \t/* comment */\n"),
      ok("@charset 'UTF-8';\n@media {}"),
      ok("@import 'x.css';\r\n"),
      ok("@import 'x.css'; /* comment */\r\n"),
      ok("@import 'x.css';"),
      ok("a{\n@extend .b;\n@extend .c\n}"),
      ok("@font-face {}; a {}"),
      bad("@mixin foo; a {}")
        .at(1, 12, expected_after)
        .fixed("@mixin foo;\n a {}"),
      bad("@mIxIn foo; a {}")
        .at(1, 12, expected_after)
        .fixed("@mIxIn foo;\n a {}"),
      bad("@MIXIN foo; a {}")
        .at(1, 12, expected_after)
        .fixed("@MIXIN foo;\n a {}"),
      bad("@import url(\"x.css\"); @charset \"UTF-8\";")
        .at(1, 22, expected_after)
        .fixed("@import url(\"x.css\");\n @charset \"UTF-8\";"),
      bad("@charset \"UTF-8\"; a {};")
        .at(1, 18, expected_after)
        .fixed("@charset \"UTF-8\";\n a {};"),
      bad("a{\n@extend .b; @extend .c\n}")
        .at(2, 12, expected_after)
        .fixed("a{\n@extend .b;\n @extend .c\n}"),
      bad("a{\r\n@extend .b; @extend .c\r\n}")
        .at(2, 12, expected_after)
        .fixed("a{\r\n@extend .b;\r\n @extend .c\r\n}"),
    ],
  );
}

#[test]
fn at_rule_semicolon_newline_after_always_less() {
  check(
    "@stylistic/at-rule-semicolon-newline-after",
    json!(["always"]),
    Less,
    &[
      ok("\n        .someMixin() { margin: 0; }\n        span { .someMixin(); }\n      "),
      ok(
        "\n        @myVariable: #f7f8f9;\n        span { background-color: @myVariable; }\n      ",
      ),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn at_rule_semicolon_newline_after_always_gale_cases() {
  check(
    "@stylistic/at-rule-semicolon-newline-after",
    json!("always"),
    Css,
    &[
      ok("@import url(\"foo.css\");\na { }"),
      bad("@import url(\"foo.css\"); a { }")
        .at(1, 24, "Expected newline after \";\"")
        .fixed("@import url(\"foo.css\");\n a { }"),
      ok("@charset \"UTF-8\";"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn at_rule_semicolon_newline_after_always_scss_gale_cases() {
  check("@stylistic/at-rule-semicolon-newline-after", json!("always"), Scss, &[
    bad("a {\n\t// @todo refactor this away; it is\n\t// in the quote block.\n\t&:first-child { color: red; }\n}\n@import url(//example.com/a.css); b { }").at(6, 34, "Expected newline after \";\"").fixed("a {\n\t// @todo refactor this away; it is\n\t// in the quote block.\n\t&:first-child { color: red; }\n}\n@import url(//example.com/a.css);\n b { }"),
  ]);
}

#[test]
fn scss_at_else_if_parentheses_space_before_always_scss() {
  let expected_before = "Expected a single space before parentheses in else-if declaration";
  check(
    "scss/at-else-if-parentheses-space-before",
    json!(["always"]),
    Scss,
    &[
      ok("\n      @else if ($bar) {\n      }\n    "),
      ok("\n      @else if $bar {\n      }\n    "),
      ok("\n      @else if ($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    "),
      ok("\n      @else if ($bar == 0 or($bar > 5 and $bar < 10)) {\n      }\n    "),
      ok("\n      @else if $bar == 0 or ($bar > 5 and $bar < 10) {\n      }\n    "),
      ok("\n      @else if $bar == 0 or($bar > 5 and $bar < 10) {\n      }\n    "),
      ok("\n      @bar foo ($n) {\n      }\n    "),
      bad("\n      @else\n      if\n      ($foo) {\n      }\n    ")
        .span((4, 7), (4, 8), expected_before)
        .fixed("\n      @else\n      if ($foo) {\n      }\n    "),
      bad("\n      @else if($foo) {\n      }\n    ")
        .span((2, 15), (2, 16), expected_before)
        .fixed("\n      @else if ($foo) {\n      }\n    "),
      bad("\n      @else if  ($foo) {\n      }\n    ")
        .at(2, 17, expected_before)
        .fixed("\n      @else if ($foo) {\n      }\n    "),
      bad("\n      @else if($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    ")
        .at(2, 15, expected_before)
        .fixed("\n      @else if ($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    "),
    ],
  );
}

#[test]
fn scss_at_else_if_parentheses_space_before_never_scss() {
  let rejected_before = "Unexpected whitespace before parentheses in else-if declaration";
  check(
    "scss/at-else-if-parentheses-space-before",
    json!(["never"]),
    Scss,
    &[
      ok("\n      @else if($bar) {\n      }\n    "),
      ok("\n      @else if $bar {\n      }\n    "),
      ok("\n      @else if($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    "),
      ok("\n      @else if($bar == 0 or($bar > 5 and $bar < 10)) {\n      }\n    "),
      ok("\n      @else if $bar == 0 or ($bar > 5 and $bar < 10) {\n      }\n    "),
      ok("\n      @else if $bar == 0 or($bar > 5 and $bar < 10) {\n      }\n    "),
      ok("\n      @bar foo($n) {\n      }\n    "),
      bad("\n      @else\n      if\n      ($foo) {\n      }\n    ")
        .span((4, 7), (4, 8), rejected_before)
        .fixed("\n      @else\n      if($foo) {\n      }\n    "),
      bad("\n      @else if ($foo) {\n      }\n    ")
        .span((2, 16), (2, 17), rejected_before)
        .fixed("\n      @else if($foo) {\n      }\n    "),
      bad("\n      @else if ($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    ")
        .at(2, 16, rejected_before)
        .fixed("\n      @else if($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    "),
      bad("\n      @else if  ($foo) {\n      }\n    ")
        .at(2, 17, rejected_before)
        .fixed("\n      @else if($foo) {\n      }\n    "),
      bad("\n      @else if  ($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    ")
        .at(2, 17, rejected_before)
        .fixed("\n      @else if($bar == 0 or ($bar > 5 and $bar < 10)) {\n      }\n    "),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_else_if_parentheses_space_before_never_scss_gale_cases() {
  check(
    "scss/at-else-if-parentheses-space-before",
    json!("never"),
    Scss,
    &[
      ok("@if $x { } @else if($y) { }"),
      bad("@if $x { } @else if ($y) { }")
        .at(
          1,
          21,
          "Unexpected whitespace before parentheses in else-if declaration",
        )
        .fixed("@if $x { } @else if($y) { }"),
      ok("@if $x { } @else { }"),
      ok("@if $x { } @else if $y { }"),
      bad("@if $a { } @else if($b) { } @else if ($c) { }")
        .at(
          1,
          38,
          "Unexpected whitespace before parentheses in else-if declaration",
        )
        .fixed("@if $a { } @else if($b) { } @else if($c) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_else_if_parentheses_space_before_always_scss_gale_cases() {
  check(
    "scss/at-else-if-parentheses-space-before",
    json!("always"),
    Scss,
    &[
      ok("@if $x { } @else if ($y) { }"),
      bad("@if $x { } @else if($y) { }")
        .at(
          1,
          20,
          "Expected a single space before parentheses in else-if declaration",
        )
        .fixed("@if $x { } @else if ($y) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_else_if_parentheses_space_before_never_gale_cases() {
  check(
    "scss/at-else-if-parentheses-space-before",
    json!("never"),
    Css,
    &[ok("@else if($y) { }")],
  );
}

#[test]
fn scss_at_function_parentheses_space_before_always_scss() {
  let expected_before = "Expected a single space before parentheses in function declaration";
  check(
    "scss/at-function-parentheses-space-before",
    json!(["always"]),
    Scss,
    &[
      ok("\n      @function foo () {\n      }\n    "),
      ok("\n      @function foo ($links: 10){\n      }\n    "),
      ok("\n      @function foo ($n) {\n      }\n    "),
      ok("\n      @function  foo ($n) {\n      }\n    "),
      ok("\n      @functio1n foo ($n) {\n      }\n    "),
      ok("\n      @function foo-bar () {\n      }\n    "),
      bad("\n      @function\n      foo\n      ($n) {\n      }\n    ")
        .span((4, 7), (4, 8), expected_before)
        .fixed("\n      @function\n      foo ($n) {\n      }\n    "),
      bad("\n      @function foo($n) {\n      }\n    ")
        .span((2, 20), (2, 21), expected_before)
        .fixed("\n      @function foo ($n) {\n      }\n    "),
      bad("\n      @function  foo($n) {\n      }\n    ")
        .span((2, 21), (2, 22), expected_before)
        .fixed("\n      @function  foo ($n) {\n      }\n    "),
      bad("\n      @function foo-bar($n) {\n      }\n    ")
        .span((2, 24), (2, 25), expected_before)
        .fixed("\n      @function foo-bar ($n) {\n      }\n    "),
    ],
  );
}

#[test]
fn scss_at_function_parentheses_space_before_never_scss() {
  let rejected_before = "Unexpected whitespace before parentheses in function declaration";
  check(
    "scss/at-function-parentheses-space-before",
    json!(["never"]),
    Scss,
    &[
      ok("\n      @function foo() {\n      }\n    "),
      ok("\n      @function foo($links: 10){\n      }\n    "),
      ok("\n      @function foo($n) {\n      }\n    "),
      ok("\n      @function  foo($n) {\n      }\n    "),
      ok("\n      @functio1n foo($n) {\n      }\n    "),
      ok("\n      @function foo-bar() {\n      }\n    "),
      bad("\n      @function\n      foo\n      ($n) {\n      }\n    ")
        .span((4, 7), (4, 8), rejected_before)
        .fixed("\n      @function\n      foo($n) {\n      }\n    "),
      bad("\n      @function foo ($n) {\n      }\n    ")
        .span((2, 21), (2, 22), rejected_before)
        .fixed("\n      @function foo($n) {\n      }\n    "),
      bad("\n      @function foo  ($n) {\n      }\n    ")
        .span((2, 22), (2, 23), rejected_before)
        .fixed("\n      @function foo($n) {\n      }\n    "),
      bad("\n      @function foo-bar ($n) {\n      }\n    ")
        .span((2, 25), (2, 26), rejected_before)
        .fixed("\n      @function foo-bar($n) {\n      }\n    "),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_function_parentheses_space_before_never_scss_gale_cases() {
  check(
    "scss/at-function-parentheses-space-before",
    json!("never"),
    Scss,
    &[
      ok("@function foo($x) { @return $x; }"),
      bad("@function foo ($x) { @return $x; }")
        .at(
          1,
          15,
          "Unexpected whitespace before parentheses in function declaration",
        )
        .fixed("@function foo($x) { @return $x; }"),
      bad("@function foo($x) { @return $x; }\n@function bar ($y) { @return $y; }")
        .at(
          2,
          15,
          "Unexpected whitespace before parentheses in function declaration",
        )
        .fixed("@function foo($x) { @return $x; }\n@function bar($y) { @return $y; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_function_parentheses_space_before_always_scss_gale_cases() {
  check(
    "scss/at-function-parentheses-space-before",
    json!("always"),
    Scss,
    &[
      ok("@function foo ($x) { @return $x; }"),
      bad("@function foo($x) { @return $x; }")
        .at(
          1,
          14,
          "Expected a single space before parentheses in function declaration",
        )
        .fixed("@function foo ($x) { @return $x; }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_function_parentheses_space_before_never_gale_cases() {
  check(
    "scss/at-function-parentheses-space-before",
    json!("never"),
    Css,
    &[ok("@function foo($x) { }")],
  );
}

#[test]
fn scss_at_mixin_parentheses_space_before_always_scss() {
  let expected_before = "Expected a single space before parentheses in mixin declaration";
  check(
    "scss/at-mixin-parentheses-space-before",
    json!(["always"]),
    Scss,
    &[
      ok("\n      @mixin foo () {\n      }\n    "),
      ok("\n      @mixin foo {\n      }\n    "),
      ok("\n      @mixin foo ($links: 10){\n      }\n    "),
      ok("\n      @mixin foo ($n) {\n      }\n    "),
      ok("\n      @mixin  foo ($n) {\n      }\n    "),
      ok("\n      @bar foo ($n) {\n      }\n    "),
      ok("\n      @mixin foo-bar () {\n      }\n    "),
      bad("\n      @mixin\n      foo\n      ($n) {\n      }\n    ")
        .span((4, 7), (4, 8), expected_before)
        .fixed("\n      @mixin\n      foo ($n) {\n      }\n    "),
      bad("\n      @mixin foo($n) {\n      }\n    ")
        .at(2, 17, expected_before)
        .fixed("\n      @mixin foo ($n) {\n      }\n    "),
      bad("\n      @mixin  foo($n) {\n      }\n    ")
        .at(2, 18, expected_before)
        .fixed("\n      @mixin  foo ($n) {\n      }\n    "),
      bad("\n      @mixin foo-bar($n) {\n      }\n    ")
        .at(2, 21, expected_before)
        .fixed("\n      @mixin foo-bar ($n) {\n      }\n    "),
    ],
  );
}

#[test]
fn scss_at_mixin_parentheses_space_before_never_scss() {
  let rejected_before = "Unexpected whitespace before parentheses in mixin declaration";
  check(
    "scss/at-mixin-parentheses-space-before",
    json!(["never"]),
    Scss,
    &[
      ok("\n      @mixin foo() {\n      }\n    "),
      ok("\n      @mixin foo {\n      }\n    "),
      ok("\n      @mixin foo($links: 10){\n      }\n    "),
      ok("\n      @mixin foo($n) {\n      }\n    "),
      ok("\n      @mixin  foo($n) {\n      }\n    "),
      ok("\n      @bar foo($n) {\n      }\n    "),
      ok("\n      @mixin foo-bar() {\n      }\n    "),
      bad("\n      @mixin\n      foo\n      ($n) {\n      }\n    ")
        .span((4, 7), (4, 8), rejected_before)
        .fixed("\n      @mixin\n      foo($n) {\n      }\n    "),
      bad("\n      @mixin foo ($n) {\n      }\n    ")
        .at(2, 18, rejected_before)
        .fixed("\n      @mixin foo($n) {\n      }\n    "),
      bad("\n      @mixin foo  ($n) {\n      }\n    ")
        .at(2, 19, rejected_before)
        .fixed("\n      @mixin foo($n) {\n      }\n    "),
      bad("\n      @mixin foo-bar ($n) {\n      }\n    ")
        .at(2, 22, rejected_before)
        .fixed("\n      @mixin foo-bar($n) {\n      }\n    "),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_mixin_parentheses_space_before_never_scss_gale_cases() {
  check(
    "scss/at-mixin-parentheses-space-before",
    json!("never"),
    Scss,
    &[
      ok("@mixin foo($x) { }"),
      bad("@mixin foo ($x) { }")
        .at(
          1,
          12,
          "Unexpected whitespace before parentheses in mixin declaration",
        )
        .fixed("@mixin foo($x) { }"),
      ok("@mixin foo { }"),
      bad("@mixin foo($x) { }\n@mixin bar ($y) { }")
        .at(
          2,
          12,
          "Unexpected whitespace before parentheses in mixin declaration",
        )
        .fixed("@mixin foo($x) { }\n@mixin bar($y) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_mixin_parentheses_space_before_always_scss_gale_cases() {
  check(
    "scss/at-mixin-parentheses-space-before",
    json!("always"),
    Scss,
    &[
      ok("@mixin foo ($x) { }"),
      bad("@mixin foo($x) { }")
        .at(
          1,
          11,
          "Expected a single space before parentheses in mixin declaration",
        )
        .fixed("@mixin foo ($x) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn scss_at_mixin_parentheses_space_before_never_gale_cases() {
  check(
    "scss/at-mixin-parentheses-space-before",
    json!("never"),
    Css,
    &[ok("@mixin foo($x) { }")],
  );
}
