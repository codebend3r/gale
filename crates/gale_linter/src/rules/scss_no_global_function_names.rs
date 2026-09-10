use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow deprecated global SCSS function calls that should use modules.
///
/// e.g. `adjust-color()` should be `color.adjust()`.
pub struct ScssNoGlobalFunctionNames;

/// Returns the Stylelint-compatible message for a deprecated global function.
/// Matches the exact format produced by stylelint-scss no-global-function-names.
fn function_message(name: &str) -> &'static str {
  match name {
    // rule_mapping entries: have specific argument transformations
    "darken" => {
      "Expected color.adjust($color, $lightness: -$amount) instead of darken($color, $amount)"
    }
    "lighten" => {
      "Expected color.adjust($color, $lightness: $amount) instead of lighten($color, $amount)"
    }
    "adjust-hue" => {
      "Expected color.adjust($color, $hue: $amount) instead of adjust-hue($color, $amount)"
    }
    "desaturate" => {
      "Expected color.adjust($color, $saturation: -$amount) instead of desaturate($color, $amount)"
    }
    "opacify" => {
      "Expected color.adjust($color, $alpha: -$amount) instead of opacify($color, $amount)"
    }
    "saturate" => {
      "Expected color.adjust($color, $saturation: $amount) instead of saturate($color, $amount)"
    }
    "transparentize" => {
      "Expected color.adjust($color, $alpha: -$amount) instead of transparentize($color, $amount)"
    }
    // new_rule_names entries (no rule_mapping): Expected module.new_name instead of name
    "adjust-color" => "Expected color.adjust instead of adjust-color",
    "scale-color" => "Expected color.scale instead of scale-color",
    "change-color" => "Expected color.change instead of change-color",
    "map-get" => "Expected map.get instead of map-get",
    "map-merge" => "Expected map.merge instead of map-merge",
    "map-remove" => "Expected map.remove instead of map-remove",
    "map-keys" => "Expected map.keys instead of map-keys",
    "map-values" => "Expected map.values instead of map-values",
    "map-has-key" => "Expected map.has-key instead of map-has-key",
    "str-length" => "Expected string.length instead of str-length",
    "str-insert" => "Expected string.insert instead of str-insert",
    "str-index" => "Expected string.index instead of str-index",
    "str-slice" => "Expected string.slice instead of str-slice",
    "unitless" => "Expected math.is-unitless instead of unitless",
    "comparable" => "Expected math.compatible instead of comparable",
    "list-separator" => "Expected list.separator instead of list-separator",
    "selector-nest" => "Expected selector.nest instead of selector-nest",
    "selector-append" => "Expected selector.append instead of selector-append",
    "selector-replace" => "Expected selector.replace instead of selector-replace",
    "selector-unify" => "Expected selector.unify instead of selector-unify",
    "selector-parse" => "Expected selector.parse instead of selector-parse",
    "selector-extend" => "Expected selector.extend instead of selector-extend",
    "is-superselector" => "Expected selector.is-superselector instead of is-superselector",
    // Remaining: Expected module.name instead of name
    "red" => "Expected color.red instead of red",
    "blue" => "Expected color.blue instead of blue",
    "green" => "Expected color.green instead of green",
    "mix" => "Expected color.mix instead of mix",
    "hue" => "Expected color.hue instead of hue",
    "saturation" => "Expected color.saturation instead of saturation",
    "lightness" => "Expected color.lightness instead of lightness",
    "complement" => "Expected color.complement instead of complement",
    "ie-hex-str" => "Expected color.ie-hex-str instead of ie-hex-str",
    "unquote" => "Expected string.unquote instead of unquote",
    "quote" => "Expected string.quote instead of quote",
    "to-upper-case" => "Expected string.to-upper-case instead of to-upper-case",
    "to-lower-case" => "Expected string.to-lower-case instead of to-lower-case",
    "unique-id" => "Expected string.unique-id instead of unique-id",
    "percentage" => "Expected math.percentage instead of percentage",
    "ceil" => "Expected math.ceil instead of ceil",
    "floor" => "Expected math.floor instead of floor",
    "abs" => "Expected math.abs instead of abs",
    "random" => "Expected math.random instead of random",
    "unit" => "Expected math.unit instead of unit",
    "length" => "Expected list.length instead of length",
    "nth" => "Expected list.nth instead of nth",
    "set-nth" => "Expected list.set-nth instead of set-nth",
    "join" => "Expected list.join instead of join",
    "append" => "Expected list.append instead of append",
    "zip" => "Expected list.zip instead of zip",
    "index" => "Expected list.index instead of index",
    "feature-exists" => "Expected meta.feature-exists instead of feature-exists",
    "variable-exists" => "Expected meta.variable-exists instead of variable-exists",
    "global-variable-exists" => {
      "Expected meta.global-variable-exists instead of global-variable-exists"
    }
    "function-exists" => "Expected meta.function-exists instead of function-exists",
    "mixin-exists" => "Expected meta.mixin-exists instead of mixin-exists",
    "inspect" => "Expected meta.inspect instead of inspect",
    "get-function" => "Expected meta.get-function instead of get-function",
    "type-of" => "Expected meta.type-of instead of type-of",
    "call" => "Expected meta.call instead of call",
    "content-exists" => "Expected meta.content-exists instead of content-exists",
    "keywords" => "Expected meta.keywords instead of keywords",
    "simple-selectors" => "Expected selector.simple-selectors instead of simple-selectors",
    // Fallback (should not be reached for known functions)
    _ => "Expected a Sass module function instead of a global function",
  }
}

/// Returns true if `name` is a deprecated global SCSS function (per stylelint-scss rules).
fn is_deprecated_global(name: &str) -> bool {
  matches!(
    name,
    "abs"
      | "adjust-color"
      | "adjust-hue"
      | "append"
      | "blue"
      | "call"
      | "ceil"
      | "change-color"
      | "comparable"
      | "complement"
      | "content-exists"
      | "darken"
      | "desaturate"
      | "feature-exists"
      | "floor"
      | "function-exists"
      | "get-function"
      | "global-variable-exists"
      | "green"
      | "hue"
      | "ie-hex-str"
      | "index"
      | "inspect"
      | "is-superselector"
      | "join"
      | "keywords"
      | "length"
      | "lighten"
      | "lightness"
      | "list-separator"
      | "map-get"
      | "map-has-key"
      | "map-keys"
      | "map-merge"
      | "map-remove"
      | "map-values"
      | "mix"
      | "mixin-exists"
      | "nth"
      | "opacify"
      | "percentage"
      | "quote"
      | "random"
      | "red"
      | "saturate"
      | "saturation"
      | "scale-color"
      | "selector-append"
      | "selector-extend"
      | "selector-nest"
      | "selector-parse"
      | "selector-replace"
      | "selector-unify"
      | "set-nth"
      | "simple-selectors"
      | "str-index"
      | "str-insert"
      | "str-length"
      | "str-slice"
      | "to-lower-case"
      | "to-upper-case"
      | "transparentize"
      | "type-of"
      | "unique-id"
      | "unit"
      | "unitless"
      | "unquote"
      | "variable-exists"
      | "zip"
  )
}

fn scan_value_for_global_functions(
  rule_name: &'static str,
  severity: Severity,
  value: &str,
  decl_span: gale_css_parser::Span,
  source: &str,
  diagnostics: &mut Vec<Diagnostic>,
) {
  // Compute the byte offset in `source` where the value string starts.
  // The declaration span covers "property: value", so we search for the
  // value within that range to find the exact offset.
  let decl_start = decl_span.offset as usize;
  let decl_end = (decl_span.offset + decl_span.length) as usize;
  let decl_end = decl_end.min(source.len());
  let decl_text = &source[decl_start..decl_end];
  let value_pos_in_decl = decl_text.find(value).unwrap_or(0);

  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  let mut in_single_quote = false;
  let mut in_double_quote = false;

  while i < len {
    // Track string context to skip function names inside strings
    if bytes[i] == b'"' && !in_single_quote {
      in_double_quote = !in_double_quote;
      i += 1;
      continue;
    }
    if bytes[i] == b'\'' && !in_double_quote {
      in_single_quote = !in_single_quote;
      i += 1;
      continue;
    }
    if in_single_quote || in_double_quote {
      i += 1;
      continue;
    }

    // Skip non-alpha/hyphen
    if !bytes[i].is_ascii_alphabetic() && bytes[i] != b'-' {
      i += 1;
      continue;
    }

    // Collect function name
    let start = i;
    while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
      i += 1;
    }

    // Check if followed by `(`
    if i < len && bytes[i] == b'(' {
      let func_name = &value[start..i];

      // Skip namespaced calls (e.g., `math.div(`, `map.get(`)
      let is_namespaced = start > 0 && bytes[start - 1] == b'.';

      if !is_namespaced && is_deprecated_global(func_name) {
        // Compute absolute byte offset of the function name in the source
        let func_abs_offset = decl_start + value_pos_in_decl + start;
        let func_span = Span::new(func_abs_offset, func_name.len());

        diagnostics.push(
          Diagnostic::new(rule_name, function_message(func_name).to_string())
            .severity(severity)
            .span(func_span),
        );
      }
    }

    i += 1;
  }
}

impl Rule for ScssNoGlobalFunctionNames {
  fn name(&self) -> &'static str {
    "scss/no-global-function-names"
  }

  fn description(&self) -> &'static str {
    "Disallow global function names that should use Sass modules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut diagnostics = Vec::new();

    match node {
      CssNode::Declaration(decl) => {
        scan_value_for_global_functions(
          self.name(),
          self.default_severity(),
          &decl.value,
          decl.span,
          ctx.source,
          &mut diagnostics,
        );
      }
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          scan_value_for_global_functions(
            self.name(),
            self.default_severity(),
            &decl.value,
            decl.span,
            ctx.source,
            &mut diagnostics,
          );
        }
      }
      _ => {}
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, Syntax};

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

  fn decl(value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "color".to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn skips_non_scss() {
    assert!(
      ScssNoGlobalFunctionNames
        .check(&decl("adjust-color(red, $red: 10)"), &css_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_deprecated_global() {
    let d = ScssNoGlobalFunctionNames.check(&decl("adjust-color(red, $red: 10)"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("adjust-color"));
  }

  #[test]
  fn reports_map_get() {
    let d = ScssNoGlobalFunctionNames.check(&decl("map-get($map, key)"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("map-get"));
  }

  #[test]
  fn allows_module_function() {
    // `color.adjust()` is not a deprecated global.
    let d = ScssNoGlobalFunctionNames.check(&decl("color.adjust(red, $red: 10)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_namespaced_map_get() {
    // `map.get()` should not be flagged — it uses a namespace
    let d = ScssNoGlobalFunctionNames.check(&decl("map.get($map, key)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_css_native_min_max() {
    // min() and max() are NOT in stylelint-scss deprecated list, not flagged
    let d = ScssNoGlobalFunctionNames.check(&decl("min(100px, 50vw)"), &scss_ctx());
    assert!(d.is_empty());
    let d = ScssNoGlobalFunctionNames.check(&decl("max(100px, 50vw)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_css_native_round() {
    // round() is NOT in stylelint-scss deprecated list, not flagged
    let d = ScssNoGlobalFunctionNames.check(&decl("round(1.5)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_function_in_string() {
    let d = ScssNoGlobalFunctionNames.check(&decl("\"use map-get() instead\""), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_deprecated() {
    let d = ScssNoGlobalFunctionNames.check(&decl("rgba(0, 0, 0, 0.5)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn darken_message_matches_stylelint() {
    let d = ScssNoGlobalFunctionNames.check(&decl("darken($color, 10%)"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected color.adjust($color, $lightness: -$amount) instead of darken($color, $amount)"
    );
  }
}
