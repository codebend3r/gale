//! At-rule names, and the media features `@media` params name.

use gale_css_parser::Syntax;

use super::lookup;

/// Whether `name` is a standard CSS at-rule (without the `@`).
pub fn is_known_at_rule(name: &str) -> bool {
  lookup(KNOWN_AT_RULES, name)
}

/// Returns `true` when the at-rule name is valid for the given syntax.
///
/// - `Syntax::Css` — only standard CSS at-rules.
/// - `Syntax::Scss` / `Syntax::Sass` — CSS at-rules plus SCSS-specific ones.
/// - `Syntax::Less` — CSS at-rules plus Less-specific ones.
pub fn is_known_at_rule_for_syntax(name: &str, syntax: Syntax) -> bool {
  if is_known_at_rule(name) {
    return true;
  }
  match syntax {
    Syntax::Scss | Syntax::Sass => lookup(KNOWN_SCSS_AT_RULES, name),
    Syntax::Less => lookup(KNOWN_LESS_AT_RULES, name),
    Syntax::Css => false,
  }
}

/// Whether `name` is a standard CSS media feature.
pub fn is_known_media_feature(name: &str) -> bool {
  lookup(KNOWN_MEDIA_FEATURES, name)
}

static KNOWN_AT_RULES: &[&str] = &[
  "charset",
  "color-profile",
  "container",
  "counter-style",
  "document",
  "font-face",
  "font-feature-values",
  "font-palette-values",
  "function",
  "import",
  "keyframes",
  "layer",
  "media",
  "namespace",
  "page",
  "position-try",
  "property",
  "scope",
  "starting-style",
  "supports",
  "view-transition",
  "viewport",
];

/// SCSS / Sass at-rules.
static KNOWN_SCSS_AT_RULES: &[&str] = &[
  "at-root", "content", "debug", "each", "else", "else if", "error", "extend", "for", "forward",
  "function", "if", "include", "mixin", "return", "use", "warn", "while",
];

/// Less at-rules.
static KNOWN_LESS_AT_RULES: &[&str] = &["detached-ruleset", "plugin"];

static KNOWN_MEDIA_FEATURES: &[&str] = &[
  "any-hover",
  "any-pointer",
  "aspect-ratio",
  "color",
  "color-gamut",
  "color-index",
  "device-aspect-ratio",
  "device-height",
  "device-width",
  "display-mode",
  "dynamic-range",
  "environment-blending",
  "forced-colors",
  "grid",
  "height",
  "horizontal-viewport-segments",
  "hover",
  "inverted-colors",
  "max-aspect-ratio",
  "max-color",
  "max-color-index",
  "max-device-aspect-ratio",
  "max-device-height",
  "max-device-width",
  "max-height",
  "max-monochrome",
  "max-resolution",
  "max-width",
  "min-aspect-ratio",
  "min-color",
  "min-color-index",
  "min-device-aspect-ratio",
  "min-device-height",
  "min-device-width",
  "min-height",
  "min-monochrome",
  "min-resolution",
  "min-width",
  "monochrome",
  "nav-controls",
  "orientation",
  "overflow-block",
  "overflow-inline",
  "pointer",
  "prefers-color-scheme",
  "prefers-contrast",
  "prefers-reduced-data",
  "prefers-reduced-motion",
  "prefers-reduced-transparency",
  "resolution",
  "scan",
  "scripting",
  "update",
  "vertical-viewport-segments",
  "video-color-gamut",
  "video-dynamic-range",
  "width",
];

#[cfg(test)]
mod tests {
  use super::*;
  use crate::data::assert_sorted;

  #[test]
  fn known_at_rules() {
    assert!(is_known_at_rule("media"));
    assert!(is_known_at_rule("keyframes"));
    assert!(is_known_at_rule("import"));
    assert!(!is_known_at_rule("tailwind"));
  }

  #[test]
  fn known_at_rules_for_scss() {
    assert!(is_known_at_rule_for_syntax("media", Syntax::Scss));
    assert!(is_known_at_rule_for_syntax("mixin", Syntax::Scss));
    assert!(is_known_at_rule_for_syntax("include", Syntax::Scss));
    assert!(is_known_at_rule_for_syntax("if", Syntax::Scss));
    assert!(is_known_at_rule_for_syntax("each", Syntax::Scss));
    assert!(is_known_at_rule_for_syntax("use", Syntax::Scss));
    assert!(is_known_at_rule_for_syntax("forward", Syntax::Scss));
    assert!(is_known_at_rule_for_syntax("at-root", Syntax::Scss));
    assert!(!is_known_at_rule_for_syntax("mixin", Syntax::Css));
    assert!(!is_known_at_rule_for_syntax("plugin", Syntax::Scss));
  }

  #[test]
  fn known_at_rules_for_less() {
    assert!(is_known_at_rule_for_syntax("media", Syntax::Less));
    assert!(is_known_at_rule_for_syntax("plugin", Syntax::Less));
    assert!(is_known_at_rule_for_syntax(
      "detached-ruleset",
      Syntax::Less
    ));
    assert!(!is_known_at_rule_for_syntax("plugin", Syntax::Css));
    assert!(!is_known_at_rule_for_syntax("mixin", Syntax::Less));
  }

  #[test]
  fn known_media_features() {
    assert!(is_known_media_feature("width"));
    assert!(is_known_media_feature("min-width"));
    assert!(is_known_media_feature("hover"));
    assert!(is_known_media_feature("prefers-color-scheme"));
    assert!(!is_known_media_feature("fake-feature"));
  }

  #[test]
  fn tables_are_sorted() {
    assert_sorted(KNOWN_AT_RULES, "KNOWN_AT_RULES");
    assert_sorted(KNOWN_SCSS_AT_RULES, "KNOWN_SCSS_AT_RULES");
    assert_sorted(KNOWN_LESS_AT_RULES, "KNOWN_LESS_AT_RULES");
    assert_sorted(KNOWN_MEDIA_FEATURES, "KNOWN_MEDIA_FEATURES");
  }
}
