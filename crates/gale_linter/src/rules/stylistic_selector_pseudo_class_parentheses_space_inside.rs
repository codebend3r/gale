use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space inside the parentheses of pseudo-class selectors.
///
/// Equivalent to `@stylistic/selector-pseudo-class-parentheses-space-inside`.
pub struct StylisticSelectorPseudoClassParenthesesSpaceInside;

impl Rule for StylisticSelectorPseudoClassParenthesesSpaceInside {
  fn name(&self) -> &'static str {
    "@stylistic/selector-pseudo-class-parentheses-space-inside"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space inside the parentheses of pseudo-class selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("never");
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;

    while i < len {
      // Skip strings
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        i += 1;
        continue;
      }

      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
        continue;
      }

      // Detect pseudo-class: a single colon followed by an identifier and then '('
      if bytes[i] == b':' && (i == 0 || bytes[i - 1] != b':') {
        let colon_pos = i;
        i += 1;
        // Skip if it's a double colon (pseudo-element)
        if i < len && bytes[i] == b':' {
          i += 1;
          continue;
        }
        // Read pseudo-class name
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
          i += 1;
        }
        if i >= len || bytes[i] != b'(' || i == name_start {
          continue;
        }

        // Found a pseudo-class with parentheses
        let open_paren = i;
        // Find matching close paren
        let mut depth = 1;
        let mut j = i + 1;
        while j < len && depth > 0 {
          if bytes[j] == b'(' {
            depth += 1;
          } else if bytes[j] == b')' {
            depth -= 1;
          } else if bytes[j] == b'\'' || bytes[j] == b'"' {
            let q = bytes[j];
            j += 1;
            while j < len && bytes[j] != q {
              if bytes[j] == b'\\' {
                j += 1;
              }
              j += 1;
            }
          }
          if depth > 0 {
            j += 1;
          }
        }
        let close_paren = j;

        // Skip empty pseudo-class parens
        if close_paren == open_paren + 1 {
          i = close_paren + 1;
          continue;
        }

        let after_open = open_paren + 1;
        let before_close = close_paren.saturating_sub(1);

        let has_space_after_open = after_open < len && bytes[after_open] == b' ';
        let has_space_before_close = before_close > open_paren && bytes[before_close] == b' ';

        let expect_space = option == "always";

        if expect_space && !has_space_after_open {
          diagnostics.push(
            Diagnostic::new(self.name(), "Expected a space after \"(\"")
              .severity(self.default_severity())
              .span(Span::new(open_paren, 1)),
          );
        } else if !expect_space && has_space_after_open {
          diagnostics.push(
            Diagnostic::new(self.name(), "Unexpected space after \"(\"")
              .severity(self.default_severity())
              .span(Span::new(open_paren, 1)),
          );
        }

        if expect_space && !has_space_before_close {
          diagnostics.push(
            Diagnostic::new(self.name(), "Expected a space before \")\"")
              .severity(self.default_severity())
              .span(Span::new(close_paren, 1)),
          );
        } else if !expect_space && has_space_before_close {
          diagnostics.push(
            Diagnostic::new(self.name(), "Unexpected space before \")\"")
              .severity(self.default_severity())
              .span(Span::new(close_paren, 1)),
          );
        }

        i = close_paren + 1;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticSelectorPseudoClassParenthesesSpaceInside;
    let opts = serde_json::json!(option);
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn never_accepts_no_space() {
    let d = check("a:not(.foo) { }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_inside() {
    let d = check("a:not( .foo ) { }", "never");
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn always_accepts_space_inside() {
    let d = check("a:not( .foo ) { }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space() {
    let d = check("a:not(.foo) { }", "always");
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn skips_pseudo_elements() {
    // Double colon is a pseudo-element, not a pseudo-class
    let d = check("a::slotted(.foo) { }", "never");
    assert!(d.is_empty());
  }
}
