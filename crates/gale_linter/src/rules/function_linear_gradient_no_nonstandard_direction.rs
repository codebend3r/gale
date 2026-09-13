use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow non-standard direction values in `linear-gradient()` calls.
///
/// The standard syntax uses `to top`, `to right`, etc. Non-standard
/// (legacy WebKit) syntax uses `top`, `left`, etc. without the `to` keyword.
///
/// This rule only checks **unprefixed** `linear-gradient()` and
/// `repeating-linear-gradient()`.  Vendor-prefixed versions
/// (`-webkit-linear-gradient`, `-moz-linear-gradient`, `-o-linear-gradient`)
/// legitimately use the old syntax, so they are **not** flagged.
///
/// Equivalent to Stylelint's `function-linear-gradient-no-nonstandard-direction` rule.
pub struct FunctionLinearGradientNoNonstandardDirection;

/// Direction keywords that are non-standard when used without `to`
/// in the **standard** (unprefixed) gradient syntax.
const NON_STANDARD_DIRECTIONS: &[&str] = &[
  "top",
  "bottom",
  "left",
  "right",
  "top left",
  "top right",
  "bottom left",
  "bottom right",
  "left top",
  "left bottom",
  "right top",
  "right bottom",
];

/// Unprefixed gradient function names that should use the standard syntax.
const STANDARD_GRADIENT_FUNCTIONS: &[&str] = &["linear-gradient", "repeating-linear-gradient"];

impl Rule for FunctionLinearGradientNoNonstandardDirection {
  fn name(&self) -> &'static str {
    "function-linear-gradient-no-nonstandard-direction"
  }

  fn description(&self) -> &'static str {
    "Disallow direction values in linear-gradient() calls that are not valid according to the standard syntax"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };
    for decl in declarations {
      let decl_start = decl.span.offset;
      let decl_end = decl_start + decl.span.length;
      let search_area = if decl.span.length > 0 && decl_end <= _ctx.source.len() {
        &_ctx.source[decl_start..decl_end]
      } else {
        &decl.value
      };
      check_value(search_area, decl_start, self, &mut diags);
    }
    diags
  }
}

fn check_value(
  value: &str,
  base_offset: usize,
  rule: &FunctionLinearGradientNoNonstandardDirection,
  diags: &mut Vec<Diagnostic>,
) {
  let lower = value.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if bytes[i] == b'(' && i > 0 {
      // Find start of function name.
      let paren_pos = i;
      let mut start = i;
      while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
          || bytes[start - 1] == b'-'
          || bytes[start - 1] == b'_')
      {
        start -= 1;
      }
      let fname = &lower[start..paren_pos];

      // Only check unprefixed standard gradient functions.
      // Vendor-prefixed versions use the old syntax legitimately.
      if STANDARD_GRADIENT_FUNCTIONS.contains(&fname) {
        // Extract the first argument (before the first comma).
        let after_paren = paren_pos + 1;
        // Find matching close paren.
        let mut depth = 1i32;
        let mut close = after_paren;
        for (j, &byte) in bytes.iter().enumerate().take(len).skip(after_paren) {
          if byte == b'(' {
            depth += 1;
          } else if byte == b')' {
            depth -= 1;
            if depth == 0 {
              close = j;
              break;
            }
          }
        }
        // Get first argument (before first comma at depth 0).
        let args_area = &lower[after_paren..close];
        let first_arg = if let Some(comma) = find_top_level_comma(args_area) {
          args_area[..comma].trim()
        } else {
          args_area.trim()
        };

        // Check if the first argument is a non-standard direction.
        if is_nonstandard_direction(first_arg) {
          diags.push(
            Diagnostic::new(
              rule.name(),
              format!("Unexpected non-standard direction \"{}\"", first_arg),
            )
            .severity(rule.default_severity())
            .span(Span::new(base_offset + start, close + 1 - start)),
          );
        }

        i = close + 1;
        continue;
      }
    }
    i += 1;
  }
}

/// Check if a first argument is a non-standard direction.
///
/// Non-standard means:
/// - A bare direction keyword like `top`, `bottom`, `left`, `right`
///   (including two-word combos like `top right`)
/// - A `to` keyword followed by an angle like `to 90deg` (this is also invalid)
///
/// Standard means:
/// - `to top`, `to bottom`, `to left`, `to right`, `to top right`, etc.
/// - An angle like `45deg`, `1turn`, `100grad`, etc.
/// - A color (starts a color stop directly, no direction)
fn is_nonstandard_direction(first_arg: &str) -> bool {
  let trimmed = first_arg.trim();

  if trimmed.is_empty() {
    return false;
  }

  // Check for bare direction keywords (non-standard)
  if NON_STANDARD_DIRECTIONS.contains(&trimmed) {
    return true;
  }

  // "to" followed by angle is invalid too (e.g., "to 90deg")
  if let Some(after_to) = trimmed.strip_prefix("to ") {
    let after_to = after_to.trim();
    if looks_like_angle(after_to) {
      return true;
    }
  }

  false
}

/// Check if a string looks like a CSS angle value.
fn looks_like_angle(s: &str) -> bool {
  let s = s.trim();
  if s.is_empty() {
    return false;
  }
  // Angles end with deg, grad, rad, or turn
  let units = ["deg", "grad", "rad", "turn"];
  for unit in &units {
    if let Some(num_part) = s.strip_suffix(unit) {
      let num_part = num_part.trim();
      if !num_part.is_empty()
        && num_part
          .bytes()
          .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+')
      {
        return true;
      }
    }
  }
  false
}

/// Find the position of the first comma at depth 0 (not inside nested parens).
fn find_top_level_comma(s: &str) -> Option<usize> {
  let mut depth = 0i32;
  for (i, b) in s.bytes().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => depth -= 1,
      b',' if depth == 0 => return Some(i),
      _ => {}
    }
  }
  None
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

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "background".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, val.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  fn decl_with_value(val: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "background".to_string(),
      value: val.to_string(),
      span: ParserSpan::new(0, val.len()),
      important: false,
    })
  }

  #[test]
  fn reports_nonstandard_direction() {
    let d = FunctionLinearGradientNoNonstandardDirection
      .check(&style_with_value("linear-gradient(top, red, blue)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("top"));
  }

  #[test]
  fn accepts_standard_direction() {
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &style_with_value("linear-gradient(to top, red, blue)"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn accepts_angle() {
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &style_with_value("linear-gradient(45deg, red, blue)"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn reports_nonstandard_in_declaration_node() {
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &decl_with_value("linear-gradient(bottom, red, blue)"),
      &ctx(),
    );
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_two_word_direction() {
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &style_with_value("linear-gradient(top right, red, blue)"),
      &ctx(),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("top right"));
  }

  #[test]
  fn ignores_non_gradient() {
    let d = FunctionLinearGradientNoNonstandardDirection
      .check(&style_with_value("rgb(255, 0, 0)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_vendor_prefixed_gradient() {
    // Vendor-prefixed gradients use the old syntax legitimately
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &style_with_value("-webkit-linear-gradient(top, red, blue)"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_moz_prefixed_gradient() {
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &style_with_value("-moz-linear-gradient(top, red, blue)"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn reports_repeating_linear_gradient() {
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &style_with_value("repeating-linear-gradient(top, red, blue)"),
      &ctx(),
    );
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn accepts_standard_repeating() {
    let d = FunctionLinearGradientNoNonstandardDirection.check(
      &style_with_value("repeating-linear-gradient(to top, red, blue)"),
      &ctx(),
    );
    assert!(d.is_empty());
  }
}
