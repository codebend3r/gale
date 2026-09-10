use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require modern or legacy color function notation.
///
/// In modern CSS, `rgb()` accepts an optional alpha channel as the 4th argument,
/// making `rgba()` redundant. The same applies to `hsl()` vs `hsla()`.
///
/// Equivalent to Stylelint's `color-function-alias-notation` rule with "modern" option.
/// In "modern" mode, flags usage of `rgba()` (prefer `rgb()`) and `hsla()` (prefer `hsl()`).
pub struct ColorFunctionAliasNotation;

const ALIAS_FUNCTIONS: &[(&str, &str)] = &[("rgba(", "rgb"), ("hsla(", "hsl")];

/// Check whether the function call starting at `fn_start` in `value`
/// contains any SCSS-specific arguments that make it non-standard syntax.
///
/// Stylelint's `isStandardSyntaxColorFunction` returns `false` when any
/// function argument starts with `#` (including `#{...}` interpolation) or
/// `$` (SCSS variable), or includes `.$` (chained variable). In those cases
/// the function is not checked.
fn has_scss_args(value: &str, fn_start: usize) -> bool {
  // Find the opening parenthesis
  let after_fn = &value[fn_start..];
  let paren_pos = match after_fn.find('(') {
    Some(p) => fn_start + p + 1,
    None => return false,
  };

  // Find the matching closing parenthesis
  let mut depth = 1i32;
  let mut end = paren_pos;
  let bytes = value.as_bytes();
  while end < bytes.len() && depth > 0 {
    match bytes[end] {
      b'(' => depth += 1,
      b')' => depth -= 1,
      _ => {}
    }
    end += 1;
  }

  let args = &value[paren_pos..end.saturating_sub(1)];

  // Check each comma-separated argument for SCSS syntax
  for arg in args.split(',') {
    let trimmed = arg.trim();
    // SCSS interpolation #{...} or hex starting with #
    if trimmed.starts_with('#') {
      return true;
    }
    // SCSS variable $var
    if trimmed.starts_with('$') {
      return true;
    }
    // Chained variable .$
    if trimmed.contains(".$") {
      return true;
    }
  }
  false
}

impl Rule for ColorFunctionAliasNotation {
  fn name(&self) -> &'static str {
    "color-function-alias-notation"
  }

  fn description(&self) -> &'static str {
    "Specify modern or legacy notation for color function aliases"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };
    let mut diags = Vec::new();
    let mut seen_offsets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for decl in decls {
      // Use source text to avoid lightningcss normalization of color functions
      let decl_start = decl.span.offset;
      let decl_end = (decl_start + decl.span.length).min(ctx.source.len());
      let search_area = if decl_end > decl_start && decl_end <= ctx.source.len() {
        &ctx.source[decl_start..decl_end]
      } else {
        &decl.value
      };
      let lower = search_area.to_ascii_lowercase();

      for &(alias, modern) in ALIAS_FUNCTIONS {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(alias) {
          let abs_pos = search_from + pos;
          let legacy = &alias[..alias.len() - 1]; // "rgba" or "hsla"

          // Skip functions with SCSS-specific args (variables, interpolation).
          if has_scss_args(&lower, abs_pos) {
            search_from = abs_pos + 1;
            continue;
          }

          let fn_offset = decl_start + abs_pos;
          if !seen_offsets.insert(fn_offset) {
            search_from = abs_pos + 1;
            continue;
          }
          let fn_len = legacy.len();
          diags.push(
            Diagnostic::new(
              self.name(),
              format!("Expected \"{legacy}\" to be \"{modern}\""),
            )
            .severity(self.default_severity())
            .span(Span::new(fn_offset, fn_len)),
          );
          search_from = abs_pos + 1;
        }
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

  fn style_with_value(value: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: value.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_rgba() {
    let d = ColorFunctionAliasNotation.check(&style_with_value("rgba(0, 0, 0, 0.5)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("\"rgba\" to be \"rgb\""));
  }

  #[test]
  fn reports_hsla() {
    let d = ColorFunctionAliasNotation.check(&style_with_value("hsla(0, 100%, 50%, 0.8)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("\"hsla\" to be \"hsl\""));
  }

  #[test]
  fn allows_rgb() {
    let d = ColorFunctionAliasNotation.check(&style_with_value("rgb(0 0 0)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_hsl() {
    let d = ColorFunctionAliasNotation.check(&style_with_value("hsl(0 100% 50%)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_rgb_with_alpha() {
    let d = ColorFunctionAliasNotation.check(&style_with_value("rgb(0 0 0 / 0.5)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_multiple_aliases() {
    let d = ColorFunctionAliasNotation.check(
      &style_with_value("rgba(0, 0, 0, 1), hsla(0, 0%, 0%, 1)"),
      &ctx(),
    );
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn skips_rgba_with_scss_variable() {
    let d = ColorFunctionAliasNotation.check(&style_with_value("rgba($danger, 0.7)"), &ctx());
    assert!(
      d.is_empty(),
      "rgba() with SCSS variable arg should be skipped"
    );
  }

  #[test]
  fn skips_rgba_with_scss_interpolation() {
    let d =
      ColorFunctionAliasNotation.check(&style_with_value("rgba(0, 0, 0, #{$opacity})"), &ctx());
    assert!(
      d.is_empty(),
      "rgba() with SCSS interpolation should be skipped"
    );
  }

  #[test]
  fn still_reports_plain_rgba() {
    let d = ColorFunctionAliasNotation.check(&style_with_value("rgba(0, 0, 0, 0.5)"), &ctx());
    assert_eq!(d.len(), 1);
  }
}
