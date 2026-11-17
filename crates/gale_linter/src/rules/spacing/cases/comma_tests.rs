use gale_css_parser::Syntax;
use serde_json::json;

use crate::testing::{fix, invalid_options, lint};

/// `(line, column, message)` of each problem `rule` reports with
/// `option`.
fn problems(rule: &str, option: &str, source: &str) -> Vec<(usize, usize, String)> {
  problems_in(rule, option, source, Syntax::Css)
}

fn problems_in(
  rule: &str,
  option: &str,
  source: &str,
  syntax: Syntax,
) -> Vec<(usize, usize, String)> {
  let index = gale_diagnostics::SourceLineIndex::build(source);
  let mut found: Vec<_> = lint(rule, json!(option), source, syntax)
    .into_iter()
    .map(|d| {
      let (line, column) = index.offset_to_location(d.span.offset);
      (line, column, d.message)
    })
    .collect();
  found.sort();
  found
}

#[test]
fn selector_list_commas_are_checked_outside_functions() {
  let rule = "@stylistic/selector-list-comma-space-after";
  assert_eq!(
    problems(rule, "always", "a,b, c:is(d,e),  f {}"),
    [
      (1, 2, "Expected single space after \",\"".into()),
      (1, 15, "Expected single space after \",\"".into()),
    ]
  );
  assert_eq!(
    fix(rule, json!("always"), "a,b,  c {}", Syntax::Css),
    "a, b, c {}"
  );
  assert_eq!(
    problems(rule, "always-single-line", "a,\nb {}"),
    [],
    "a multi-line selector is not single-line"
  );
  let before = "@stylistic/selector-list-comma-space-before";
  assert_eq!(
    fix(before, json!("never"), "a , b\t,c {}", Syntax::Css),
    "a, b,c {}"
  );
}

#[test]
fn selector_list_newlines_allow_comments_and_indentation() {
  let after = "@stylistic/selector-list-comma-newline-after";
  assert_eq!(
    problems(after, "always", "a, /* x */\nb, c {}"),
    [(2, 2, "Expected newline after \",\"".into())]
  );
  assert_eq!(
    fix(after, json!("always"), "a, b,\nc {}", Syntax::Css),
    "a,\n b,\nc {}"
  );
  let before = "@stylistic/selector-list-comma-newline-before";
  assert_eq!(problems(before, "always", "a\n  , b {}"), []);
  assert_eq!(
    fix(before, json!("always"), "a , b {}", Syntax::Css),
    "a\n , b {}"
  );
  // Gale's own `"never"`: no line break before the comma.
  assert_eq!(
    problems(before, "never", "a\n, b , c {}"),
    [(2, 1, "Unexpected whitespace before \",\"".into())]
  );
}

#[test]
fn value_list_commas_report_and_fix_in_the_value() {
  let rule = "@stylistic/value-list-comma-space-after";
  assert_eq!(
    problems(
      rule,
      "always",
      "a { margin: 1px,2px; font: a, rgb(0,0,0),b; }"
    ),
    [
      (1, 16, "Expected single space after \",\"".into()),
      (1, 41, "Expected single space after \",\"".into()),
    ]
  );
  assert_eq!(
    fix(rule, json!("never"), "a { font: a,  b , c; }", Syntax::Css),
    "a { font: a,b ,c; }"
  );
  let newline = "@stylistic/value-list-comma-newline-after";
  assert_eq!(
    problems(
      newline,
      "always",
      "a { font: a, /* x */\n b, // y\n c, d; }"
    ),
    [(3, 3, "Expected newline after \",\"".into())]
  );
  // postcss-scss writes the `//` comment as `/* y*/`, two characters
  // longer, and Stylelint reports that many columns further on.
  assert_eq!(
    problems_in(
      newline,
      "always",
      "a { font: a, /* x */\n b, // y\n c, d; }",
      Syntax::Scss
    ),
    [(3, 5, "Expected newline after \",\"".into())]
  );
}

#[test]
fn function_commas_skip_url_and_the_first_comment() {
  let rule = "@stylistic/function-comma-space-after";
  assert_eq!(
    problems(
      rule,
      "always",
      "a { b: rgb(0,0, 0); c: url(a,b); d: f(a /* , */,b); }"
    ),
    [
      (1, 13, "Expected single space after \",\"".into()),
      (1, 48, "Expected single space after \",\"".into()),
    ]
  );
  assert_eq!(
    fix(rule, json!("always"), "a { b: rgb(0,0,  0); }", Syntax::Css),
    "a { b: rgb(0, 0, 0); }"
  );
  let before = "@stylistic/function-comma-space-before";
  assert_eq!(
    fix(
      before,
      json!("never"),
      "a { b: rgb(0 ,0 , 0); }",
      Syntax::Css
    ),
    // The plugin's fix keeps the space after the comma.
    "a { b: rgb(0,0, 0); }"
  );
  let newline = "@stylistic/function-comma-newline-after";
  assert_eq!(
    fix(
      newline,
      json!("always-multi-line"),
      "a { b: f(\n0, 1); }",
      Syntax::Css
    ),
    "a { b: f(\n0,\n1); }"
  );
}

#[test]
fn media_query_list_commas_are_in_media_params_only() {
  let rule = "@stylistic/media-query-list-comma-space-after";
  assert_eq!(
    problems(rule, "always", "@media a,b {}\n@supports (a,b) {}"),
    [(1, 9, "Expected single space after \",\"".into())]
  );
  let newline = "@stylistic/media-query-list-comma-newline-after";
  assert_eq!(problems(newline, "always", "@media a, /* x */\nb {}"), []);
  assert_eq!(
    problems(newline, "always", "@media a, /* x */ b {}"),
    [(1, 17, "Expected newline after \",\"".into())]
  );
  assert_eq!(
    fix(
      newline,
      json!("always"),
      "@media a,  \nb, c {}",
      Syntax::Css
    ),
    "@media a,\nb,\n c {}"
  );
}

#[test]
fn the_first_comment_of_function_arguments_is_left_out() {
  use super::without_first_comment;
  assert_eq!(without_first_comment("a /* b */ c /* d */"), "a c /* d */");
  assert_eq!(without_first_comment("a /* b */c"), "a /* b */c");
  assert_eq!(without_first_comment("a  // b\nc"), "a\nc");
  assert_eq!(without_first_comment("a / b"), "a / b");
}

#[test]
fn options_are_validated_like_the_plugin() {
  for (rule, valid) in [
    (
      "@stylistic/selector-list-comma-space-after",
      "never-single-line",
    ),
    ("@stylistic/value-list-comma-newline-before", "never"),
    (
      "@stylistic/function-comma-newline-after",
      "never-multi-line",
    ),
  ] {
    assert!(invalid_options(rule, json!(valid)).is_empty(), "{rule}");
  }
  assert_eq!(
    invalid_options("@stylistic/function-comma-newline-after", json!("never")),
    ["Invalid option value \"never\" for rule \"@stylistic/function-comma-newline-after\""]
  );
  assert!(
    invalid_options(
      "@stylistic/function-comma-space-after",
      json!(["always", { "ignoreFunctions": ["a"] }])
    )
    .is_empty()
  );
}

/// Fixes that leave a problem for the same rule: Stylelint fixes once and
/// gale fixes until nothing changes, which is where a second Stylelint
/// run ends too.
#[test]
fn fixes_settle_where_a_second_stylelint_run_would() {
  use crate::rules::spacing::harness::{bad, check};
  // Joining the lines puts the comment next to the comma; the next pass
  // takes the space after it (Stylelint's first run stops at
  // `,/* x */ inset`).
  check(
    "@stylistic/value-list-comma-newline-after",
    json!("never-multi-line"),
    Syntax::Css,
    &[
      bad("a {\n  box-shadow:\n    0 0 red,\n    /* x */ inset 0 0 blue;\n}")
        .at(
          3,
          12,
          "Unexpected whitespace after \",\" in a multi-line list",
        )
        .fixed("a {\n  box-shadow:\n    0 0 red,/* x */inset 0 0 blue;\n}"),
    ],
  );
  // After a trailing comma the whitespace is the `)`'s: the space the
  // plugin adds stays a problem, so gale adds it once, as Stylelint does.
  check(
    "@stylistic/function-comma-space-after",
    json!("always"),
    Syntax::Css,
    &[bad("a { b: f(a,\n); }")
      .at(1, 11, "Expected single space after \",\"")
      .fixed("a { b: f(a, \n); }")],
  );
  // Without a line break in the file the fixes use `\n`, as Stylelint's
  // `context.newline` does.
  check(
    "@stylistic/selector-list-comma-newline-after",
    json!("always"),
    Syntax::Css,
    &[bad("a,b {}")
      .at(1, 2, "Expected newline after \",\"")
      .fixed("a,\nb {}")],
  );
}

/// Gale's own `"never"` on `selector-list-comma-newline-after`, which the
/// plugin does not take: no line break after the comma, spaces aside.
#[test]
fn selector_list_comma_newline_after_never() {
  use crate::rules::spacing::harness::{bad, check, ok};
  check(
    "@stylistic/selector-list-comma-newline-after",
    json!("never"),
    Syntax::Css,
    &[
      ok("a, b {}"),
      ok("a,b {}"),
      bad("a,\nb {}").at(1, 2, "Unexpected newline after \",\""),
      bad("a,  \n  b {}").at(1, 2, "Unexpected newline after \",\""),
    ],
  );
}
