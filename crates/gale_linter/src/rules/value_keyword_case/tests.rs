use super::*;
use gale_css_parser::Syntax;

use crate::testing::{CheckSource, configured, context, fix, invalid_options};

/// The diagnostics for `source` in `syntax` with `options`.
fn lint(source: &str, syntax: Syntax, options: Option<&serde_json::Value>) -> Vec<Diagnostic> {
  let ctx = context(source, syntax);
  configured(&ValueKeywordCase, options.cloned()).check_source(&ctx)
}

/// The diagnostics for `a { <prop>: <value>; }` with no options.
fn decl(prop: &str, value: &str) -> Vec<Diagnostic> {
  lint(&format!("a {{ {prop}: {value}; }}"), Syntax::Css, None)
}

#[test]
fn reports_uppercase_keyword() {
  let d = decl("display", "BLOCK");
  assert_eq!(d.len(), 1);
  assert_eq!(d[0].message, "Expected \"BLOCK\" to be \"block\"");
  assert_eq!(d[0].span.offset, 13);
  assert!(d[0].fix.is_some());
}

#[test]
fn allows_lowercase_keyword() {
  assert!(decl("display", "block").is_empty());
}

#[test]
fn reports_mixed_case() {
  let d = decl("color", "Inherit");
  assert_eq!(d.len(), 1);
  assert!(d[0].message.contains("\"Inherit\""));
}

#[test]
fn skips_urls_strings_and_animation_names() {
  assert!(decl("background", "url(BLOCK)").is_empty());
  assert!(decl("content", "\"BLOCK\"").is_empty());
  assert!(decl("animation-name", "ANIMATION-NAME").is_empty());
  assert!(decl("font-family", "Gill Sans Extrabold").is_empty());
}

#[test]
fn skips_system_colors() {
  // Stylelint does not enforce lowercase for CSS system colors (e.g. GrayText, ButtonText).
  assert!(decl("color", "InactiveCaptionText").is_empty());
  assert!(decl("color", "-moz-NativeHyperlinkText").is_empty());
}

#[test]
fn reports_font_family_generic() {
  assert_eq!(decl("font-family", "MONOSPACE").len(), 1);
}

#[test]
fn current_color_follows_camel_case_svg_keywords() {
  // `lower` without camelCaseSvgKeywords wants `currentcolor`.
  let d = decl("color", "currentColor");
  assert_eq!(d.len(), 1);
  assert!(d[0].message.contains("currentcolor"));
  assert_eq!(decl("border-color", "currentColor").len(), 1);
  assert!(decl("color", "currentcolor").is_empty());
  let camel = serde_json::json!(["lower", { "camelCaseSvgKeywords": true }]);
  assert!(lint("a { color: currentColor; }", Syntax::Css, Some(&camel)).is_empty());
  assert_eq!(
    lint("a { color: currentcolor; }", Syntax::Css, Some(&camel)).len(),
    1
  );
}

#[test]
fn reports_georgia_in_custom_property() {
  let source = ":root {\n  --font-serif: var(--font-roboto-serif), ui-serif, Georgia;\n}";
  let d = lint(source, Syntax::Css, None);
  assert!(d.iter().any(|diag| diag.message.contains("Georgia")));
}

#[test]
fn checks_every_declaration_postcss_sees() {
  // Directly inside an at-rule, and with a value the CSS parser rejects.
  assert_eq!(
    lint("@media (min-width: 1px) { color: Red; }", Syntax::Css, None).len(),
    1
  );
  let d = lint("a { display: block !Import; }", Syntax::Css, None);
  assert_eq!(d.len(), 1);
  assert_eq!(d[0].message, "Expected \"!Import\" to be \"!import\"");
  assert_eq!(d[0].span.offset, 19);
  // SCSS variables and nested properties are declarations too.
  let scss = "$c: Red;\na { font: { family: Arial; } }";
  assert_eq!(lint(scss, Syntax::Scss, None).len(), 2);
  // `!important` is not part of the value, and a variable value is skipped.
  assert!(lint("a { color: red !IMPORTANT; top: $X; }", Syntax::Scss, None).is_empty());
}

#[test]
fn fix_rewrites_the_keyword_in_place() {
  assert_eq!(
    fix(
      "value-keyword-case",
      serde_json::json!("lower"),
      "@media screen { color: GREEN; @media (min-width: 1px) { color: Red !important; } }",
      Syntax::Css,
    ),
    "@media screen { color: green; @media (min-width: 1px) { color: red !important; } }"
  );
}

#[test]
fn flags_uppercase_ident_after_triple_slash_in_value() {
  // PostCSS-SCSS does NOT treat `///` (triple-slash Sass doc comments) as line comments.
  // Identifiers after `///` remain visible to value-keyword-case.
  let tokens = tokenize_value("(\n  /// Prevent blah\n  inherit\n)");
  let ident_tokens: Vec<_> = tokens
    .iter()
    .filter(|t| t.kind == TokenKind::Ident)
    .collect();
  let texts: Vec<&str> = ident_tokens.iter().map(|t| t.text.as_str()).collect();
  assert!(
    texts.contains(&"Prevent"),
    "Prevent after /// should be tokenized as Ident (/// is not a comment), got: {:?}",
    texts
  );
  assert!(
    texts.contains(&"inherit"),
    "inherit should be tokenized as Ident, got: {:?}",
    texts
  );
}

#[test]
fn skips_double_slash_comment_in_value() {
  // Regular `//` SCSS comments ARE skipped by the tokenizer.
  // Identifiers after `//` should NOT be flagged.
  let tokens = tokenize_value("(\n  // Prevent blah\n  inherit\n)");
  let ident_tokens: Vec<_> = tokens
    .iter()
    .filter(|t| t.kind == TokenKind::Ident)
    .collect();
  let texts: Vec<&str> = ident_tokens.iter().map(|t| t.text.as_str()).collect();
  assert!(
    !texts.contains(&"Prevent"),
    "Prevent after // should be skipped (// is a comment), got: {:?}",
    texts
  );
  assert!(
    texts.contains(&"inherit"),
    "inherit after // comment should still be tokenized, got: {:?}",
    texts
  );
}

#[test]
fn options_are_validated_like_stylelint() {
  assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
  let all = serde_json::json!(["lower", {
    "ignoreProperties": ["/^my-/"],
    "ignoreKeywords": ["Block"],
    "ignoreFunctions": ["url"],
    "camelCaseSvgKeywords": true
  }]);
  assert!(invalid_options(NAME, all).is_empty());
  assert_eq!(
    invalid_options(NAME, serde_json::json!("Lower")),
    ["Invalid option value \"Lower\" for rule \"value-keyword-case\""]
  );
  assert_eq!(
    invalid_options(
      NAME,
      serde_json::json!(["lower", {"camelCaseSvgKeywords": "yes", "ignoreValues": ["x"]}])
    ),
    [
      "Invalid value \"yes\" for option \"camelCaseSvgKeywords\" of rule \"value-keyword-case\"",
      "Invalid option name \"ignoreValues\" for rule \"value-keyword-case\""
    ]
  );
}
