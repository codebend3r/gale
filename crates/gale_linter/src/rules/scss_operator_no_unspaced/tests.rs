use super::*;

use crate::testing::{
  CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
};

/// A rule `a` declaring `property: value`.
fn make_node(property: &str, value: &str) -> String {
  format!("a {{\n  {property}: {value};\n}}\n")
}

#[test]
fn skips_css_files() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "10px+5px");
  assert!(rule.check_source(&ctx_with_source(&node)).is_empty());
}
#[test]
fn reports_unspaced_plus() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "$a+$b");
  assert!(!rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn reports_unspaced_multiply() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "$a*2");
  assert!(!rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn allows_spaced_operators() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "$a + $b");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_unary_minus() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("margin", "-10px");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn reports_calc_operators_like_upstream() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "calc(100%+20px)");
  assert_eq!(rule.check_source(&scss_ctx_with_source(&node)).len(), 2);
}
#[test]
fn skips_font_shorthand_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("font", "14px/1.5 sans-serif");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_unary_after_operator() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "$a * -1");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_border_radius_shorthand_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("border-radius", "10px 5px / 20px 15px");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_border_radius_unspaced_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("border-radius", "10px/5px");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_grid_shorthand_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("grid", "auto-flow / 1fr 1fr 1fr");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_grid_column_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("grid-column", "1 / 3");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_grid_row_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("grid-row", "1/3");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_grid_area_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("grid-area", "1 / 1 / 3 / 3");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_grid_template_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("grid-template", "'a a' 100px / 1fr 1fr");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_background_shorthand_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("background", "url(img.png) center/cover no-repeat");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_list_style_shorthand_slash() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("list-style", "disc outside/inside");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_in_url() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node(
    "background-image",
    "url(https://example.com/path/to/image.png)",
  );
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_in_url_with_quotes() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node(
    "background-image",
    "url('https://example.com/path/to/image.png')",
  );
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_vendor_prefixed_border_radius() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("-webkit-border-radius", "10px/5px");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn still_reports_unspaced_slash_in_math() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "$a/2");
  assert!(!rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_inline_comment() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("box-shadow", "0 0 0 3px blue, // comment");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_negative_value_after_space() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("margin", "-10px");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  let node = make_node("top", "-2px");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  let node = make_node("box-shadow", "0 -2px red");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn flags_negative_variable_as_subtraction() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("box-shadow", "0 -$offset red");
  assert!(!rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn still_reports_unspaced_subtraction() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  // Upstream reads `$a-$b` as one hyphenated word, not a subtraction.
  let node = make_node("width", "$a-$b");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  let node = make_node("width", "$a -$b");
  assert_eq!(rule.check_source(&scss_ctx_with_source(&node)).len(), 1);
}
#[test]
fn still_reports_unspaced_operators_in_shorthand() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("border-radius", "$a+$b");
  assert!(!rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_separator_in_rgb() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "rgb(255 0 0 / 0.5)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn reports_unspaced_slash_alpha_separator_like_upstream() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "rgb(255 0 0/0.5)");
  assert_eq!(rule.check_source(&scss_ctx_with_source(&node)).len(), 2);
}
#[test]
fn skips_slash_alpha_separator_in_rgba() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "rgba(255 0 0 / 50%)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_separator_in_hsl() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "hsl(120 100% 50% / .5)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_separator_in_hsla() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "hsla(120 100% 50% / 0.5)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_separator_in_hwb() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "hwb(120 0% 0% / 0.5)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_separator_in_oklch() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "oklch(0.5 0.2 240 / 0.5)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_separator_in_rgb_with_var() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "rgb(from var(--color) r g b / 50%)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_separator_in_color_function() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("color", "color(srgb 1 0 0 / 0.5)");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn still_reports_unspaced_division_outside_color_fn() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("width", "$a/2");
  assert!(!rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}
#[test]
fn skips_slash_alpha_in_rgb_variable_assignment() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let source = "$color: rgb(255 0 0 / 0.5);\n";
  let ctx = scss_ctx_with_source(source);
  let diags = rule.check_source(&ctx);
  assert!(
    diags.is_empty(),
    "Expected no diagnostics for rgb alpha separator in variable assignment, got: {:?}",
    diags
  );
}

#[test]
fn skips_block_comments_in_values() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  // Block comment inside a value should not trigger on * or /
  let node = make_node("color", "red /* fallback */ blue");
  assert!(
    rule.check_source(&scss_ctx_with_source(&node)).is_empty(),
    "Block comment content should not trigger operator spacing"
  );
}

#[test]
fn skips_jsdoc_style_comments_at_source_level() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let source =
    "/**\n * @summary Focus state\n * @selector .slds-has-focus\n */\n.foo { color: red; }\n";
  let ctx = scss_ctx_with_source(source);
  let diags = rule.check_source(&ctx);
  assert!(
    diags.is_empty(),
    "JSDoc-style block comments should not trigger operator spacing, got: {:?}",
    diags
  );
}

#[test]
fn skips_block_comment_with_asterisks_in_value() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  // Simulates a value containing a block comment with star-prefixed lines
  let node = make_node("color", "/* * * * */ red");
  assert!(
    rule.check_source(&scss_ctx_with_source(&node)).is_empty(),
    "Stars inside block comments should not be flagged"
  );
}

#[test]
fn accepts_operator_at_end_of_line() {
  // Operator at end of line with value on next line — Stylelint allows this.
  // This pattern appears in multi-line SCSS expressions.
  let scss = "$x: 4 *\n  $y;";
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let ctx = scss_ctx_with_source(scss);
  let diags: Vec<_> = rule.check_source(&ctx);
  assert!(
    diags.is_empty(),
    "operator at end of line should not be flagged, got: {:?}",
    diags
  );
}

#[test]
fn accepts_comparison_operator_followed_by_newline() {
  // `== \n    'string'` pattern from _density.scss — the space before `==` is present,
  // and a newline follows — Stylelint does not flag this.
  let scss = "$x: $a ==\n  'string';";
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let ctx = scss_ctx_with_source(scss);
  let diags: Vec<_> = rule.check_source(&ctx);
  assert!(
    diags.is_empty(),
    "== followed by newline should not be flagged, got: {:?}",
    diags
  );
}

#[test]
fn checks_declarations_below_box_drawing_comments() {
  let scss = "// \u{2500}\u{2500} geometry \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n.a {\n  width: 10px;\n}";
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let diags = rule.check_source(&scss_ctx_with_source(scss));
  assert!(
    diags.is_empty(),
    "clean declaration should not be flagged, got: {:?}",
    diags
  );
}

#[test]
fn reports_each_unspaced_side_where_written() {
  let source = "a {\n  width: 1px+2px;\n  $b: 1+2;\n  margin: #{$x+1}px;\n  #{$p+1}-x: 1;\n}\n";
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let found: Vec<usize> = rule
    .check_source(&scss_ctx_with_source(source))
    .iter()
    .map(|d| d.span.offset)
    .collect();
  let plus = |line: &str| source.find(line).unwrap() + line.find('+').unwrap();
  assert_eq!(
    found,
    [
      plus("1px+2px"),
      plus("1px+2px"),
      plus("1+2"),
      plus("1+2"),
      plus("$x+1"),
      plus("$x+1"),
      plus("$p+1"),
      plus("$p+1"),
    ]
  );
}

#[test]
fn skips_private_variable_names() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  let node = make_node("padding-inline-end", "$-icon-size");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
  let node = make_node("width", "$a-$-b");
  assert!(rule.check_source(&scss_ctx_with_source(&node)).is_empty());
}

#[test]
fn skips_keywords_glued_to_variables() {
  let rule = configured(&ScssOperatorNoUnspaced, None);
  for value in ["-webkit-$value", "zoom-$value"] {
    let node = make_node("cursor", value);
    assert!(
      rule.check_source(&scss_ctx_with_source(&node)).is_empty(),
      "{value}"
    );
  }
  let node = make_node("width", "1-$b");
  assert_eq!(rule.check_source(&scss_ctx_with_source(&node)).len(), 2);
}

#[test]
fn options_are_validated_like_stylelint() {
  assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
  // Upstream validates no secondary options, so it ignores them all.
  assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
  assert_eq!(
    invalid_options(NAME, serde_json::json!("always")),
    ["Unexpected option value \"always\" for rule \"scss/operator-no-unspaced\""]
  );
}
