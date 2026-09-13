use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of adjacent empty lines within functions.
///
/// Equivalent to `@stylistic/function-max-empty-lines`.
pub struct StylisticFunctionMaxEmptyLines;

impl Rule for StylisticFunctionMaxEmptyLines {
  fn name(&self) -> &'static str {
    "@stylistic/function-max-empty-lines"
  }

  fn description(&self) -> &'static str {
    "Limit the number of adjacent empty lines within functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let max = context
      .primary_option()
      .and_then(|v| v.as_u64())
      .unwrap_or(0) as usize;

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

      // Detect function call: identifier followed by '('
      if bytes[i] == b'('
        && i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        let open_paren = i;
        let mut depth = 1;
        let mut j = i + 1;
        // Scan inside the function for empty lines
        while j < len && depth > 0 {
          if bytes[j] == b'(' {
            depth += 1;
          } else if bytes[j] == b')' {
            depth -= 1;
            if depth == 0 {
              break;
            }
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
          j += 1;
        }
        let close_paren = j;

        // Now scan the content between parens for consecutive empty lines
        let mut k = open_paren + 1;
        while k < close_paren {
          if bytes[k] == b'\n' {
            let line_start = k;
            k += 1;
            let mut consecutive_empty = 0;
            while k < close_paren {
              let mut m = k;
              while m < close_paren && (bytes[m] == b' ' || bytes[m] == b'\t' || bytes[m] == b'\r')
              {
                m += 1;
              }
              if m < close_paren && bytes[m] == b'\n' {
                consecutive_empty += 1;
                k = m + 1;
              } else {
                break;
              }
            }

            if consecutive_empty > max {
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Expected no more than {} empty line(s)", max),
                )
                .severity(self.default_severity())
                .span(Span::new(line_start, k - line_start)),
              );
            }
            continue;
          }
          k += 1;
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

  fn check(source: &str, max: u64) -> Vec<Diagnostic> {
    let rule = StylisticFunctionMaxEmptyLines;
    let opts = serde_json::json!(max);
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn allows_no_empty_lines() {
    let d = check("a { transform: translate(\n1px,\n2px\n); }", 0);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_empty_line_in_function_when_max_zero() {
    let d = check("a { transform: translate(\n1px,\n\n2px\n); }", 0);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 0"));
  }

  #[test]
  fn allows_one_empty_line_when_max_one() {
    let d = check("a { transform: translate(\n1px,\n\n2px\n); }", 1);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_two_empty_lines_when_max_one() {
    let d = check("a { transform: translate(\n1px,\n\n\n2px\n); }", 1);
    assert_eq!(d.len(), 1);
  }
}
