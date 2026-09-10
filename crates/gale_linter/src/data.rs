//! Known CSS identifiers for validation rules.
//!
//! All arrays are sorted alphabetically for binary search.

use gale_css_parser::Syntax;

// Lookup helpers — case-insensitive binary search.

fn lookup(haystack: &[&str], needle: &str) -> bool {
  let lower = needle.to_ascii_lowercase();
  haystack.binary_search(&lower.as_str()).is_ok()
}

pub fn is_known_property(name: &str) -> bool {
  lookup(KNOWN_PROPERTIES, name)
}

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

pub fn is_known_pseudo_class(name: &str) -> bool {
  lookup(KNOWN_PSEUDO_CLASSES, name)
}

pub fn is_known_pseudo_element(name: &str) -> bool {
  lookup(KNOWN_PSEUDO_ELEMENTS, name)
}

pub fn is_known_unit(name: &str) -> bool {
  lookup(KNOWN_UNITS, name)
}

pub fn is_known_media_feature(name: &str) -> bool {
  lookup(KNOWN_MEDIA_FEATURES, name)
}

pub fn is_known_html_element(name: &str) -> bool {
  lookup(KNOWN_HTML_ELEMENTS, name)
}

// ---------------------------------------------------------------------------
// CSS Properties (curated standard set, sorted)
// ---------------------------------------------------------------------------

static KNOWN_PROPERTIES: &[&str] = &[
  "accent-color",
  "align-content",
  "align-items",
  "align-self",
  "all",
  "animation",
  "animation-composition",
  "animation-delay",
  "animation-direction",
  "animation-duration",
  "animation-fill-mode",
  "animation-iteration-count",
  "animation-name",
  "animation-play-state",
  "animation-timeline",
  "animation-timing-function",
  "appearance",
  "aspect-ratio",
  "backdrop-filter",
  "backface-visibility",
  "background",
  "background-attachment",
  "background-blend-mode",
  "background-clip",
  "background-color",
  "background-image",
  "background-origin",
  "background-position",
  "background-position-x",
  "background-position-y",
  "background-repeat",
  "background-size",
  "block-size",
  "border",
  "border-block",
  "border-block-color",
  "border-block-end",
  "border-block-end-color",
  "border-block-end-style",
  "border-block-end-width",
  "border-block-start",
  "border-block-start-color",
  "border-block-start-style",
  "border-block-start-width",
  "border-block-style",
  "border-block-width",
  "border-bottom",
  "border-bottom-color",
  "border-bottom-left-radius",
  "border-bottom-right-radius",
  "border-bottom-style",
  "border-bottom-width",
  "border-collapse",
  "border-color",
  "border-end-end-radius",
  "border-end-start-radius",
  "border-image",
  "border-image-outset",
  "border-image-repeat",
  "border-image-slice",
  "border-image-source",
  "border-image-width",
  "border-inline",
  "border-inline-color",
  "border-inline-end",
  "border-inline-end-color",
  "border-inline-end-style",
  "border-inline-end-width",
  "border-inline-start",
  "border-inline-start-color",
  "border-inline-start-style",
  "border-inline-start-width",
  "border-inline-style",
  "border-inline-width",
  "border-left",
  "border-left-color",
  "border-left-style",
  "border-left-width",
  "border-radius",
  "border-right",
  "border-right-color",
  "border-right-style",
  "border-right-width",
  "border-spacing",
  "border-start-end-radius",
  "border-start-start-radius",
  "border-style",
  "border-top",
  "border-top-color",
  "border-top-left-radius",
  "border-top-right-radius",
  "border-top-style",
  "border-top-width",
  "border-width",
  "bottom",
  "box-decoration-break",
  "box-shadow",
  "box-sizing",
  "break-after",
  "break-before",
  "break-inside",
  "caption-side",
  "caret-color",
  "clear",
  "clip",
  "clip-path",
  "clip-rule",
  "color",
  "color-interpolation",
  "color-interpolation-filters",
  "color-scheme",
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
  "contain",
  "contain-intrinsic-block-size",
  "contain-intrinsic-height",
  "contain-intrinsic-inline-size",
  "contain-intrinsic-size",
  "contain-intrinsic-width",
  "container",
  "container-name",
  "container-type",
  "content",
  "content-visibility",
  "counter-increment",
  "counter-reset",
  "counter-set",
  "cursor",
  "cx",
  "cy",
  "d",
  "direction",
  "display",
  "dominant-baseline",
  "elevation",
  "empty-cells",
  "field-sizing",
  "fill",
  "fill-opacity",
  "fill-rule",
  "filter",
  "flex",
  "flex-basis",
  "flex-direction",
  "flex-flow",
  "flex-grow",
  "flex-shrink",
  "flex-wrap",
  "float",
  "flood-color",
  "flood-opacity",
  "font",
  "font-display",
  "font-family",
  "font-feature-settings",
  "font-kerning",
  "font-language-override",
  "font-optical-sizing",
  "font-palette",
  "font-size",
  "font-size-adjust",
  "font-stretch",
  "font-style",
  "font-synthesis",
  "font-synthesis-small-caps",
  "font-synthesis-style",
  "font-synthesis-weight",
  "font-variant",
  "font-variant-alternates",
  "font-variant-caps",
  "font-variant-east-asian",
  "font-variant-emoji",
  "font-variant-ligatures",
  "font-variant-numeric",
  "font-variant-position",
  "font-variation-settings",
  "font-weight",
  "forced-color-adjust",
  "gap",
  "grid",
  "grid-area",
  "grid-auto-columns",
  "grid-auto-flow",
  "grid-auto-rows",
  "grid-column",
  "grid-column-end",
  "grid-column-gap",
  "grid-column-start",
  "grid-gap",
  "grid-row",
  "grid-row-end",
  "grid-row-gap",
  "grid-row-start",
  "grid-template",
  "grid-template-areas",
  "grid-template-columns",
  "grid-template-rows",
  "hanging-punctuation",
  "height",
  "hyphenate-character",
  "hyphenate-limit-chars",
  "hyphens",
  "image-orientation",
  "image-rendering",
  "inherits",
  "initial-letter",
  "initial-value",
  "inline-size",
  "input-security",
  "inset",
  "inset-block",
  "inset-block-end",
  "inset-block-start",
  "inset-inline",
  "inset-inline-end",
  "inset-inline-start",
  "interpolate-size",
  "isolation",
  "justify-content",
  "justify-items",
  "justify-self",
  "left",
  "letter-spacing",
  "lighting-color",
  "line-break",
  "line-clamp",
  "line-height",
  "list-style",
  "list-style-image",
  "list-style-position",
  "list-style-type",
  "margin",
  "margin-block",
  "margin-block-end",
  "margin-block-start",
  "margin-bottom",
  "margin-inline",
  "margin-inline-end",
  "margin-inline-start",
  "margin-left",
  "margin-right",
  "margin-top",
  "margin-trim",
  "marker",
  "marker-end",
  "marker-mid",
  "marker-start",
  "mask",
  "mask-border",
  "mask-border-mode",
  "mask-border-outset",
  "mask-border-repeat",
  "mask-border-slice",
  "mask-border-source",
  "mask-border-width",
  "mask-clip",
  "mask-composite",
  "mask-image",
  "mask-mode",
  "mask-origin",
  "mask-position",
  "mask-repeat",
  "mask-size",
  "mask-type",
  "math-depth",
  "math-style",
  "max-block-size",
  "max-height",
  "max-inline-size",
  "max-width",
  "min-block-size",
  "min-height",
  "min-inline-size",
  "min-width",
  "mix-blend-mode",
  "object-fit",
  "object-position",
  "object-view-box",
  "offset",
  "offset-anchor",
  "offset-distance",
  "offset-path",
  "offset-position",
  "offset-rotate",
  "opacity",
  "order",
  "orphans",
  "outline",
  "outline-color",
  "outline-offset",
  "outline-style",
  "outline-width",
  "overflow",
  "overflow-anchor",
  "overflow-block",
  "overflow-clip-margin",
  "overflow-inline",
  "overflow-wrap",
  "overflow-x",
  "overflow-y",
  "overscroll-behavior",
  "overscroll-behavior-block",
  "overscroll-behavior-inline",
  "overscroll-behavior-x",
  "overscroll-behavior-y",
  "padding",
  "padding-block",
  "padding-block-end",
  "padding-block-start",
  "padding-bottom",
  "padding-inline",
  "padding-inline-end",
  "padding-inline-start",
  "padding-left",
  "padding-right",
  "padding-top",
  "page",
  "page-break-after",
  "page-break-before",
  "page-break-inside",
  "paint-order",
  "perspective",
  "perspective-origin",
  "place-content",
  "place-items",
  "place-self",
  "pointer-events",
  "position",
  "print-color-adjust",
  "quotes",
  "r",
  "resize",
  "right",
  "rotate",
  "row-gap",
  "ruby-align",
  "ruby-position",
  "rx",
  "ry",
  "scale",
  "scroll-behavior",
  "scroll-margin",
  "scroll-margin-block",
  "scroll-margin-block-end",
  "scroll-margin-block-start",
  "scroll-margin-bottom",
  "scroll-margin-inline",
  "scroll-margin-inline-end",
  "scroll-margin-inline-start",
  "scroll-margin-left",
  "scroll-margin-right",
  "scroll-margin-top",
  "scroll-padding",
  "scroll-padding-block",
  "scroll-padding-block-end",
  "scroll-padding-block-start",
  "scroll-padding-bottom",
  "scroll-padding-inline",
  "scroll-padding-inline-end",
  "scroll-padding-inline-start",
  "scroll-padding-left",
  "scroll-padding-right",
  "scroll-padding-top",
  "scroll-snap-align",
  "scroll-snap-stop",
  "scroll-snap-type",
  "scroll-timeline",
  "scroll-timeline-axis",
  "scroll-timeline-name",
  "scrollbar-3dlight-color",
  "scrollbar-arrow-color",
  "scrollbar-base-color",
  "scrollbar-color",
  "scrollbar-darkshadow-color",
  "scrollbar-face-color",
  "scrollbar-gutter",
  "scrollbar-highlight-color",
  "scrollbar-shadow-color",
  "scrollbar-track-color",
  "scrollbar-width",
  "shape-image-threshold",
  "shape-margin",
  "shape-outside",
  "shape-rendering",
  "size",
  "speak",
  "stop-color",
  "stop-opacity",
  "stroke",
  "stroke-dasharray",
  "stroke-dashoffset",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-miterlimit",
  "stroke-opacity",
  "stroke-width",
  "tab-size",
  "table-layout",
  "text-align",
  "text-align-last",
  "text-anchor",
  "text-autospace",
  "text-combine-upright",
  "text-decoration",
  "text-decoration-color",
  "text-decoration-line",
  "text-decoration-skip",
  "text-decoration-skip-ink",
  "text-decoration-style",
  "text-decoration-thickness",
  "text-emphasis",
  "text-emphasis-color",
  "text-emphasis-position",
  "text-emphasis-style",
  "text-indent",
  "text-justify",
  "text-orientation",
  "text-overflow",
  "text-rendering",
  "text-shadow",
  "text-size-adjust",
  "text-transform",
  "text-underline-offset",
  "text-underline-position",
  "text-wrap",
  "text-wrap-mode",
  "text-wrap-style",
  "timeline-scope",
  "top",
  "touch-action",
  "transform",
  "transform-box",
  "transform-origin",
  "transform-style",
  "transition",
  "transition-behavior",
  "transition-delay",
  "transition-duration",
  "transition-property",
  "transition-timing-function",
  "translate",
  "unicode-bidi",
  "user-select",
  "vertical-align",
  "view-timeline",
  "view-timeline-axis",
  "view-timeline-inset",
  "view-timeline-name",
  "view-transition-name",
  "visibility",
  "white-space",
  "white-space-collapse",
  "widows",
  "width",
  "will-change",
  "word-break",
  "word-spacing",
  "word-wrap",
  "writing-mode",
  "x",
  "y",
  "z-index",
  "zoom",
];

// ---------------------------------------------------------------------------
// At-rules
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// SCSS / Sass at-rules (sorted)
// ---------------------------------------------------------------------------

static KNOWN_SCSS_AT_RULES: &[&str] = &[
  "at-root", "content", "debug", "each", "else", "else if", "error", "extend", "for", "forward",
  "function", "if", "include", "mixin", "return", "use", "warn", "while",
];

// ---------------------------------------------------------------------------
// Less at-rules (sorted)
// ---------------------------------------------------------------------------

static KNOWN_LESS_AT_RULES: &[&str] = &["detached-ruleset", "plugin"];

// ---------------------------------------------------------------------------
// Pseudo-classes (without the leading colon)
// ---------------------------------------------------------------------------

static KNOWN_PSEUDO_CLASSES: &[&str] = &[
  "active",
  "active-view-transition",
  "active-view-transition-type",
  "any-link",
  "autofill",
  "buffering",
  "checked",
  "current",
  "default",
  "defined",
  "dir",
  "disabled",
  "empty",
  "enabled",
  "first-child",
  "first-of-type",
  "focus",
  "focus-visible",
  "focus-within",
  "fullscreen",
  "future",
  "has",
  "has-slotted",
  "heading",
  "host",
  "host-context",
  "hover",
  "in-range",
  "indeterminate",
  "invalid",
  "is",
  "lang",
  "last-child",
  "last-of-type",
  "left",
  "link",
  "local-link",
  "low-value",
  "matches",
  "modal",
  "muted",
  "not",
  "nth-child",
  "nth-last-child",
  "nth-last-of-type",
  "nth-of-type",
  "only-child",
  "only-of-type",
  "open",
  "optional",
  "out-of-range",
  "past",
  "paused",
  "picture-in-picture",
  "placeholder-shown",
  "playing",
  "popover-open",
  "read-only",
  "read-write",
  "required",
  "right",
  "root",
  "scope",
  "seeking",
  "stalled",
  "state",
  "suboptimal-value",
  "target",
  "unchecked",
  "user-error",
  "user-invalid",
  "user-valid",
  "valid",
  "visited",
  "volume-locked",
  "where",
];

// ---------------------------------------------------------------------------
// Pseudo-elements (without the leading ::)
// ---------------------------------------------------------------------------

static KNOWN_PSEUDO_ELEMENTS: &[&str] = &[
  "after",
  "backdrop",
  "before",
  "content",
  "cue",
  "details-content",
  "file-selector-button",
  "first-letter",
  "first-line",
  "grammar-error",
  "highlight",
  "marker",
  "part",
  "placeholder",
  "selection",
  "slotted",
  "spelling-error",
  "target-text",
  "view-transition",
  "view-transition-group",
  "view-transition-image-pair",
  "view-transition-new",
  "view-transition-old",
];

// ---------------------------------------------------------------------------
// CSS Units (all lowercase, sorted)
// ---------------------------------------------------------------------------

static KNOWN_UNITS: &[&str] = &[
  "%", "cap", "ch", "cm", "cqb", "cqh", "cqi", "cqmax", "cqmin", "cqw", "deg", "dpcm", "dpi",
  "dppx", "dvb", "dvh", "dvi", "dvmax", "dvmin", "dvw", "em", "ex", "fr", "grad", "hz", "ic", "in",
  "khz", "lh", "lvb", "lvh", "lvi", "lvmax", "lvmin", "lvw", "mm", "ms", "pc", "pt", "px", "q",
  "rad", "rcap", "rch", "rem", "rex", "ric", "rlh", "s", "svb", "svh", "svi", "svmax", "svmin",
  "svw", "turn", "vb", "vh", "vi", "vmax", "vmin", "vw", "x",
];

// ---------------------------------------------------------------------------
// CSS Media Features (sorted)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// HTML Elements (sorted)
// ---------------------------------------------------------------------------

static KNOWN_HTML_ELEMENTS: &[&str] = &[
  "a",
  "abbr",
  "acronym",
  "address",
  "animate",
  "animatemotion",
  "animatetransform",
  "annotation",
  "annotation-xml",
  "applet",
  "area",
  "article",
  "aside",
  "audio",
  "b",
  "base",
  "basefont",
  "bdi",
  "bdo",
  "big",
  "blockquote",
  "body",
  "br",
  "button",
  "canvas",
  "caption",
  "center",
  "circle",
  "cite",
  "clippath",
  "code",
  "col",
  "colgroup",
  "data",
  "datalist",
  "dd",
  "defs",
  "del",
  "desc",
  "details",
  "dfn",
  "dialog",
  "div",
  "dl",
  "dt",
  "ellipse",
  "em",
  "embed",
  "feblend",
  "fecolormatrix",
  "fecomponenttransfer",
  "fecomposite",
  "feflood",
  "fefunca",
  "fefuncb",
  "fefuncg",
  "fefuncr",
  "fegaussianblur",
  "femerge",
  "femergenode",
  "feoffset",
  "fieldset",
  "figcaption",
  "figure",
  "filter",
  "font",
  "footer",
  "foreignobject",
  "form",
  "g",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "head",
  "header",
  "hgroup",
  "hr",
  "html",
  "i",
  "iframe",
  "image",
  "img",
  "input",
  "ins",
  "kbd",
  "label",
  "legend",
  "li",
  "line",
  "lineargradient",
  "link",
  "maction",
  "main",
  "map",
  "mark",
  "marker",
  "mask",
  "math",
  "menclose",
  "menu",
  "merror",
  "meta",
  "metadata",
  "meter",
  "mfenced",
  "mfrac",
  "mi",
  "mmultiscripts",
  "mn",
  "mo",
  "mover",
  "mpadded",
  "mphantom",
  "mprescripts",
  "mroot",
  "mrow",
  "ms",
  "mspace",
  "msqrt",
  "mstyle",
  "msub",
  "msubsup",
  "msup",
  "mtable",
  "mtd",
  "mtext",
  "mtr",
  "munder",
  "munderover",
  "nav",
  "none",
  "noscript",
  "object",
  "ol",
  "optgroup",
  "option",
  "output",
  "p",
  "param",
  "path",
  "pattern",
  "picture",
  "polygon",
  "polyline",
  "pre",
  "progress",
  "q",
  "radialgradient",
  "rect",
  "rp",
  "rt",
  "ruby",
  "s",
  "samp",
  "script",
  "search",
  "section",
  "select",
  "semantics",
  "set",
  "slot",
  "small",
  "source",
  "span",
  "stop",
  "strike",
  "strong",
  "style",
  "sub",
  "summary",
  "sup",
  "svg",
  "switch",
  "symbol",
  "table",
  "tbody",
  "td",
  "template",
  "text",
  "textarea",
  "textpath",
  "tfoot",
  "th",
  "thead",
  "time",
  "title",
  "tr",
  "track",
  "tspan",
  "tt",
  "u",
  "ul",
  "use",
  "var",
  "video",
  "wbr",
];

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn known_properties() {
    assert!(is_known_property("color"));
    assert!(is_known_property("Color")); // case-insensitive
    assert!(is_known_property("display"));
    assert!(is_known_property("flex-direction"));
    assert!(!is_known_property("colr"));
    assert!(!is_known_property("fake-property"));
  }

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
  fn known_pseudo_classes() {
    assert!(is_known_pseudo_class("hover"));
    assert!(is_known_pseudo_class("focus"));
    assert!(is_known_pseudo_class("nth-child"));
    assert!(!is_known_pseudo_class("hoverr"));
  }

  #[test]
  fn known_pseudo_elements() {
    assert!(is_known_pseudo_element("before"));
    assert!(is_known_pseudo_element("after"));
    assert!(is_known_pseudo_element("placeholder"));
    assert!(!is_known_pseudo_element("beforre"));
  }

  #[test]
  fn known_units() {
    assert!(is_known_unit("px"));
    assert!(is_known_unit("rem"));
    assert!(is_known_unit("em"));
    assert!(is_known_unit("%"));
    assert!(!is_known_unit("xyz"));
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
  fn known_html_elements() {
    assert!(is_known_html_element("div"));
    assert!(is_known_html_element("span"));
    assert!(is_known_html_element("a"));
    assert!(is_known_html_element("section"));
    assert!(!is_known_html_element("fakeelement"));
  }

  #[test]
  fn known_mathml_elements() {
    // MathML 4 tags added for Stylelint v17 compatibility
    assert!(is_known_html_element("math"));
    assert!(is_known_html_element("annotation"));
    assert!(is_known_html_element("annotation-xml"));
    assert!(is_known_html_element("maction"));
    assert!(is_known_html_element("menclose"));
    assert!(is_known_html_element("merror"));
    assert!(is_known_html_element("mfenced"));
    assert!(is_known_html_element("mfrac"));
    assert!(is_known_html_element("mi"));
    assert!(is_known_html_element("mmultiscripts"));
    assert!(is_known_html_element("mn"));
    assert!(is_known_html_element("mo"));
    assert!(is_known_html_element("mover"));
    assert!(is_known_html_element("mpadded"));
    assert!(is_known_html_element("mphantom"));
    assert!(is_known_html_element("mprescripts"));
    assert!(is_known_html_element("mroot"));
    assert!(is_known_html_element("mrow"));
    assert!(is_known_html_element("ms"));
    assert!(is_known_html_element("mspace"));
    assert!(is_known_html_element("msqrt"));
    assert!(is_known_html_element("mstyle"));
    assert!(is_known_html_element("msub"));
    assert!(is_known_html_element("msup"));
    assert!(is_known_html_element("msubsup"));
    assert!(is_known_html_element("mtable"));
    assert!(is_known_html_element("mtd"));
    assert!(is_known_html_element("mtext"));
    assert!(is_known_html_element("mtr"));
    assert!(is_known_html_element("munder"));
    assert!(is_known_html_element("munderover"));
    assert!(is_known_html_element("none"));
    assert!(is_known_html_element("semantics"));
  }

  #[test]
  fn arrays_are_sorted() {
    fn assert_sorted(arr: &[&str], name: &str) {
      for window in arr.windows(2) {
        assert!(
          window[0] < window[1],
          "{name}: '{0}' should come before '{1}'",
          window[0],
          window[1],
        );
      }
    }
    assert_sorted(KNOWN_PROPERTIES, "KNOWN_PROPERTIES");
    assert_sorted(KNOWN_AT_RULES, "KNOWN_AT_RULES");
    assert_sorted(KNOWN_SCSS_AT_RULES, "KNOWN_SCSS_AT_RULES");
    assert_sorted(KNOWN_LESS_AT_RULES, "KNOWN_LESS_AT_RULES");
    assert_sorted(KNOWN_PSEUDO_CLASSES, "KNOWN_PSEUDO_CLASSES");
    assert_sorted(KNOWN_PSEUDO_ELEMENTS, "KNOWN_PSEUDO_ELEMENTS");
    assert_sorted(KNOWN_UNITS, "KNOWN_UNITS");
    assert_sorted(KNOWN_MEDIA_FEATURES, "KNOWN_MEDIA_FEATURES");
    assert_sorted(KNOWN_HTML_ELEMENTS, "KNOWN_HTML_ELEMENTS");
  }
}
