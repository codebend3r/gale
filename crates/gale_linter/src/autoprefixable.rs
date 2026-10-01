//! Port of Stylelint's `isAutoprefixable`: the vendor-prefixed at-rules,
//! selectors, properties and values that Autoprefixer would add back, and
//! so the ones the `*-no-vendor-prefix` rules report and unprefix.

/// Whether `name` (without the `@`) is a prefixed at-rule Autoprefixer
/// handles, such as `-webkit-keyframes`.
pub fn at_rule_name(name: &str) -> bool {
  lookup(AT_RULES, &format!("@{}", name.to_ascii_lowercase()))
}

/// Whether `pseudo` (with its colons, e.g. `::-moz-placeholder`) is a
/// prefixed pseudo-class or pseudo-element Autoprefixer handles.
pub fn selector(pseudo: &str) -> bool {
  lookup(SELECTORS, pseudo)
}

/// Whether `name` is a prefixed media feature Autoprefixer handles: any
/// that mentions `device-pixel-ratio`.
pub fn media_feature_name(name: &str) -> bool {
  name.to_ascii_lowercase().contains("device-pixel-ratio")
}

/// Whether `prop` is a vendor-prefixed property whose unprefixed form
/// Autoprefixer handles.
pub fn property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  let prefix_len = vendor_prefix_len(&lower);
  prefix_len > 0 && lookup(PROPERTIES, &lower[prefix_len..])
}

/// Whether `value` is a vendor-prefixed keyword or function name
/// Autoprefixer handles, such as `-webkit-linear-gradient`.
pub fn property_value(value: &str) -> bool {
  lookup(PROPERTY_VALUES, value)
}

/// `identifier` without its first vendor prefix, as Stylelint's
/// `unprefix` (`value.replace(/-\w+-/, '')`) strips it: the first `-`, one
/// or more word characters and a `-`, wherever they occur.
pub fn unprefix(identifier: &str) -> String {
  (0..identifier.len())
    .find_map(|start| prefix_end(identifier.as_bytes(), start).map(|end| (start, end)))
    .map_or_else(
      || identifier.to_string(),
      |(start, end)| format!("{}{}", &identifier[..start], &identifier[end..]),
    )
}

/// Byte length of the vendor prefix at the start of `prop`, as Stylelint's
/// `vendor.prefix` matches it (`/^(-\w+-)/`), or 0 when there is none.
pub fn vendor_prefix_len(prop: &str) -> usize {
  prefix_end(prop.as_bytes(), 0).unwrap_or(0)
}

/// The exclusive end of a `-\w+-` match starting at `start`, if one does.
/// `\w+` stops at the first non-word byte, which has to be the closing `-`;
/// backtracking could only end it before a word byte, so it never helps.
fn prefix_end(bytes: &[u8], start: usize) -> Option<usize> {
  if bytes.get(start) != Some(&b'-') {
    return None;
  }
  let mut end = start + 1;
  while end < bytes.len() && is_word_byte(bytes[end]) {
    end += 1;
  }
  (end > start + 1 && bytes.get(end) == Some(&b'-')).then_some(end + 1)
}

/// Whether `b` is a JavaScript regex word character (`\w`).
fn is_word_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'_'
}

/// Case-insensitive lookup in a sorted, lowercase table.
fn lookup(table: &[&str], name: &str) -> bool {
  table
    .binary_search(&name.to_ascii_lowercase().as_str())
    .is_ok()
}

/// Prefixed at-rules, with their `@`.
static AT_RULES: &[&str] = &[
  "@-moz-document",
  "@-moz-keyframes",
  "@-ms-keyframes",
  "@-ms-viewport",
  "@-o-keyframes",
  "@-o-viewport",
  "@-webkit-keyframes",
  "@-webkit-viewport",
];

/// Prefixed pseudo-classes and pseudo-elements, with their colons.
static SELECTORS: &[&str] = &[
  ":-moz-any-link",
  ":-moz-full-screen",
  ":-moz-placeholder",
  ":-moz-placeholder-shown",
  ":-moz-read-only",
  ":-moz-read-write",
  ":-ms-fullscreen",
  ":-ms-input-placeholder",
  ":-webkit-any-link",
  ":-webkit-full-screen",
  "::-moz-placeholder",
  "::-moz-selection",
  "::-ms-input-placeholder",
  "::-webkit-backdrop",
  "::-webkit-input-placeholder",
];

/// Unprefixed names of the properties Autoprefixer prefixes.
static PROPERTIES: &[&str] = &[
  "align-content",
  "align-items",
  "align-self",
  "animation",
  "animation-delay",
  "animation-direction",
  "animation-duration",
  "animation-fill-mode",
  "animation-iteration-count",
  "animation-name",
  "animation-play-state",
  "animation-timing-function",
  "appearance",
  "backdrop-filter",
  "backface-visibility",
  "background-clip",
  "background-origin",
  "background-size",
  "border-block-end",
  "border-block-start",
  "border-bottom-left-radius",
  "border-bottom-right-radius",
  "border-image",
  "border-inline-end",
  "border-inline-start",
  "border-radius",
  "border-top-left-radius",
  "border-top-right-radius",
  "box-decoration-break",
  "box-shadow",
  "box-sizing",
  "break-after",
  "break-before",
  "break-inside",
  "clip-path",
  "color-adjust",
  "column-count",
  "column-fill",
  "column-gap",
  "column-rule",
  "column-rule-color",
  "column-rule-style",
  "column-rule-width",
  "column-span",
  "column-width",
  "columns",
  "filter",
  "flex",
  "flex-basis",
  "flex-direction",
  "flex-flow",
  "flex-grow",
  "flex-shrink",
  "flex-wrap",
  "flow-from",
  "flow-into",
  "font-feature-settings",
  "font-kerning",
  "font-language-override",
  "font-variant-ligatures",
  "grid-area",
  "grid-column",
  "grid-column-align",
  "grid-column-end",
  "grid-column-start",
  "grid-row",
  "grid-row-align",
  "grid-row-end",
  "grid-row-start",
  "grid-template",
  "grid-template-areas",
  "grid-template-columns",
  "grid-template-rows",
  "hyphens",
  "image-rendering",
  "justify-content",
  "margin-block-end",
  "margin-block-start",
  "margin-inline-end",
  "margin-inline-start",
  "mask",
  "mask-border",
  "mask-border-outset",
  "mask-border-repeat",
  "mask-border-slice",
  "mask-border-source",
  "mask-border-width",
  "mask-clip",
  "mask-composite",
  "mask-image",
  "mask-origin",
  "mask-position",
  "mask-repeat",
  "mask-size",
  "object-fit",
  "object-position",
  "order",
  "overscroll-behavior",
  "padding-block-end",
  "padding-block-start",
  "padding-inline-end",
  "padding-inline-start",
  "perspective",
  "perspective-origin",
  "place-self",
  "region-fragment",
  "scroll-snap-coordinate",
  "scroll-snap-destination",
  "scroll-snap-points-x",
  "scroll-snap-points-y",
  "scroll-snap-type",
  "shape-image-threshold",
  "shape-margin",
  "shape-outside",
  "tab-size",
  "text-align-last",
  "text-decoration",
  "text-decoration-color",
  "text-decoration-line",
  "text-decoration-skip",
  "text-decoration-skip-ink",
  "text-decoration-style",
  "text-emphasis",
  "text-emphasis-color",
  "text-emphasis-position",
  "text-emphasis-style",
  "text-orientation",
  "text-overflow",
  "text-size-adjust",
  "text-spacing",
  "touch-action",
  "transform",
  "transform-origin",
  "transform-style",
  "transition",
  "transition-delay",
  "transition-duration",
  "transition-property",
  "transition-timing-function",
  "user-select",
  "writing-mode",
];

/// Prefixed keywords and function names.
static PROPERTY_VALUES: &[&str] = &[
  "-moz-all",
  "-moz-arabic-indic",
  "-moz-bengali",
  "-moz-calc",
  "-moz-cjk-earthly-branch",
  "-moz-cjk-heavenly-stem",
  "-moz-crisp-edges",
  "-moz-devanagari",
  "-moz-element",
  "-moz-ethiopic-numeric",
  "-moz-fit-content",
  "-moz-grab",
  "-moz-grabbing",
  "-moz-gujarati",
  "-moz-gurmukhi",
  "-moz-hangul",
  "-moz-hangul-consonant",
  "-moz-initial",
  "-moz-isolate",
  "-moz-isolate-override",
  "-moz-japanese-formal",
  "-moz-japanese-informal",
  "-moz-kannada",
  "-moz-khmer",
  "-moz-lao",
  "-moz-linear-gradient",
  "-moz-malayalam",
  "-moz-max-content",
  "-moz-min-content",
  "-moz-myanmar",
  "-moz-oriya",
  "-moz-persian",
  "-moz-plaintext",
  "-moz-pre-wrap",
  "-moz-radial-gradient",
  "-moz-repeating-linear-gradient",
  "-moz-repeating-radial-gradient",
  "-moz-simp-chinese-formal",
  "-moz-simp-chinese-informal",
  "-moz-tamil",
  "-moz-telugu",
  "-moz-thai",
  "-moz-trad-chinese-formal",
  "-moz-trad-chinese-informal",
  "-moz-zoom-in",
  "-moz-zoom-out",
  "-ms-flexbox",
  "-ms-grid",
  "-ms-inline-grid",
  "-ms-linear-gradient",
  "-ms-radial-gradient",
  "-ms-repeating-linear-gradient",
  "-ms-repeating-radial-gradient",
  "-o-crisp-edges",
  "-o-linear-gradient",
  "-o-pre-wrap",
  "-o-radial-gradient",
  "-o-repeating-linear-gradient",
  "-o-repeating-radial-gradient",
  "-webkit-calc",
  "-webkit-cross-fade",
  "-webkit-filter",
  "-webkit-fit-content",
  "-webkit-flex",
  "-webkit-grab",
  "-webkit-grabbing",
  "-webkit-image-set",
  "-webkit-inline-flex",
  "-webkit-isolate",
  "-webkit-linear-gradient",
  "-webkit-max-content",
  "-webkit-min-content",
  "-webkit-plaintext",
  "-webkit-radial-gradient",
  "-webkit-repeating-linear-gradient",
  "-webkit-repeating-radial-gradient",
  "-webkit-sticky",
  "-webkit-zoom-in",
  "-webkit-zoom-out",
  "-xv-digits",
  "-xv-literal-punctuation",
  "-xv-no-punctuation",
];

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tables_are_sorted_for_binary_search() {
    for table in [AT_RULES, SELECTORS, PROPERTIES, PROPERTY_VALUES] {
      assert!(table.windows(2).all(|w| w[0] < w[1]));
    }
  }

  #[test]
  fn recognises_prefixed_names_in_any_case() {
    assert!(property("-WEBKIT-tranSFoRM"));
    assert!(!property("-webkit-touch-callout"));
    assert!(!property("transform"));
    assert!(property_value("-Webkit-Linear-Gradient"));
    assert!(selector("::-MOZ-placeholder"));
    assert!(at_rule_name("-webkit-keyframes"));
    assert!(media_feature_name("-webkit-min-device-pixel-ratio"));
  }

  #[test]
  fn unprefix_strips_the_first_prefix_and_keeps_case() {
    assert_eq!(unprefix("-WEBKIT-tranSFoRM"), "tranSFoRM");
    assert_eq!(unprefix("::-moz-placeholder"), "::placeholder");
    assert_eq!(
      unprefix("min--moz-device-pixel-ratio"),
      "min-device-pixel-ratio"
    );
    assert_eq!(unprefix("plain"), "plain");
    assert_eq!(vendor_prefix_len("-o-columns"), 3);
    assert_eq!(vendor_prefix_len("--custom"), 0);
  }
}
