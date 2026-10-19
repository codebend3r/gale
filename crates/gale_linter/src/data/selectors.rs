//! Pseudo-class, pseudo-element and HTML element names.

use super::lookup;

/// Whether `name` is a standard CSS pseudo-class (without the `:`).
pub fn is_known_pseudo_class(name: &str) -> bool {
  lookup(KNOWN_PSEUDO_CLASSES, name)
}

/// Whether `name` is a standard CSS pseudo-element (without the `::`).
pub fn is_known_pseudo_element(name: &str) -> bool {
  lookup(KNOWN_PSEUDO_ELEMENTS, name)
}

/// Whether `name` is a standard HTML element name.
pub fn is_known_html_element(name: &str) -> bool {
  lookup(KNOWN_HTML_ELEMENTS, name)
}

/// Whether `name` is an HTML element Stylelint recognises that has not yet
/// reached the standard set, such as `selectlist` or `fencedframe`.
pub fn is_experimental_html_element(name: &str) -> bool {
  lookup(EXPERIMENTAL_HTML_ELEMENTS, name)
}

/// Pseudo-classes, without the leading colon.
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

/// Pseudo-elements, without the leading `::`.
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

/// HTML elements Stylelint knows about that are not yet in the standard set.
static EXPERIMENTAL_HTML_ELEMENTS: &[&str] = &[
  "fencedframe",
  "geolocation",
  "install",
  "listbox",
  "model",
  "portal",
  "selectedcontent",
  "selectlist",
  "usermedia",
];

#[cfg(test)]
mod tests {
  use super::*;
  use crate::data::assert_sorted;

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
  fn tables_are_sorted() {
    assert_sorted(KNOWN_PSEUDO_CLASSES, "KNOWN_PSEUDO_CLASSES");
    assert_sorted(KNOWN_PSEUDO_ELEMENTS, "KNOWN_PSEUDO_ELEMENTS");
    assert_sorted(KNOWN_HTML_ELEMENTS, "KNOWN_HTML_ELEMENTS");
    assert_sorted(EXPERIMENTAL_HTML_ELEMENTS, "EXPERIMENTAL_HTML_ELEMENTS");
  }
}
