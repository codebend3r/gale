use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before the commas of functions.
///
/// Primary: "always" | "never"
pub struct StylisticFunctionCommaSpaceBefore;

impl Rule for StylisticFunctionCommaSpaceBefore {
  fn name(&self) -> &'static str {
    "@stylistic/function-comma-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before the commas of functions"
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
        let paren_start = i;
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
        let paren_end = j;

        let mut k = paren_start + 1;
        let mut inner_depth = 0;
        while k < paren_end {
          if bytes[k] == b'(' {
            inner_depth += 1;
          } else if bytes[k] == b')' {
            if inner_depth > 0 {
              inner_depth -= 1;
            }
          } else if bytes[k] == b'\'' || bytes[k] == b'"' {
            let q = bytes[k];
            k += 1;
            while k < paren_end && bytes[k] != q {
              if bytes[k] == b'\\' {
                k += 1;
              }
              k += 1;
            }
          } else if bytes[k] == b',' && inner_depth == 0 {
            let comma_pos = k;
            let has_space_before = comma_pos > 0 && bytes[comma_pos - 1] == b' ';

            let violation = match option {
              "always" => !has_space_before,
              "never" => has_space_before,
              _ => false,
            };

            if violation {
              let msg = match option {
                "always" => "Expected single space before \",\"",
                "never" => "Unexpected whitespace before \",\"",
                "always-single-line" => {
                  "Expected single space before \",\" in a single-line function"
                }
                _ => "Expected single space before \",\"",
              };
              diagnostics.push(
                Diagnostic::new(self.name(), msg)
                  .severity(self.default_severity())
                  .span(Span::new(comma_pos, 1)),
              );
            }
          }
          k += 1;
        }
        // Advance past the opening paren only — do NOT skip to paren_end.
        // This lets nested function calls be individually detected and checked.
        i = paren_start + 1;
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
    let rule = StylisticFunctionCommaSpaceBefore;
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
  fn never_accepts_no_space_before_comma() {
    let d = check("a { transform: translate(1px, 2px); }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_before_comma() {
    let d = check("a { transform: translate(1px , 2px); }", "never");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn always_accepts_space_before_comma() {
    let d = check("a { transform: translate(1px ,2px); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_before_comma() {
    let d = check("a { transform: translate(1px,2px); }", "always");
    assert_eq!(d.len(), 1);
  }
}
