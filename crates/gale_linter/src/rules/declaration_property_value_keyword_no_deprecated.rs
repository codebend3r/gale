use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_value;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Stylelint's `DEPRECATED_COLORS`: deprecated system colors and their
/// replacements.
const DEPRECATED_COLORS: &[(&str, &str)] = &[
  ("activecaption", "canvas"),
  ("appworkspace", "canvas"),
  ("background", "canvas"),
  ("inactivecaption", "canvas"),
  ("infobackground", "canvas"),
  ("menu", "canvas"),
  ("scrollbar", "canvas"),
  ("window", "canvas"),
  ("activeborder", "ButtonBorder"),
  ("inactiveborder", "ButtonBorder"),
  ("threeddarkshadow", "ButtonBorder"),
  ("threedhighlight", "ButtonBorder"),
  ("threedlightshadow", "ButtonBorder"),
  ("threedshadow", "ButtonBorder"),
  ("windowframe", "ButtonBorder"),
  ("captiontext", "CanvasText"),
  ("infotext", "CanvasText"),
  ("menutext", "CanvasText"),
  ("windowtext", "CanvasText"),
  ("buttonhighlight", "ButtonFace"),
  ("buttonshadow", "ButtonFace"),
  ("threedface", "ButtonFace"),
  ("inactivecaptiontext", "GrayText"),
];

/// Stylelint's `singleValueColorProperties` plus the multi-value color
/// properties it maps to the deprecated colors.
const COLOR_PROPERTIES: &[&str] = &[
  "accent-color",
  "background-color",
  "border-block-color",
  "border-block-end-color",
  "border-block-start-color",
  "border-bottom-color",
  "border-color",
  "border-inline-color",
  "border-inline-end-color",
  "border-inline-start-color",
  "border-left-color",
  "border-right-color",
  "border-top-color",
  "caret-color",
  "color",
  "column-rule-color",
  "flood-color",
  "lighting-color",
  "outline-color",
  "scrollbar-color",
  "stop-color",
  "text-decoration-color",
  "text-emphasis-color",
];

/// Stylelint's `colorFunctions`, whose arguments are checked too.
const COLOR_FUNCTIONS: &[&str] = &[
  "color",
  "color-contrast",
  "color-mix",
  "contrast-color",
  "device-cmyk",
  "hsl",
  "hsla",
  "hwb",
  "lab",
  "lch",
  "light-dark",
  "oklab",
  "oklch",
  "rgb",
  "rgba",
];

/// What a property's deprecated keywords are.
enum Keywords {
  /// Keywords with a replacement, as Stylelint's object maps hold them.
  /// Keys are compared with the lower-cased keyword, so Stylelint's
  /// camel-case keys (`optimizeQuality`) never match, and neither do these.
  Replaced(&'static [(&'static str, &'static str)]),
  /// Keywords without a replacement.
  Rejected(&'static [&'static str]),
}

/// Stylelint's `PROPERTY_NAME_TO_KEYWORDS`, for a lower-case property.
fn keywords_for(property: &str) -> Option<Keywords> {
  if COLOR_PROPERTIES.contains(&property) {
    return Some(Keywords::Replaced(DEPRECATED_COLORS));
  }
  Some(match property {
    "appearance" => Keywords::Replaced(&[
      ("button", "auto"),
      ("checkbox", "auto"),
      ("listbox", "auto"),
      ("menulist", "auto"),
      ("meter", "auto"),
      ("progress-bar", "auto"),
      ("push-button", "auto"),
      ("radio", "auto"),
      ("searchfield", "auto"),
      ("slider-horizontal", "auto"),
      ("square-button", "auto"),
      ("textarea", "auto"),
    ]),
    "image-rendering" => Keywords::Replaced(&[
      ("optimizeQuality", "smooth"),
      ("optimizeSpeed", "pixelated"),
    ]),
    "overflow" | "overflow-x" | "overflow-y" => Keywords::Replaced(&[("overlay", "auto")]),
    "text-justify" => Keywords::Replaced(&[("distribute", "inter-character")]),
    "text-orientation" => Keywords::Replaced(&[("sideways-right", "sideways")]),
    "user-select" => Keywords::Replaced(&[("element", "contain")]),
    "zoom" => Keywords::Replaced(&[("reset", "1")]),
    "text-decoration" | "text-decoration-line" => Keywords::Rejected(&["blink"]),
    "box-sizing" => Keywords::Rejected(&["padding-box"]),
    "image-orientation" => Keywords::Rejected(&["flip"]),
    "min-height" | "min-width" | "max-height" | "max-width" | "height" | "width" => {
      Keywords::Rejected(&["intrinsic", "min-intrinsic"])
    }
    "word-break" => Keywords::Rejected(&["break-word"]),
    _ => return None,
  })
}

/// Stylelint's `mayIncludeRegexes.keyword` (`/(?<![0-9])[a-z]+(?!\()/i`):
/// a letter not after a digit that starts a run of letters not directly
/// followed by `(`.
fn may_include_keyword(value: &str) -> bool {
  let bytes = value.as_bytes();
  // A one-letter run is the shortest the regex can take, so it only fails
  // when the letter is directly followed by `(`.
  (0..bytes.len()).any(|i| {
    bytes[i].is_ascii_alphabetic()
      && (i == 0 || !bytes[i - 1].is_ascii_digit())
      && bytes.get(i + 1) != Some(&b'(')
  })
}

const NAME: &str = "declaration-property-value-keyword-no-deprecated";

declare_rule! {
  /// Disallow deprecated keywords for properties.
  ///
  /// Equivalent to Stylelint's `declaration-property-value-keyword-no-deprecated`
  /// rule: deprecated keywords with a replacement (`overflow: overlay`,
  /// system colors such as `InactiveCaptionText`) are fixed to it in place;
  /// those without one (`text-decoration: blink`) are reported unfixed.
  pub struct DeclarationPropertyValueKeywordNoDeprecated => DeclarationPropertyValueKeywordNoDeprecatedCheck {
    name: NAME,
    description: "Disallow deprecated keyword values for properties",
    default_severity: Warning,
  }
}

/// `declaration-property-value-keyword-no-deprecated` as configured.
pub struct DeclarationPropertyValueKeywordNoDeprecatedCheck {
  /// `ignoreKeywords`, matched against the whole value.
  ignore_keywords: Matchers,
}

impl DeclarationPropertyValueKeywordNoDeprecatedCheck {
  /// Stylelint's options: `true`, then `ignoreKeywords`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_keywords = options.matchers("ignoreKeywords");
    options.finish()?;
    Ok(Self { ignore_keywords })
  }
}

impl Check for DeclarationPropertyValueKeywordNoDeprecatedCheck {
  /// Flags deprecated value keywords, skipping values that match
  /// `ignoreKeywords`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for &decl in ctx.written_declarations(node).iter() {
      if !may_include_keyword(decl.value) {
        continue;
      }
      // Sass and Less variables are not properties.
      if decl.prop.starts_with('$') || (decl.prop.starts_with('@') && !decl.prop.starts_with("@{"))
      {
        continue;
      }
      // Stylelint matches the whole value against `ignoreKeywords`.
      if self.ignore_keywords.matches(decl.value) {
        continue;
      }
      let Some(keywords) = keywords_for(&decl.prop.to_ascii_lowercase()) else {
        continue;
      };
      let nodes = value_parser::parse(decl.value);
      let mut standard = true;
      value_parser::walk(&nodes, &mut |node: &ValueNode| {
        if node.kind != NodeKind::Comment && !is_standard_syntax_value(node.value) {
          standard = false;
        }
        true
      });
      if !standard {
        continue;
      }
      value_parser::walk(&nodes, &mut |node: &ValueNode| {
        let lower = node.value.to_ascii_lowercase();
        if node.kind == NodeKind::Function {
          return COLOR_FUNCTIONS.contains(&lower.as_str());
        }
        if node.kind != NodeKind::Word {
          return true;
        }
        let span = Span::new(decl.value_start + node.source_index, node.value.len());
        match &keywords {
          Keywords::Rejected(rejected) if rejected.contains(&lower.as_str()) => {
            diags.push(
              Diagnostic::new(
                NAME,
                format!(
                  "Unexpected deprecated keyword \"{}\" for property \"{}\"",
                  node.value, decl.prop
                ),
              )
              .span(span),
            );
          }
          Keywords::Replaced(replaced) => {
            if let Some((_, expected)) = replaced.iter().find(|(key, _)| *key == lower) {
              diags.push(
                Diagnostic::new(
                  NAME,
                  format!("Expected \"{}\" to be \"{expected}\"", node.value),
                )
                .span(span)
                .fix(Fix::new(
                  format!("Replace with \"{expected}\""),
                  vec![Edit::new(span, *expected)],
                )),
              );
            }
          }
          Keywords::Rejected(_) => {}
        }
        true
      });
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::NAME;
  use gale_css_parser::Syntax;

  use crate::testing::{fix, invalid_options, lint};

  const RULE: &str = "declaration-property-value-keyword-no-deprecated";

  #[test]
  fn replaces_keywords_that_have_a_replacement() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { appearance: searchfield; }",
        Syntax::Css
      ),
      "a { appearance: auto; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { color: CoLoR(from InactiveCaptionText srgb r g b / 0.5); }",
        Syntax::Css
      ),
      "a { color: CoLoR(from GrayText srgb r g b / 0.5); }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { scrollbar-color: --foo(background, bar) menu; }",
        Syntax::Css
      ),
      "a { scrollbar-color: --foo(background, bar) canvas; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { zOom: /*qux*/reset/*baz*/; }",
        Syntax::Css
      ),
      "a { zOom: /*qux*/1/*baz*/; }"
    );
    let warnings = lint(RULE, on, "a { overflow: hidden overlay; }", Syntax::Css);
    assert_eq!(warnings[0].message, "Expected \"overlay\" to be \"auto\"");
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (21, 7));
  }

  #[test]
  fn reports_keywords_without_a_replacement_unfixed() {
    let on = serde_json::json!(true);
    let warnings = lint(
      RULE,
      on.clone(),
      "a { text-decoration: foo blink bar; }",
      Syntax::Css,
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].fix.is_none());
    for css in [
      "a { color: red; }",
      "a { color: --foo(background); }",
      "a { image-rendering: optimizeSpeed; }",
    ] {
      assert!(lint(RULE, on.clone(), css, Syntax::Css).is_empty(), "{css}");
    }
    assert!(
      lint(
        RULE,
        on,
        "$p: x; a { background-color: menu + $p; }",
        Syntax::Scss
      )
      .is_empty()
    );
  }

  #[test]
  fn ignore_keywords_matches_the_whole_value() {
    let options = serde_json::json!([true, { "ignoreKeywords": ["/intrinsic$/", "padding-box"] }]);
    assert!(
      lint(
        RULE,
        options.clone(),
        "a { box-sizing: padding-box; }",
        Syntax::Css
      )
      .is_empty()
    );
    assert!(
      lint(
        RULE,
        options.clone(),
        "a { width: min-intrinsic; }",
        Syntax::Css
      )
      .is_empty()
    );
    assert_eq!(
      lint(RULE, options, "a { box-sizing: PADDING-BOX; }", Syntax::Css).len(),
      1
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    let css = "a { overflow: overlay; }";
    assert_eq!(lint(RULE, options.clone(), css, Syntax::Css).len(), 1);
    assert_eq!(fix(RULE, options, css, Syntax::Css), css);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreKeywords": ["/^search/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"declaration-property-value-keyword-no-deprecated\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreKeywords": [1] }])),
      [
        "Invalid value \"1\" for option \"ignoreKeywords\" of rule \"declaration-property-value-keyword-no-deprecated\""
      ]
    );
  }
}
