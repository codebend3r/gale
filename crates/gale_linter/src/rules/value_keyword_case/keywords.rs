//! The keyword tables `value-keyword-case` decides with: system colors,
//! SVG's camelCase keywords, and the properties whose values mix keywords
//! with author-chosen identifiers.

use crate::stylelint_version::{installed_at_least, stylelint_major_version};

/// CSS2 system colors (deprecated) — used by Stylelint ≤13.
const SYSTEM_COLORS_CSS2: &[&str] = &[
  "ActiveBorder",
  "ActiveCaption",
  "AppWorkspace",
  "Background",
  "ButtonFace",
  "ButtonHighlight",
  "ButtonShadow",
  "ButtonText",
  "CaptionText",
  "GrayText",
  "Highlight",
  "HighlightText",
  "InactiveBorder",
  "InactiveCaption",
  "InactiveCaptionText",
  "InfoBackground",
  "InfoText",
  "Menu",
  "MenuText",
  "Scrollbar",
  "ThreeDDarkShadow",
  "ThreeDFace",
  "ThreeDHighlight",
  "ThreeDLightShadow",
  "ThreeDShadow",
  "Window",
  "WindowFrame",
  "WindowText",
];

/// CSS Color Level 4 system colors — added in Stylelint 14+.
const SYSTEM_COLORS_CSS4: &[&str] = &[
  "AccentColor",
  "AccentColorText",
  "ActiveText",
  "ButtonBorder",
  "Canvas",
  "CanvasText",
  "Field",
  "FieldText",
  "LinkText",
  "Mark",
  "MarkText",
  "SelectedItem",
  "SelectedItemText",
  "VisitedText",
];

/// Whether `s` is a system color, gated on the installed Stylelint's major
/// version since CSS4 added more.
pub(super) fn is_system_color(s: &str) -> bool {
  let v = stylelint_major_version();
  if SYSTEM_COLORS_CSS2.iter().any(|c| c.eq_ignore_ascii_case(s)) {
    return true;
  }
  if v >= 14 && SYSTEM_COLORS_CSS4.iter().any(|c| c.eq_ignore_ascii_case(s)) {
    return true;
  }
  if installed_at_least(16, 12)
    && PREFIXED_SYSTEM_COLORS
      .iter()
      .any(|c| c.eq_ignore_ascii_case(s))
  {
    return true;
  }
  false
}

/// Vendor-prefixed system colors, which Stylelint ignores since 16.12.
const PREFIXED_SYSTEM_COLORS: &[&str] = &[
  "-moz-buttondefault",
  "-moz-buttonhoverface",
  "-moz-buttonhovertext",
  "-moz-cellhighlight",
  "-moz-cellhighlighttext",
  "-moz-combobox",
  "-moz-comboboxtext",
  "-moz-dialog",
  "-moz-dialogtext",
  "-moz-dragtargetzone",
  "-moz-eventreerow",
  "-moz-field",
  "-moz-fieldtext",
  "-moz-html-cellhighlight",
  "-moz-html-cellhighlighttext",
  "-moz-mac-accentdarkestshadow",
  "-moz-mac-accentdarkshadow",
  "-moz-mac-accentface",
  "-moz-mac-accentlightesthighlight",
  "-moz-mac-accentlightshadow",
  "-moz-mac-accentregularhighlight",
  "-moz-mac-accentregularshadow",
  "-moz-mac-chrome-active",
  "-moz-mac-chrome-inactive",
  "-moz-mac-focusring",
  "-moz-mac-menuselect",
  "-moz-mac-menushadow",
  "-moz-mac-menutextselect",
  "-moz-menubarhovertext",
  "-moz-menubartext",
  "-moz-menuhover",
  "-moz-menuhovertext",
  "-moz-nativehyperlinktext",
  "-moz-oddtreerow",
  "-moz-win-accentcolor",
  "-moz-win-accentcolortext",
  "-moz-win-communicationstext",
  "-moz-win-mediatext",
  "-ms-hotlight",
];

/// SVG keywords that have camelCase canonical forms.
pub(super) const SVG_CAMEL_CASE_KEYWORDS: &[&str] = &[
  "currentColor",
  "optimizeSpeed",
  "optimizeLegibility",
  "optimizeQuality",
  "crispEdges",
  "geometricPrecision",
  "visiblePainted",
  "visibleFill",
  "visibleStroke",
  "sRGB",
  "linearRGB",
];

/// Whether `s` is an SVG keyword that is legitimately camelCase.
pub(super) fn is_svg_camel_case_keyword(s: &str) -> bool {
  SVG_CAMEL_CASE_KEYWORDS
    .iter()
    .any(|k| k.eq_ignore_ascii_case(s))
}

/// Properties whose values are entirely custom identifiers.
const CUSTOM_IDENT_PROPERTIES: &[&str] = &[
  "animation-name",
  "counter-increment",
  "counter-reset",
  "grid-row",
  "grid-column",
  "grid-area",
  "grid-row-start",
  "grid-row-end",
  "grid-column-start",
  "grid-column-end",
  "list-style-type",
];

/// Whether the property takes a `<custom-ident>`, whose case is author-chosen.
pub(super) fn is_custom_ident_property(prop: &str) -> bool {
  CUSTOM_IDENT_PROPERTIES.contains(&prop.to_ascii_lowercase().as_str())
}

/// Properties where some positions are keywords and some are custom idents.
const MIXED_IDENT_PROPERTIES: &[&str] = &["animation", "font", "font-family", "list-style"];

/// Whether the property mixes keywords with author-chosen identifiers.
pub(super) fn is_mixed_ident_property(prop: &str) -> bool {
  MIXED_IDENT_PROPERTIES.contains(&prop.to_ascii_lowercase().as_str())
}

/// Known CSS list-style-type keywords (includes predefined counter styles).
const LIST_STYLE_TYPE_KEYWORDS: &[&str] = &[
  "none",
  "disc",
  "circle",
  "square",
  "decimal",
  "decimal-leading-zero",
  "lower-roman",
  "upper-roman",
  "lower-greek",
  "lower-latin",
  "upper-latin",
  "lower-alpha",
  "upper-alpha",
  "armenian",
  "georgian",
  "cjk-ideographic",
  "hiragana",
  "katakana",
  "hiragana-iroha",
  "katakana-iroha",
  // CSS Counter Styles Level 3 predefined counter styles
  "arabic-indic",
  "bengali",
  "cambodian",
  "cjk-decimal",
  "cjk-earthly-branch",
  "cjk-heavenly-stem",
  "devanagari",
  "ethiopic-numeric",
  "gujarati",
  "gurmukhi",
  "hebrew",
  "japanese-formal",
  "japanese-informal",
  "kannada",
  "khmer",
  "korean-hangul-formal",
  "korean-hanja-formal",
  "korean-hanja-informal",
  "lao",
  "malayalam",
  "mongolian",
  "myanmar",
  "oriya",
  "persian",
  "simp-chinese-formal",
  "simp-chinese-informal",
  "tamil",
  "telugu",
  "thai",
  "tibetan",
  "trad-chinese-formal",
  "trad-chinese-informal",
  "disclosure-closed",
  "disclosure-open",
  "ethiopic-halehame-ti-er",
  "ethiopic-halehame-ti-et",
  "ethiopic-halehame",
  "hangul",
  "hangul-consonant",
  "somali",
  "inherit",
  "initial",
  "unset",
  "revert",
  "revert-layer",
];

/// Whether `s` is a `list-style-type` keyword.
pub(super) fn is_list_style_type_keyword(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  LIST_STYLE_TYPE_KEYWORDS.iter().any(|k| *k == lower)
}
