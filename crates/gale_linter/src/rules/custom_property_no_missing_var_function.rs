use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports when a custom property (e.g. `--my-color`) is used as a declaration
/// value without being wrapped in a `var()` function.
///
/// Equivalent to Stylelint's `custom-property-no-missing-var-function` rule.
pub struct CustomPropertyNoMissingVarFunction;

/// Check if a value contains a custom property reference (`--something`) that is
/// not wrapped in `var(...)`.
fn has_bare_custom_property(value: &str) -> bool {
  // If the trimmed value starts with `--`, it's likely a bare custom property
  // reference used directly as a value (e.g., `color: --my-color`).
  let trimmed = value.trim();
  if trimmed.starts_with("--") {
    return true;
  }

  // Also check for bare `--` tokens that are NOT inside `var(...)` or parentheses.
  // Strategy: remove all `var(...)` occurrences then check for remaining `--` tokens
  // that are not inside parentheses (non-var parens like `(--x, 0.72)` are not
  // custom property references).
  let without_var = remove_var_functions(trimmed);
  let chars_vec: Vec<char> = without_var.chars().collect();
  let mut paren_depth = 0i32;
  for (idx, ch) in chars_vec.iter().enumerate() {
    if *ch == '(' {
      paren_depth += 1;
    } else if *ch == ')' {
      paren_depth -= 1;
    }
    if *ch == '-'
      && idx + 1 < chars_vec.len()
      && chars_vec[idx + 1] == '-'
      && (idx == 0 || !chars_vec[idx - 1].is_ascii_alphanumeric())
      && paren_depth == 0
    {
      return true;
    }
  }

  false
}

/// Remove `var(...)` function calls from a string, handling nested parentheses.
fn remove_var_functions(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    // Check for "var("
    if i + 4 <= len
      && chars[i] == 'v'
      && chars[i + 1] == 'a'
      && chars[i + 2] == 'r'
      && chars[i + 3] == '('
    {
      let mut depth = 1;
      let mut j = i + 4;
      while j < len && depth > 0 {
        if chars[j] == '(' {
          depth += 1;
        } else if chars[j] == ')' {
          depth -= 1;
        }
        j += 1;
      }
      i = j;
    } else {
      result.push(chars[i]);
      i += 1;
    }
  }

  result
}

impl Rule for CustomPropertyNoMissingVarFunction {
  fn name(&self) -> &'static str {
    "custom-property-no-missing-var-function"
  }

  fn description(&self) -> &'static str {
    "Disallow missing var function for custom properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let style = match node {
      CssNode::Style(s) => s,
      _ => return vec![],
    };

    let mut diagnostics = Vec::new();

    for decl in &style.declarations {
      // Skip custom property definitions (e.g., `--my-color: red`).
      if decl.property.starts_with("--") {
        continue;
      }

      // Skip values containing SCSS/Less interpolation — the `--`
      // may be part of an interpolated identifier, not a bare custom
      // property reference.
      if decl.value.contains("#{") || decl.value.contains("@{") {
        continue;
      }

      if has_bare_custom_property(&decl.value) {
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            "Unexpected missing var function for custom property",
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_bare_custom_property_in_value() {
    let rule = CustomPropertyNoMissingVarFunction;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "--my-color".to_string(),
        span: ParserSpan::new(4, 18),
        important: false,
      }],
      span: ParserSpan::new(0, 24),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(
      diags[0]
        .message
        .contains("missing var function for custom property")
    );
  }

  #[test]
  fn ignores_custom_property_inside_var() {
    let rule = CustomPropertyNoMissingVarFunction;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "var(--my-color)".to_string(),
        span: ParserSpan::new(4, 22),
        important: false,
      }],
      span: ParserSpan::new(0, 28),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_custom_property_definitions() {
    let rule = CustomPropertyNoMissingVarFunction;
    let node = CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![Declaration {
        property: "--my-color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(8, 16),
        important: false,
      }],
      span: ParserSpan::new(0, 26),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }
}
