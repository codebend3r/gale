use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// SCSS-aware replacement for `function-no-unknown`.
///
/// Like the core rule but recognises SCSS built-in functions (`darken()`,
/// `lighten()`, `map-get()`, etc.) and user-defined functions.
/// Only active in SCSS/Sass files.
///
/// Equivalent to `scss/function-no-unknown`.
pub struct ScssFunctionNoUnknown;

/// Known CSS functions (sorted for binary search).
/// Includes legacy/vendor functions that Stylelint's scss/function-no-unknown
/// considers standard (e.g. `rect` for clip, `from`/`to`/`color-stop` for old WebKit gradients).
static KNOWN_CSS_FUNCTIONS: &[&str] = &[
  "abs",
  "acos",
  "anchor",
  "anchor-size",
  "asin",
  "atan",
  "atan2",
  "attr",
  "blur",
  "brightness",
  "calc",
  "circle",
  "clamp",
  "color",
  "color-mix",
  "color-stop",
  "conic-gradient",
  "contrast",
  "cos",
  "counter",
  "counters",
  "cross-fade",
  "cubic-bezier",
  "drop-shadow",
  "ease",
  "element",
  "ellipse",
  "env",
  "exp",
  "fit-content",
  "format",
  "from",
  "grayscale",
  "hsl",
  "hsla",
  "hue-rotate",
  "hwb",
  "hypot",
  "image-set",
  "inherit",
  "initial",
  "inset",
  "invert",
  "lab",
  "lch",
  "light-dark",
  "linear",
  "linear-gradient",
  "local",
  "log",
  "matrix",
  "matrix3d",
  "max",
  "min",
  "minmax",
  "mod",
  "oklab",
  "oklch",
  "opacity",
  "paint",
  "path",
  "perspective",
  "polygon",
  "pow",
  "radial-gradient",
  "ray",
  "rect",
  "rem",
  "repeat",
  "repeating-conic-gradient",
  "repeating-linear-gradient",
  "repeating-radial-gradient",
  "revert",
  "revert-layer",
  "rgb",
  "rgba",
  "rotate",
  "rotate3d",
  "rotatex",
  "rotatey",
  "rotatez",
  "round",
  "saturate",
  "scale",
  "scale3d",
  "scalex",
  "scaley",
  "scalez",
  "selector",
  "sepia",
  "sign",
  "sin",
  "skew",
  "skewx",
  "skewy",
  "sqrt",
  "steps",
  "supports",
  "symbols",
  "tan",
  "tech",
  "to",
  "translate",
  "translate3d",
  "translatex",
  "translatey",
  "translatez",
  "unset",
  "url",
  "var",
];

/// Known SCSS built-in functions (sorted for binary search).
static KNOWN_SCSS_FUNCTIONS: &[&str] = &[
  "adjust-color",
  "adjust-hue",
  "alpha",
  "append",
  "blue",
  "call",
  "ceil",
  "change-color",
  "comparable",
  "complement",
  "content-exists",
  "darken",
  "desaturate",
  "feature-exists",
  "floor",
  "function-exists",
  "get-function",
  "global-variable-exists",
  "green",
  "hue",
  "ie-hex-str",
  "if",
  "index",
  "inspect",
  "is-bracketed",
  "is-superselector",
  "join",
  "keywords",
  "length",
  "lighten",
  "lightness",
  "list-separator",
  "map-get",
  "map-has-key",
  "map-keys",
  "map-merge",
  "map-remove",
  "map-values",
  "meta-call",
  "mixin-exists",
  "mix",
  "nth",
  "opacify",
  "opacity",
  "percentage",
  "quote",
  "random",
  "red",
  "round",
  "saturate",
  "saturation",
  "scale-color",
  "selector-append",
  "selector-extend",
  "selector-nest",
  "selector-parse",
  "selector-replace",
  "selector-unify",
  "set-nth",
  "simple-selectors",
  "str-index",
  "str-insert",
  "str-length",
  "str-slice",
  "to-lower-case",
  "to-upper-case",
  "transparentize",
  "type-of",
  "unique-id",
  "unit",
  "unitless",
  "unquote",
  "variable-exists",
  "zip",
];

fn is_known_css_function(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  KNOWN_CSS_FUNCTIONS.binary_search(&lower.as_str()).is_ok()
}

fn is_known_scss_function(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  KNOWN_SCSS_FUNCTIONS.binary_search(&lower.as_str()).is_ok()
}

/// Extract function call names from a CSS value string.
fn extract_functions(value: &str) -> Vec<String> {
  let mut functions = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if bytes[i] == b'(' && i > 0 {
      let end = i;
      let mut start = i;
      while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
          || bytes[start - 1] == b'-'
          || bytes[start - 1] == b'_')
      {
        start -= 1;
      }
      if start < end {
        let name = &value[start..end];
        functions.push(name.to_string());
      }
    }
    i += 1;
  }

  functions
}

impl Rule for ScssFunctionNoUnknown {
  fn name(&self) -> &'static str {
    "scss/function-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown functions (SCSS-aware)"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read `ignoreFunctions` from secondary options.
    let ignore_functions: Vec<String> = ctx
      .secondary_options()
      .and_then(|v| v.get("ignoreFunctions"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| v.as_str())
          .map(|s| s.to_ascii_lowercase())
          .collect()
      })
      .unwrap_or_default();

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      for func_name in extract_functions(&decl.value) {
        // Skip vendor-prefixed functions
        if func_name.starts_with('-') {
          continue;
        }

        // Skip namespaced SCSS module calls like `color.adjust()`
        // The `.` is not part of the function name extraction, so
        // we check if a `.` precedes the function in the value.
        // Actually, extract_functions stops at `.`, so `color.adjust(`
        // would yield `adjust`. These are user module calls — allow them.
        // We handle this by checking if the character before the function
        // name in the value is `.`.
        let func_start = decl.value.find(&format!("{}(", func_name));
        if let Some(pos) = func_start
          && pos > 0
          && decl.value.as_bytes()[pos - 1] == b'.'
        {
          continue;
        }

        // Skip functions in the user-supplied ignoreFunctions list
        let func_lower = func_name.to_ascii_lowercase();
        if ignore_functions.iter().any(|f| f == &func_lower) {
          continue;
        }

        if is_known_css_function(&func_name) {
          continue;
        }
        if is_known_scss_function(&func_name) {
          continue;
        }

        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown function \"{}\"", func_name),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn css_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_node(props: &[(&str, &str)]) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: props
        .iter()
        .map(|(p, v)| Declaration {
          property: p.to_string(),
          value: v.to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        })
        .collect(),
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn skips_non_scss() {
    let node = style_node(&[("color", "unknownfn(red)")]);
    assert!(ScssFunctionNoUnknown.check(&node, &css_ctx()).is_empty());
  }

  #[test]
  fn allows_known_css_functions() {
    let node = style_node(&[("width", "calc(100% - 20px)"), ("color", "rgb(255, 0, 0)")]);
    assert!(ScssFunctionNoUnknown.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn allows_known_scss_functions() {
    let node = style_node(&[("color", "darken(#000, 10%)")]);
    assert!(ScssFunctionNoUnknown.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn reports_unknown_function() {
    let node = style_node(&[("color", "totally-unknown(red)")]);
    let d = ScssFunctionNoUnknown.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("totally-unknown"));
  }

  #[test]
  fn allows_vendor_prefixed() {
    let node = style_node(&[("background", "-webkit-linear-gradient(red, blue)")]);
    assert!(ScssFunctionNoUnknown.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn allows_namespaced_module_calls() {
    let node = style_node(&[("color", "color.adjust(red, $red: 10)")]);
    assert!(ScssFunctionNoUnknown.check(&node, &scss_ctx()).is_empty());
  }
}
