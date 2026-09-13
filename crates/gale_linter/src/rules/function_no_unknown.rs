use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

pub struct FunctionNoUnknown;

/// Known CSS functions (sorted alphabetically for binary search).
static KNOWN_FUNCTIONS: &[&str] = &[
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
  "translate",
  "translate3d",
  "translatex",
  "translatey",
  "translatez",
  "unset",
  "url",
  "var",
];

/// Known SCSS built-in functions (sorted).
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
  "global-variable-exists",
  "green",
  "hue",
  "if",
  "index",
  "inspect",
  "is-bracketed",
  "is-superselector",
  "join",
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
  "mixin-exists",
  "mix",
  "nth",
  "opacify",
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

fn is_known_function(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  KNOWN_FUNCTIONS.binary_search(&lower.as_str()).is_ok()
}

fn is_known_scss_function(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  KNOWN_SCSS_FUNCTIONS.binary_search(&lower.as_str()).is_ok()
}

/// Extract function names from a CSS value string.
///
/// Looks for patterns like `name(` and returns the function names found.
fn extract_functions(value: &str) -> Vec<String> {
  let mut functions = Vec::new();
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    // Look for `(` and collect the preceding identifier
    if chars[i] == '(' {
      // Walk back to find the function name
      let end = i;
      let mut start = i;
      while start > 0
        && (chars[start - 1].is_ascii_alphanumeric()
          || chars[start - 1] == '-'
          || chars[start - 1] == '_')
      {
        start -= 1;
      }
      if start < end {
        let name: String = chars[start..end].iter().collect();
        functions.push(name);
      }
    }
    i += 1;
  }

  functions
}

impl Rule for FunctionNoUnknown {
  fn name(&self) -> &'static str {
    "function-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown CSS functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let is_scss = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    );

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      for func_name in extract_functions(&decl.value) {
        // Skip vendor-prefixed functions
        if func_name.starts_with('-') {
          continue;
        }
        if is_known_function(&func_name) {
          continue;
        }
        // In SCSS mode, also allow SCSS built-in functions
        if is_scss && is_known_scss_function(&func_name) {
          continue;
        }
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown function \"{func_name}\""),
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

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
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
  fn reports_unknown_function() {
    let node = style_node(&[("color", "unknownfn(red)")]);
    let d = FunctionNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("unknownfn"));
  }

  #[test]
  fn allows_known_functions() {
    let node = style_node(&[
      ("width", "calc(100% - 20px)"),
      ("color", "rgb(255, 0, 0)"),
      ("background", "linear-gradient(red, blue)"),
      ("--x", "var(--color)"),
    ]);
    assert!(FunctionNoUnknown.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn allows_vendor_prefixed_functions() {
    let node = style_node(&[("background", "-webkit-linear-gradient(red, blue)")]);
    assert!(FunctionNoUnknown.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn allows_scss_functions_in_scss() {
    let node = style_node(&[("color", "darken(#000, 10%)")]);
    assert!(FunctionNoUnknown.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn reports_scss_functions_in_css() {
    let node = style_node(&[("color", "darken(#000, 10%)")]);
    let d = FunctionNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("darken"));
  }

  #[test]
  fn ignores_values_without_functions() {
    let node = style_node(&[("color", "red"), ("margin", "10px")]);
    assert!(FunctionNoUnknown.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn extracts_multiple_functions() {
    let node = style_node(&[("background", "unknownA(1) unknownB(2)")]);
    let d = FunctionNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 2);
  }
}
