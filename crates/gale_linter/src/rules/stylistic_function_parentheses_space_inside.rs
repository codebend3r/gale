use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space inside the parentheses of functions.
///
/// Primary: "always" | "never" | "always-single-line"
pub struct StylisticFunctionParenthesesSpaceInside;

impl Rule for StylisticFunctionParenthesesSpaceInside {
  fn name(&self) -> &'static str {
    "@stylistic/function-parentheses-space-inside"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space inside the parentheses of functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = ctx.primary_option_str().unwrap_or("never");
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
        continue;
      }
      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }
      // Skip SCSS interpolation #{...}
      if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }
      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      // Detect function call
      if bytes[i] == b'('
        && i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        // Check if this is a pseudo-class/element function like :not(), :is(), :where()
        let mut is_pseudo_fn = false;
        {
          let mut p = i - 1;
          while p > 0 && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
          {
            p -= 1;
          }
          if bytes[p] == b':' {
            is_pseudo_fn = true;
          }
        }

        // Check if this is a SCSS module function call like map.has-key(), color.adjust()
        let mut is_scss_module_fn = false;
        {
          let mut p = i - 1;
          // Walk back over the function name part (after the dot)
          while p > 0 && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
          {
            p -= 1;
          }
          // If we hit a dot and there's a namespace before it, it's a SCSS module call
          if bytes[p] == b'.'
            && p > 0
            && (bytes[p - 1].is_ascii_alphanumeric()
              || bytes[p - 1] == b'-'
              || bytes[p - 1] == b'_')
          {
            is_scss_module_fn = true;
          }
        }

        // Check if this is an SCSS at-rule like @include mixin(), @if(), @each, @for, @while
        let mut is_at_rule_paren = false;
        {
          let mut p = i - 1;
          // Walk back over the function/mixin name
          while p > 0 && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
          {
            p -= 1;
          }
          if bytes[p] == b'@' {
            is_at_rule_paren = true;
          } else {
            // Skip whitespace (for `@include mixin(...)` pattern)
            while p > 0 && (bytes[p] == b' ' || bytes[p] == b'\t') {
              p -= 1;
            }
            // Walk back over the at-rule keyword (e.g., "include")
            while p > 0
              && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
            {
              p -= 1;
            }
            if bytes[p] == b'@' {
              is_at_rule_paren = true;
            }
          }
        }
        let open_paren = i;
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

        // Skip pseudo-class functions, SCSS at-rule parens, and SCSS module function calls
        if is_pseudo_fn || is_at_rule_paren || is_scss_module_fn {
          i = close_paren + 1;
          continue;
        }

        // Skip empty function calls `fn()`
        if close_paren == open_paren + 1 {
          i = close_paren + 1;
          continue;
        }

        let func_content = &ctx.source[open_paren..=close_paren.min(len - 1)];
        let is_single_line = !func_content.contains('\n');

        let after_open = open_paren + 1;
        let before_close = close_paren.saturating_sub(1);

        let has_space_after_open = after_open < len && bytes[after_open] == b' ';
        let has_space_before_close = before_close > open_paren && bytes[before_close] == b' ';

        let should_check = match option {
          "always" => true,
          "never" => true,
          "always-single-line" => is_single_line,
          _ => false,
        };

        if should_check {
          let expect_space = matches!(option, "always" | "always-single-line");

          if expect_space && !has_space_after_open {
            diagnostics.push(
              Diagnostic::new(self.name(), "Expected a space after \"(\" in function")
                .severity(self.default_severity())
                .span(Span::new(open_paren, 1)),
            );
          } else if !expect_space && has_space_after_open {
            diagnostics.push(
              Diagnostic::new(self.name(), "Unexpected space after \"(\" in function")
                .severity(self.default_severity())
                .span(Span::new(open_paren, 1)),
            );
          }

          if expect_space && !has_space_before_close {
            diagnostics.push(
              Diagnostic::new(self.name(), "Expected a space before \")\" in function")
                .severity(self.default_severity())
                .span(Span::new(close_paren, 1)),
            );
          } else if !expect_space && has_space_before_close {
            diagnostics.push(
              Diagnostic::new(self.name(), "Unexpected space before \")\" in function")
                .severity(self.default_severity())
                .span(Span::new(close_paren, 1)),
            );
          }
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
    let rule = StylisticFunctionParenthesesSpaceInside;
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
  fn never_accepts_no_space_inside() {
    let d = check("a { transform: translate(1px, 2px); }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_inside() {
    let d = check("a { transform: translate( 1px, 2px ); }", "never");
    assert_eq!(d.len(), 2); // after ( and before )
  }

  #[test]
  fn always_accepts_space_inside() {
    let d = check("a { transform: translate( 1px, 2px ); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_inside() {
    let d = check("a { transform: translate(1px, 2px); }", "always");
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn ignores_pseudo_class_functions() {
    let d = check("a:not(.foo) { color: red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_at_include_parens() {
    let d = check("a { @include mixin( $a ); }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_scss_interpolation() {
    // #{...} should be skipped entirely
    let d = check("a { color: #{fn( $a )}; }", "never");
    assert!(d.is_empty());
  }
}
