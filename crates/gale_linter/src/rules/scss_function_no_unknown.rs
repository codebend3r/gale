use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/function-no-unknown";

declare_rule! {
  /// SCSS-aware replacement for `function-no-unknown`.
  ///
  /// Like the core rule but recognises SCSS built-in functions (`darken()`,
  /// `lighten()`, `map-get()`, etc.) and user-defined functions.
  /// Only active in SCSS/Sass files.
  ///
  /// Equivalent to `scss/function-no-unknown`.
  pub struct ScssFunctionNoUnknown => ScssFunctionNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown functions (SCSS-aware)",
    default_severity: Error,
  }
}

/// `scss/function-no-unknown` as configured.
pub struct ScssFunctionNoUnknownCheck {
  /// `ignoreFunctions`: names, compared case-insensitively, and `/regex/`
  /// entries, matched by search.
  ignore_functions: Matchers,
}

impl ScssFunctionNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreFunctions` (names or
  /// `/regex/`es).  Names compare case-insensitively, as gale always has.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_functions = Matchers::ignoring_case(&options.strings("ignoreFunctions"));
    options.finish()?;
    Ok(Self { ignore_functions })
  }

  /// Whether `ignoreFunctions` lists `name`.
  fn is_ignored(&self, name: &str) -> bool {
    self.ignore_functions.matches(name)
  }
}

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

/// Whether `name` is a standard CSS function.
fn is_known_css_function(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  KNOWN_CSS_FUNCTIONS.binary_search(&lower.as_str()).is_ok()
}

/// Whether `name` is a built-in SCSS function.
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

impl Check for ScssFunctionNoUnknownCheck {
  /// Flags function calls that are neither standard CSS nor built-in SCSS,
  /// skipping any listed in `ignoreFunctions`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Style(rule) = node else {
      return vec![];
    };

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
        if self.is_ignored(&func_name) {
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
            NAME,
            format!("Unexpected unknown function \"{}\"", func_name),
          )
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

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
    let rule = configured(&ScssFunctionNoUnknown, None);
    let node = style_node(&[("color", "unknownfn(red)")]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn allows_known_css_functions() {
    let rule = configured(&ScssFunctionNoUnknown, None);
    let node = style_node(&[("width", "calc(100% - 20px)"), ("color", "rgb(255, 0, 0)")]);
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn allows_known_scss_functions() {
    let rule = configured(&ScssFunctionNoUnknown, None);
    let node = style_node(&[("color", "darken(#000, 10%)")]);
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn reports_unknown_function() {
    let rule = configured(&ScssFunctionNoUnknown, None);
    let node = style_node(&[("color", "totally-unknown(red)")]);
    let d = rule.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("totally-unknown"));
  }

  #[test]
  fn allows_vendor_prefixed() {
    let rule = configured(&ScssFunctionNoUnknown, None);
    let node = style_node(&[("background", "-webkit-linear-gradient(red, blue)")]);
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn allows_namespaced_module_calls() {
    let rule = configured(&ScssFunctionNoUnknown, None);
    let node = style_node(&[("color", "color.adjust(red, $red: 10)")]);
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFunctions": ["my-fn", "/^theme-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/function-no-unknown\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFunctions": [true] }])
      ),
      [
        "Invalid value \"true\" for option \"ignoreFunctions\" of rule \"scss/function-no-unknown\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreAtRules": ["x"] }])),
      ["Invalid option name \"ignoreAtRules\" for rule \"scss/function-no-unknown\""]
    );
  }

  #[test]
  fn ignore_functions_takes_names_and_regexes() {
    let source = "a { color: my-fn(1); width: theme-size(2); height: other(3); }\n";
    let all = crate::testing::lint(
      NAME,
      serde_json::json!(true),
      source,
      gale_css_parser::Syntax::Scss,
    );
    assert_eq!(all.len(), 3);
    let options = serde_json::json!([true, { "ignoreFunctions": ["MY-FN", "/^theme-/"] }]);
    let left = crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss);
    assert_eq!(left.len(), 1);
    assert!(left[0].message.contains("other"));
  }
}
