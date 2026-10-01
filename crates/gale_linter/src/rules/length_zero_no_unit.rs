use std::sync::OnceLock;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern::option_matches;
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext};
use crate::value_parser::{self, ValueNode};

/// Disallow units for zero lengths (`0px` → `0`).
///
/// Equivalent to Stylelint's `length-zero-no-unit` rule, autofix included.
/// Secondary options:
///   - `ignore`: `custom-properties`
///   - `ignoreFunctions`: names or `/regex/` entries; nothing inside a
///     matching function is checked
///   - `ignorePreludeOfAtRules`: at-rule names or `/regex/` entries whose
///     params are not checked
///
/// Like Stylelint, it walks every declaration value and at-rule prelude in
/// the [`PostcssTree`] of the source with postcss-value-parser, so values
/// the CSS parser rejects, declarations directly inside at-rules and Sass
/// `@include` arguments are checked too.  `line-height` and `flex` values,
/// the line height in a `font` shorthand and everything inside math
/// functions keep their units.
pub struct LengthZeroNoUnit;

/// Stylelint's `lengthUnits`.
const LENGTH_UNITS: &[&str] = &[
  "cap", "ch", "em", "ex", "ic", "lh", "rcap", "rch", "rem", "rex", "ric", "rlh", "dvb", "dvh",
  "dvi", "dvmax", "dvmin", "dvw", "lvb", "lvh", "lvi", "lvmax", "lvmin", "lvw", "svb", "svh",
  "svi", "svmax", "svmin", "svw", "vb", "vh", "vi", "vw", "vmin", "vmax", "vm", "px", "mm", "cm",
  "in", "pt", "pc", "q", "mozmm", "fr", "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax",
];

/// Stylelint's `mathFunctions`.
const MATH_FUNCTIONS: &[&str] = &[
  "abs",
  "acos",
  "asin",
  "atan",
  "calc",
  "cos",
  "exp",
  "sign",
  "sin",
  "sqrt",
  "tan",
  "atan2",
  "calc-size",
  "clamp",
  "hypot",
  "log",
  "max",
  "min",
  "mod",
  "pow",
  "rem",
  "round",
];

/// Stylelint's `mayIncludeRegexes.zeroLength`: a quick test for a zero
/// length anywhere in the text.
fn may_include_zero_length(text: &str) -> bool {
  static ZERO_LENGTH: OnceLock<regex::Regex> = OnceLock::new();
  ZERO_LENGTH
    .get_or_init(|| {
      let units = LENGTH_UNITS.join("|");
      regex::Regex::new(&format!(r"(?i)\b[+-]?(?:0+|0*\.\d+)(?:{units})\b")).expect("valid regex")
    })
    .is_match(text)
}

/// The rule's secondary options.
struct Options<'a> {
  ignore_custom_properties: bool,
  ignore_functions: Option<&'a serde_json::Value>,
  ignore_prelude_of_at_rules: Option<&'a serde_json::Value>,
}

impl Rule for LengthZeroNoUnit {
  fn name(&self) -> &'static str {
    "length-zero-no-unit"
  }

  fn description(&self) -> &'static str {
    "Disallow units for zero lengths"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks every at-rule prelude, then every declaration value, as
  /// Stylelint's `walkAtRules` and `walkDecls` do.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let secondary = ctx.secondary_options();
    let options = Options {
      ignore_custom_properties: option_matches(
        secondary.and_then(|s| s.get("ignore")),
        "custom-properties",
      ),
      ignore_functions: secondary.and_then(|s| s.get("ignoreFunctions")),
      ignore_prelude_of_at_rules: secondary.and_then(|s| s.get("ignorePreludeOfAtRules")),
    };
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if node.kind != NodeKind::AtRule
        || !may_include_zero_length(&node.params)
        || !tree.is_standard_syntax_at_rule(i)
        || option_matches(options.ignore_prelude_of_at_rules, &node.name)
      {
        continue;
      }
      self.check_value(ctx, &tree, i, &options, false, &mut diags);
    }

    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if node.kind != NodeKind::Decl
        || !may_include_zero_length(&node.value)
        || !tree.is_standard_syntax_declaration(i)
      {
        continue;
      }
      let prop = node.name.to_ascii_lowercase();
      if prop == "line-height"
        || prop == "flex"
        || (options.ignore_custom_properties && node.name.starts_with("--"))
      {
        continue;
      }
      self.check_value(ctx, &tree, i, &options, prop == "font", &mut diags);
    }
    diags
  }
}

impl LengthZeroNoUnit {
  /// Walk the raw value (or prelude) of node `i` and report every zero
  /// length with a unit.  `font` skips the word right after a `/`, the
  /// line height.
  fn check_value(
    &self,
    ctx: &RuleContext,
    tree: &PostcssTree,
    i: usize,
    options: &Options,
    font: bool,
    diags: &mut Vec<Diagnostic>,
  ) {
    let span = &tree.nodes[i].value_span;
    let Some(text) = ctx.source_slice(span.start, span.end) else {
      return;
    };
    let parsed = value_parser::parse(text);
    self.walk(&parsed, span.start, options, font, diags);
  }

  /// postcss-value-parser's `walk` over `nodes`, with Stylelint's `check`
  /// for each node.  `offset` is where the parsed text starts.
  fn walk(
    &self,
    nodes: &[ValueNode],
    offset: usize,
    options: &Options,
    font: bool,
    diags: &mut Vec<Diagnostic>,
  ) {
    for (index, node) in nodes.iter().enumerate() {
      let after_slash = font && index > 0 && nodes[index - 1].is_slash();
      if !after_slash && !self.check(node, offset, options, diags) {
        continue;
      }
      if node.is_function() {
        self.walk(&node.nodes, offset, options, font, diags);
      }
    }
  }

  /// Stylelint's `check` for one value node: report a zero length with a
  /// unit.  Returns `false` to skip a function's arguments (math functions
  /// and `ignoreFunctions`).
  fn check(
    &self,
    node: &ValueNode,
    offset: usize,
    options: &Options,
    diags: &mut Vec<Diagnostic>,
  ) -> bool {
    if node.is_function() {
      let name = node.value.to_ascii_lowercase();
      return !(MATH_FUNCTIONS.contains(&name.as_str())
        || option_matches(options.ignore_functions, node.value));
    }
    if !node.is_word() {
      return true;
    }
    let Some((number, unit)) = value_parser::unit(node.value) else {
      return true;
    };
    let unit_lower = unit.to_ascii_lowercase();
    if !is_zero(number)
      || unit.is_empty()
      || !LENGTH_UNITS.contains(&unit_lower.as_str())
      || unit_lower == "fr"
    {
      return true;
    }
    let start = offset + node.source_index;
    let index = start + number.len();
    // The fix keeps the number, without the leading `.` of `.0`.
    let fixed = number.strip_prefix('.').unwrap_or(number);
    diags.push(
      Diagnostic::new(self.name(), "Unexpected unit")
        .severity(self.default_severity())
        .span(Span::from_range(index, index + unit.len()))
        .fix(Fix::new(
          "Remove the unit",
          vec![Edit::new(
            Span::from_range(start, index + unit.len()),
            fixed.to_string(),
          )],
        )),
    );
    true
  }
}

/// JavaScript's `Number.parseFloat(number) === 0` for the number part of a
/// dimension (digits, an optional sign, fraction and exponent).
fn is_zero(number: &str) -> bool {
  let mantissa = number
    .split(['e', 'E'])
    .next()
    .unwrap_or("")
    .trim_start_matches(['+', '-']);
  mantissa.bytes().any(|b| b.is_ascii_digit()) && mantissa.bytes().all(|b| b == b'0' || b == b'.')
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::empty_lines::fix_with;
  use gale_css_parser::Syntax;

  /// The offsets of the units reported in `source`.
  fn reports(source: &str, syntax: Syntax, options: Option<serde_json::Value>) -> Vec<usize> {
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax,
      options: options.as_ref(),
      cache: None,
    };
    LengthZeroNoUnit
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.span.offset)
      .collect()
  }

  /// `source` fixed with the rule on.
  fn fix(source: &str, syntax: Syntax) -> String {
    fix_with(
      "length-zero-no-unit",
      serde_json::json!(true),
      source,
      syntax,
    )
  }

  #[test]
  fn reports_the_unit_of_a_zero_length() {
    assert_eq!(reports("a { margin: 0px; }", Syntax::Css, None), vec![13]);
    assert!(
      reports(
        "a { margin: 0; top: 10px; transition: 0s; }",
        Syntax::Css,
        None
      )
      .is_empty()
    );
    assert!(reports("a { grid-template-columns: 0fr; }", Syntax::Css, None).is_empty());
    assert_eq!(reports("a { --x: 0px; }", Syntax::Css, None).len(), 1);
    let ignore = serde_json::json!([true, { "ignore": ["custom-properties"] }]);
    assert!(reports("a { --x: 0px; }", Syntax::Css, Some(ignore)).is_empty());
  }

  #[test]
  fn keeps_units_where_they_matter() {
    assert!(reports("a { line-height: 0px; flex: 1 1 0px; }", Syntax::Css, None).is_empty());
    assert!(
      reports(
        "a { padding: calc(0px + 1px) min(0in, 1px); }",
        Syntax::Css,
        None
      )
      .is_empty()
    );
    assert_eq!(
      fix("a { font: normal 400 0px / 0px cursive; }", Syntax::Css),
      "a { font: normal 400 0 / 0px cursive; }"
    );
    let ignore = serde_json::json!([true, { "ignoreFunctions": ["/^--/", "var"] }]);
    assert!(
      reports(
        "a { top: var(--a, 0px) --b(0px); }",
        Syntax::Css,
        Some(ignore)
      )
      .is_empty()
    );
  }

  #[test]
  fn checks_preludes_and_declarations_the_css_parser_drops() {
    assert_eq!(
      fix(
        "@media (min-width: 0px /* c */) { a { top: 0em } }",
        Syntax::Css
      ),
      "@media (min-width: 0 /* c */) { a { top: 0 } }"
    );
    assert_eq!(
      fix("@include border-radius($r: 0px);", Syntax::Scss),
      "@include border-radius($r: 0);"
    );
    assert_eq!(
      fix("padding: calc(1in + 0in) 0px;", Syntax::Css),
      "padding: calc(1in + 0in) 0;"
    );
    assert_eq!(
      fix("a { grid-template-columns: 0px 0fr 1fr };", Syntax::Css),
      "a { grid-template-columns: 0 0fr 1fr };"
    );
    let ignore = serde_json::json!([true, { "ignorePreludeOfAtRules": ["media"] }]);
    assert!(reports("@media (min-width: 0px) {}", Syntax::Css, Some(ignore)).is_empty());
  }

  #[test]
  fn fix_keeps_the_number_as_written() {
    assert_eq!(
      fix("a { top: 0.000px; left: .0em; right: -0PX; }", Syntax::Css),
      "a { top: 0.000; left: 0; right: -0; }"
    );
    assert_eq!(
      fix("a { margin: 0px #{$var} 0px; }", Syntax::Scss),
      "a { margin: 0 #{$var} 0; }"
    );
  }

  #[test]
  fn zero_follows_parse_float() {
    assert!(is_zero("0"));
    assert!(is_zero("-0.00"));
    assert!(is_zero(".0"));
    assert!(is_zero("0e5"));
    assert!(!is_zero("0.5"));
    assert!(!is_zero("10"));
  }
}
