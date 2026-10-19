use std::sync::LazyLock;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};
use regex::Regex;

use crate::postcss_tree::{Node, NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext};
use crate::value_parser::{self, ValueNode};

/// Require `#{...}` around a Sass variable used where CSS reads its text
/// rather than its value.
///
/// Equivalent to stylelint-scss's `scss/dollar-variable-no-missing-interpolation`.
/// It knows the variables the stylesheet assigns (and the `$var` or
/// `module.$var` a custom property is set to), and reports the first one
/// left bare in:
///
/// - a custom-ident property (`animation-name`, `counter-reset`, ...) or
///   `@supports`, when the variable holds a quoted string;
/// - `@keyframes` or `@counter-style` params, whatever it holds;
/// - a custom property (`--x`), whatever it holds.
///
/// The fix wraps every bare variable reference in the value.  In a custom
/// property a variable inside a Sass function call has the whole call
/// wrapped (`#{svg.encode($icon)}`), while native CSS functions stay
/// outside (`calc(#{$size} * 2)`).
pub struct ScssDollarVariableNoMissingInterpolation;

/// Properties whose value is a custom identifier.
const CUSTOM_IDENT_PROPERTIES: &[&str] = &[
  "animation",
  "animation-name",
  "counter-reset",
  "counter-increment",
  "list-style-type",
  "will-change",
];

/// What upstream's fix treats as a variable reference: `$var` or
/// `module.$var`.
static VARIABLE_REFERENCE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^(?:[0-9A-Za-z_-]+\.)?\$[0-9A-Za-z_-]+$").expect("valid regex"));

/// Which assigned variables must be interpolated in a node.
#[derive(Clone, Copy)]
enum Required {
  /// Only those holding a quoted string.
  StringValued,
  /// All of them.
  All,
}

impl Rule for ScssDollarVariableNoMissingInterpolation {
  fn name(&self) -> &'static str {
    "scss/dollar-variable-no-missing-interpolation"
  }

  fn description(&self) -> &'static str {
    "Disallow $variables without #{} interpolation in strings"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports the first bare variable in each declaration or at-rule that
  /// needs it interpolated.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let tree = ctx.postcss_tree();
    let variables = Variables::collect(&tree);
    if variables.all.is_empty() {
      return Vec::new();
    }
    let mut diags = Vec::new();

    for (i, node) in tree.nodes.iter().enumerate() {
      let (required, value, value_index) = match node.kind {
        NodeKind::Decl if CUSTOM_IDENT_PROPERTIES.contains(&node.name.as_str()) => (
          Required::StringValued,
          &node.value,
          tree.declaration_value_index(node),
        ),
        NodeKind::Decl if node.name.starts_with("--") => (
          Required::All,
          &node.value,
          tree.declaration_value_index(node),
        ),
        NodeKind::AtRule if node.name == "supports" => (
          Required::StringValued,
          &node.params,
          Some(tree.at_rule_param_index(node)),
        ),
        NodeKind::AtRule if node.name == "keyframes" || node.name == "counter-style" => (
          Required::All,
          &node.params,
          Some(tree.at_rule_param_index(node)),
        ),
        _ => continue,
      };
      let Some(variable) = first_bare_variable(value, |name| variables.requires(required, name))
      else {
        continue;
      };

      let node_name = if node.kind == NodeKind::AtRule {
        format!("@{}", node.name)
      } else {
        node.name.clone()
      };
      // Upstream points at the first occurrence of the variable in the
      // node's text.
      let span = tree.text(i).find(variable).map_or_else(
        || Span::from_range(node.start, node.end),
        |at| Span::new(node.start + at, variable.len()),
      );
      let mut diag = Diagnostic::new(
        self.name(),
        format!("Expected variable {variable} to be interpolated when using it with {node_name}"),
      )
      .severity(self.default_severity())
      .message_args([node_name.as_str(), variable])
      .span(span);
      if let Some(fix) = value_index.and_then(|index| interpolate(&tree, node, value, index)) {
        diag = diag.fix(fix);
      }
      diags.push(diag);
    }
    diags
  }
}

/// The variables a stylesheet assigns (upstream's `collectVariables`).
struct Variables<'t> {
  /// Every `$variable` assigned, and every variable a custom property is
  /// set to bare.
  all: Vec<&'t str>,
  /// The `$variables` assigned a quoted string.
  string_valued: Vec<&'t str>,
}

impl<'t> Variables<'t> {
  /// The variables assigned anywhere in `tree`.
  fn collect(tree: &'t PostcssTree<'_>) -> Self {
    let mut all: Vec<&str> = Vec::new();
    let mut string_valued = Vec::new();
    for decl in tree.decls() {
      let (prop, value) = (decl.name.as_str(), decl.value.as_str());
      // A custom property set to a variable makes that variable one to
      // interpolate, assigned here or not.
      if prop.starts_with("--") && is_dollar_var(value) && !all.contains(&value) {
        all.push(value);
        continue;
      }
      if !prop.starts_with('$') || all.contains(&prop) {
        continue;
      }
      if is_string_value(value) {
        string_valued.push(prop);
      }
      all.push(prop);
    }
    Self { all, string_valued }
  }

  /// Whether `name` must be interpolated where `required` applies.
  fn requires(&self, required: Required, name: &str) -> bool {
    match required {
      Required::StringValued => self.string_valued.contains(&name),
      Required::All => self.all.contains(&name),
    }
  }
}

/// stylelint-scss's `isDollarVar`: `$name`, or a `.$` anywhere.
fn is_dollar_var(value: &str) -> bool {
  (value.len() > 1 && value.starts_with('$')) || value.contains(".$")
}

/// Upstream's `/^["'].*["']$/`: a value quoted at both ends, on one line.
fn is_string_value(value: &str) -> bool {
  value.len() >= 2
    && value.starts_with(['"', '\''])
    && value.ends_with(['"', '\''])
    && !value.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// The first word of `value` that `required` says must be interpolated and
/// that no `#{...}` holds (`checkValueForVariables`).
fn first_bare_variable<'v>(value: &'v str, required: impl Fn(&str) -> bool) -> Option<&'v str> {
  let parsed = value_parser::parse(value);
  let mut found = None;
  value_parser::walk(&parsed, &mut |node: &ValueNode<'v>| {
    if found.is_none()
      && node.is_word()
      && !node.value.is_empty()
      && !node.value.starts_with("#{")
      && required(node.value)
      && !is_inside_interpolation_block(value, node.source_index)
    {
      found = Some(node.value);
    }
    true
  });
  found
}

/// `isInsideInterpolationBlock`: whether a `#{` before `index` in `value`
/// is still open there.
fn is_inside_interpolation_block(value: &str, index: usize) -> bool {
  let bytes = value.as_bytes();
  let mut depth = 0i32;
  let mut inside = false;
  let mut i = 0;
  while i < index.min(bytes.len()) {
    if bytes[i] == b'#' && bytes.get(i + 1) == Some(&b'{') {
      depth += 1;
      inside = true;
      i += 1;
    } else if bytes[i] == b'}' {
      depth -= 1;
      if depth == 0 {
        inside = false;
      }
    }
    i += 1;
  }
  inside && depth > 0
}

/// The fix wrapping every bare variable in the node's value (or params),
/// which starts `index` bytes into the node (`wrapVariablesWithInterpolation`).
/// `None` when the value is not written in the source as PostCSS reads it,
/// as when comments were dropped from it.
fn interpolate(tree: &PostcssTree<'_>, node: &Node, value: &str, index: usize) -> Option<Fix> {
  let start = node.start + index;
  if tree.source().get(start..start + value.len())? != value {
    return None;
  }
  let wrap_function_calls = node.kind == NodeKind::Decl && node.name.starts_with("--");
  let mut ranges = Vec::new();
  interpolation_ranges(
    value,
    &value_parser::parse(value),
    wrap_function_calls,
    &mut Vec::new(),
    &mut ranges,
  );
  // Two variables in one Sass call give its range twice; wrap it once.
  ranges.dedup();
  let edits = ranges
    .into_iter()
    .map(|(from, to)| {
      Edit::new(
        Span::from_range(start + from, start + to),
        format!("#{{{}}}", &value[from..to]),
      )
    })
    .collect();
  Some(Fix::new("Interpolate the variables", edits))
}

/// Collects the ranges of `value` the fix wraps (`getInterpolationRanges`):
/// each bare variable reference, or with `wrap_function_calls`, the Sass
/// function call holding it.  `functions` are the calls around `nodes`.
fn interpolation_ranges<'v>(
  value: &str,
  nodes: &'v [ValueNode<'v>],
  wrap_function_calls: bool,
  functions: &mut Vec<&'v ValueNode<'v>>,
  ranges: &mut Vec<(usize, usize)>,
) {
  for node in nodes {
    if node.is_word()
      && VARIABLE_REFERENCE.is_match(node.value)
      && !is_inside_interpolation_block(value, node.source_index)
    {
      let call = if wrap_function_calls {
        sass_function_to_interpolate(functions)
      } else {
        None
      };
      let target = call.unwrap_or(node);
      ranges.push((target.source_index, target.source_end_index));
    }
    if node.is_function() {
      functions.push(node);
      interpolation_ranges(value, &node.nodes, wrap_function_calls, functions, ranges);
      functions.pop();
    }
  }
}

/// `findSassFunctionToInterpolate`: the outermost call in `functions`
/// inside the innermost native CSS function, if any is not native itself.
fn sass_function_to_interpolate<'v>(functions: &[&'v ValueNode<'v>]) -> Option<&'v ValueNode<'v>> {
  let inner = functions
    .iter()
    .rposition(|f| is_native_css_function(f.value))
    .map_or(0, |boundary| boundary + 1);
  functions[inner..]
    .iter()
    .find(|f| !is_native_css_function(f.value))
    .copied()
}

/// stylelint-scss's `isNativeCssFunction`, matched exactly.
fn is_native_css_function(name: &str) -> bool {
  NATIVE_CSS_FUNCTIONS.contains(&name)
}

/// The function names stylelint-scss's `isNativeCssFunction` knows.
const NATIVE_CSS_FUNCTIONS: &[&str] = &[
  "abs",
  "acos",
  "anchor",
  "anchor-size",
  "annotation",
  "asin",
  "atan",
  "atan2",
  "attr",
  "blur",
  "brightness",
  "calc",
  "character-variant",
  "calc-size",
  "circle",
  "clamp",
  "color",
  "color-contrast",
  "color-mix",
  "conic-gradient",
  "contrast",
  "cos",
  "counter",
  "counters",
  "cross-fade",
  "cubic-bezier",
  "device-cmyk",
  "drop-shadow",
  "element",
  "ellipse",
  "env",
  "exp",
  "fit-content",
  "format",
  "frames",
  "grayscale",
  "hsl",
  "hsla",
  "hue-rotate",
  "hwb",
  "hypot",
  "image",
  "image-set",
  "inset",
  "invert",
  "lab",
  "layer",
  "lch",
  "leader",
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
  "ornaments",
  "paint",
  "palette-mix",
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
  "rgb",
  "rgba",
  "rotate",
  "rotate3d",
  "rotateX",
  "rotatex",
  "rotateY",
  "rotatey",
  "rotateZ",
  "rotatez",
  "round",
  "saturate",
  "scale",
  "scale3d",
  "scaleX",
  "scalex",
  "scaleY",
  "scaley",
  "scaleZ",
  "scalez",
  "scroll",
  "sepia",
  "shape",
  "sign",
  "sin",
  "skew",
  "skewX",
  "skewx",
  "skewY",
  "skewy",
  "sqrt",
  "steps",
  "styleset",
  "stylistic",
  "swash",
  "symbols",
  "tan",
  "target-counter",
  "target-counters",
  "target-text",
  "translate",
  "translate3d",
  "translateX",
  "translatex",
  "translateY",
  "translatey",
  "translateZ",
  "translatez",
  "url",
  "var",
  "view",
  "xywh",
];

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "scss/dollar-variable-no-missing-interpolation";

  /// `(line, column, endLine, endColumn, message)` of each warning for
  /// `source`.
  fn warnings(source: &str) -> Vec<(usize, usize, usize, usize, String)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, Syntax::Scss)
      .into_iter()
      .map(|d| {
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column, d.message)
      })
      .collect()
  }

  /// The warning upstream expects for `variable` in `node`, on one line.
  fn at(
    line: usize,
    column: usize,
    end_column: usize,
    node: &str,
    variable: &str,
  ) -> (usize, usize, usize, usize, String) {
    let message =
      format!("Expected variable {variable} to be interpolated when using it with {node}");
    (line, column, line, end_column, message)
  }

  /// `source` after `gale --fix` with the rule on.
  fn fixed(source: &str) -> String {
    fix(RULE, json!(true), source, Syntax::Scss)
  }

  #[test]
  fn accepts_upstream_cases() {
    for source in [
      "\n      $var: \"my-anim\";\n\n      .class {\n        animation-name: #{$var};\n      }\n    ",
      "\n      $var: \"my-anim\";\n\n      @keyframes #{$var} {}\n    ",
      "\n      $var: my-anim;\n\n      .class {\n        animation-name: $var;\n      }\n    ",
      "\n      $var: my-anim;\n\n      .class {\n        animation-name: #{$var};\n      }\n    ",
      "\n      $var: my-counter;\n\n      body {\n        counter-reset: $var;\n      }\n    ",
      "\n      $var: my-anim;\n\n      @keyframes #{$var} {}\n    ",
      "\n      $var: circled-digits;\n\n      @counter-style #{$var} {\n        system: fixed;\n        symbols: ➀ ➁ ➂;\n        suffix: ' ';\n        speak-as: numbers;\n      }\n    ",
      "\n      $var: my-anim;\n\n      @supports (animation-name: #{$var}) {\n        @keyframes {}\n      }\n    ",
      "\n      $var: \"my-anim\";\n\n      @supports (animation-name: #{$var}) {\n        @keyframes {}\n      }\n    ",
      "\n      $var: my-anim;\n\n      @supports (animation-name: $var) {\n        @keyframes {}\n      }\n    ",
      "\n      $var: \"my-var\";\n      $foo: \"$var\";\n\n      div {\n        $bar: \"$var\";\n      }\n    ",
      "\n      $var: \"my-var\";\n      div {\n        content: $var;\n      }\n    ",
      "\n      $my-var-foo: \"my-var\";\n      div {\n        content: $my-var-foo;\n      }\n    ",
      "\n      @mixin test() {\n        $var: \"my-anim\";\n\n        @keyframes #{$var} {}\n      }\n    ",
      "\n      $var-with-dash: \"my-anim\";\n\n      @keyframes #{$var-with-dash} {}\n    ",
      "\n      $var_with_underscore: \"my-anim\";\n\n      @keyframes #{$var_with_underscore} {}\n    ",
      "\n      $color: \"red\";\n\n      .class {\n        --custom-color: #{$color};\n      }\n    ",
      "\n      $size: 10px;\n\n      .class {\n        --custom-size: #{$size};\n      }\n    ",
      "\n      $foo: 5;\n\n      a {\n        --scale: #{calculate-scale($foo)};\n      }\n    ",
      "\n      @use 'variables';\n\n      a {\n        --foo: #{variables.$someVariable};\n      }\n      ",
      "\n      @use 'svg';\n\n      $external-link-icon: \"<svg></svg>\";\n\n      :root {\n        --external-link-icon: #{svg.encode-svg($external-link-icon)};\n      }\n      ",
      "\n      @use 'sass:map';\n      @use 'sass:meta';\n\n      $theme: (font: serif);\n\n      :root {\n        --font: #{meta.inspect(map.get($theme, font))};\n      }\n      ",
      "\n      $size: 10px;\n\n      :root {\n        --custom-size: calc(#{$size} * 2);\n      }\n      ",
    ] {
      assert_eq!(warnings(source), vec![], "{source}");
      assert_eq!(fixed(source), source, "fixing {source}");
    }
  }

  #[test]
  fn reports_string_variables_in_custom_ident_properties() {
    assert_eq!(
      warnings(
        "\n      .class {\n        $var: \"my-anim\";\n        animation-name: $var;\n      }\n    "
      ),
      vec![at(4, 25, 29, "animation-name", "$var")]
    );
    assert_eq!(
      warnings(
        "\n      $var: \"my-anim\";\n\n      .class {\n        animation-name: $var;\n      }\n    "
      ),
      vec![at(5, 25, 29, "animation-name", "$var")]
    );
    assert_eq!(
      warnings(
        "\n      $var: \"my-counter\";\n\n      body {\n        counter-reset: $var;\n      }\n    "
      ),
      vec![at(5, 24, 28, "counter-reset", "$var")]
    );
  }

  #[test]
  fn reports_any_variable_in_keyframes_and_counter_style() {
    for value in ["my-anim", "\"my-anim\"", "'my-anim'"] {
      let source = format!("\n      $var: {value};\n\n      @keyframes $var {{}}\n    ");
      assert_eq!(
        warnings(&source),
        vec![at(4, 18, 22, "@keyframes", "$var")],
        "{source}"
      );
    }
    for value in ["\"circled-digits\"", "circled-digits"] {
      let source = format!(
        "\n      $var: {value};\n\n      @counter-style $var {{\n        system: fixed;\n        symbols: ➀ ➁ ➂;\n        suffix: ' ';\n        speak-as: numbers;\n      }}\n    "
      );
      assert_eq!(
        warnings(&source),
        vec![at(4, 22, 26, "@counter-style", "$var")],
        "{source}"
      );
    }
    assert_eq!(
      warnings(
        "\n      @mixin test() {\n        $var: \"my-anim\";\n\n        @keyframes $var {}\n      }\n    "
      ),
      vec![at(5, 20, 24, "@keyframes", "$var")]
    );
  }

  #[test]
  fn reports_string_variables_in_supports() {
    assert_eq!(
      warnings(
        "\n      $var: \"my-anim\";\n\n      @supports (animation-name: $var) {\n        @keyframes {}\n      }\n    "
      ),
      vec![at(4, 34, 38, "@supports", "$var")]
    );
    assert_eq!(
      warnings(
        "\n      $var: \"my-anim\";\n\n      @supports ( animation-name : $var ) {\n        @keyframes {}\n      }\n    "
      ),
      vec![at(4, 36, 40, "@supports", "$var")]
    );
  }

  #[test]
  fn reports_any_variable_in_custom_properties() {
    assert_eq!(
      warnings(
        "\n      $color: \"red\";\n\n      .class {\n        --custom-color: $color;\n      }\n    "
      ),
      vec![at(5, 25, 31, "--custom-color", "$color")]
    );
    assert_eq!(
      warnings(
        "\n      $size: 10px;\n\n      .class {\n        --custom-size: $size;\n      }\n    "
      ),
      vec![at(5, 24, 29, "--custom-size", "$size")]
    );
    // Upstream's `colum: 15` typo leaves the column unchecked there; it is
    // 16, where the variable starts.
    assert_eq!(
      warnings(
        "\n      @use 'variables';\n\n      a {\n        --foo: variables.$someVariable;\n      }  \n      "
      ),
      vec![at(5, 16, 39, "--foo", "variables.$someVariable")]
    );
    assert_eq!(
      warnings(
        "\n      $primary: \"blue\";\n      $secondary: \"green\";\n\n      .class {\n        --color-primary: $primary;\n        --color-secondary: $secondary;\n      }\n    "
      ),
      vec![
        at(6, 26, 34, "--color-primary", "$primary"),
        at(7, 28, 38, "--color-secondary", "$secondary"),
      ]
    );
  }

  #[test]
  fn reports_variables_in_sass_calls_in_custom_properties() {
    assert_eq!(
      warnings(
        "\n      @use 'svg';\n\n      $external-link-icon: \"<svg></svg>\";\n\n      :root {\n        --external-link-icon: svg.encode-svg($external-link-icon);\n      }\n      "
      )
      .into_iter()
      .map(|w| w.4)
      .collect::<Vec<_>>(),
      vec![
        "Expected variable $external-link-icon to be interpolated when using it with --external-link-icon"
      ]
    );
  }

  #[test]
  fn fixes_upstream_cases() {
    for (source, expected) in [
      (
        "\n      .class {\n        $var: \"my-anim\";\n        animation-name: $var;\n      }\n    ",
        "\n      .class {\n        $var: \"my-anim\";\n        animation-name: #{$var};\n      }\n    ",
      ),
      (
        "\n      $var: \"my-anim\";\n\n      .class {\n        animation-name: $var;\n      }\n    ",
        "\n      $var: \"my-anim\";\n\n      .class {\n        animation-name: #{$var};\n      }\n    ",
      ),
      (
        "\n      $var: \"my-counter\";\n\n      body {\n        counter-reset: $var;\n      }\n    ",
        "\n      $var: \"my-counter\";\n\n      body {\n        counter-reset: #{$var};\n      }\n    ",
      ),
      (
        "\n      $var: my-anim;\n\n      @keyframes $var {}\n    ",
        "\n      $var: my-anim;\n\n      @keyframes #{$var} {}\n    ",
      ),
      (
        "\n      $var: \"my-anim\";\n\n      @keyframes $var {}\n    ",
        "\n      $var: \"my-anim\";\n\n      @keyframes #{$var} {}\n    ",
      ),
      (
        "\n      $var: \"circled-digits\";\n\n      @counter-style $var {\n        system: fixed;\n        symbols: ➀ ➁ ➂;\n        suffix: ' ';\n        speak-as: numbers;\n      }\n    ",
        "\n      $var: \"circled-digits\";\n\n      @counter-style #{$var} {\n        system: fixed;\n        symbols: ➀ ➁ ➂;\n        suffix: ' ';\n        speak-as: numbers;\n      }\n    ",
      ),
      (
        "\n      $var: circled-digits;\n\n      @counter-style $var {\n        system: fixed;\n        symbols: ➀ ➁ ➂;\n        suffix: ' ';\n        speak-as: numbers;\n      }\n    ",
        "\n      $var: circled-digits;\n\n      @counter-style #{$var} {\n        system: fixed;\n        symbols: ➀ ➁ ➂;\n        suffix: ' ';\n        speak-as: numbers;\n      }\n    ",
      ),
      (
        "\n      $var: \"my-anim\";\n\n      @supports (animation-name: $var) {\n        @keyframes {}\n      }\n    ",
        "\n      $var: \"my-anim\";\n\n      @supports (animation-name: #{$var}) {\n        @keyframes {}\n      }\n    ",
      ),
      (
        "\n      $var: \"my-anim\";\n\n      @supports ( animation-name : $var ) {\n        @keyframes {}\n      }\n    ",
        "\n      $var: \"my-anim\";\n\n      @supports ( animation-name : #{$var} ) {\n        @keyframes {}\n      }\n    ",
      ),
      (
        "\n      @mixin test() {\n        $var: \"my-anim\";\n\n        @keyframes $var {}\n      }\n    ",
        "\n      @mixin test() {\n        $var: \"my-anim\";\n\n        @keyframes #{$var} {}\n      }\n    ",
      ),
      (
        "\n      $var1: \"anim1\";\n      $var2: \"anim2\";\n\n      .class {\n        animation: $var1 5s, $var2 3s;\n      }\n    ",
        "\n      $var1: \"anim1\";\n      $var2: \"anim2\";\n\n      .class {\n        animation: #{$var1} 5s, #{$var2} 3s;\n      }\n    ",
      ),
      (
        "\n      $var-with-dash: \"my-anim\";\n\n      @keyframes $var-with-dash {}\n    ",
        "\n      $var-with-dash: \"my-anim\";\n\n      @keyframes #{$var-with-dash} {}\n    ",
      ),
      (
        "\n      $var_with_underscore: \"my-anim\";\n\n      @keyframes $var_with_underscore {}\n    ",
        "\n      $var_with_underscore: \"my-anim\";\n\n      @keyframes #{$var_with_underscore} {}\n    ",
      ),
      (
        "\n      $color: \"red\";\n\n      .class {\n        --custom-color: $color;\n      }\n    ",
        "\n      $color: \"red\";\n\n      .class {\n        --custom-color: #{$color};\n      }\n    ",
      ),
      (
        "\n      $size: 10px;\n\n      .class {\n        --custom-size: $size;\n      }\n    ",
        "\n      $size: 10px;\n\n      .class {\n        --custom-size: #{$size};\n      }\n    ",
      ),
      (
        "\n      @use 'variables';\n\n      a {\n        --foo: variables.$someVariable;\n      }\n      ",
        "\n      @use 'variables';\n\n      a {\n        --foo: #{variables.$someVariable};\n      }\n      ",
      ),
      (
        "\n      @use 'svg';\n\n      $external-link-icon: \"<svg></svg>\";\n\n      :root {\n        --external-link-icon: svg.encode-svg($external-link-icon);\n      }\n      ",
        "\n      @use 'svg';\n\n      $external-link-icon: \"<svg></svg>\";\n\n      :root {\n        --external-link-icon: #{svg.encode-svg($external-link-icon)};\n      }\n      ",
      ),
      (
        "\n      @use 'sass:map';\n      @use 'sass:meta';\n\n      $theme: (font: serif);\n\n      :root {\n        --font: meta.inspect(map.get($theme, font));\n      }\n      ",
        "\n      @use 'sass:map';\n      @use 'sass:meta';\n\n      $theme: (font: serif);\n\n      :root {\n        --font: #{meta.inspect(map.get($theme, font))};\n      }\n      ",
      ),
      (
        "\n      $size: 10px;\n\n      :root {\n        --custom-size: calc($size * 2);\n      }\n      ",
        "\n      $size: 10px;\n\n      :root {\n        --custom-size: calc(#{$size} * 2);\n      }\n      ",
      ),
      (
        "\n      $primary: \"blue\";\n      $secondary: \"green\";\n\n      .class {\n        --color-primary: $primary;\n        --color-secondary: $secondary;\n      }\n    ",
        "\n      $primary: \"blue\";\n      $secondary: \"green\";\n\n      .class {\n        --color-primary: #{$primary};\n        --color-secondary: #{$secondary};\n      }\n    ",
      ),
    ] {
      assert_eq!(fixed(source), expected, "{source}");
    }
  }

  #[test]
  fn reports_once_per_node_with_the_first_variable() {
    let source = "$a: \"x\";\n$b: \"y\";\n.c { animation: $a 1s, $b 2s; }\n";
    assert_eq!(warnings(source), vec![at(3, 17, 19, "animation", "$a")]);
  }

  #[test]
  fn wraps_a_sass_call_once_for_two_variables() {
    assert_eq!(
      fixed("$a: 1;\n$b: 2;\n:root { --x: $a; --y: f($a, $b); }\n"),
      "$a: 1;\n$b: 2;\n:root { --x: #{$a}; --y: #{f($a, $b)}; }\n"
    );
  }
}
