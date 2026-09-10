use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline or disallow whitespace before the commas of selector lists.
///
/// Equivalent to `@stylistic/selector-list-comma-newline-before`.
pub struct StylisticSelectorListCommaNewlineBefore;

impl Rule for StylisticSelectorListCommaNewlineBefore {
  fn name(&self) -> &'static str {
    "@stylistic/selector-list-comma-newline-before"
  }

  fn description(&self) -> &'static str {
    "Require a newline or disallow whitespace before the commas of selector lists"
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
    let mut brace_depth = 0;
    let mut selector_start = 0;

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

      if bytes[i] == b'{' {
        brace_depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b'}' {
        if brace_depth > 0 {
          brace_depth -= 1;
        }
        selector_start = i + 1;
        i += 1;
        continue;
      }

      // Only check commas in selectors (outside braces)
      if bytes[i] == b',' && brace_depth == 0 {
        let comma_pos = i;

        // Check what's immediately before the comma
        let has_newline_before = if comma_pos > 0 {
          let mut j = comma_pos - 1;
          // skip spaces/tabs backwards
          while j > 0 && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j -= 1;
          }
          bytes[j] == b'\n' || bytes[j] == b'\r'
        } else {
          false
        };

        // For always-multi-line, check if the selector list is multi-line
        let is_multi_line = if option == "always-multi-line" {
          let mut end = comma_pos;
          while end < len && bytes[end] != b'{' {
            end += 1;
          }
          source[selector_start..end.min(len)].contains('\n')
        } else {
          true
        };

        let violation = match option {
          "always" => !has_newline_before,
          "never" => has_newline_before,
          "always-multi-line" => is_multi_line && !has_newline_before,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-multi-line" => "Expected newline before \",\"",
            "never" => "Unexpected whitespace before \",\"",
            _ => continue,
          };
          diagnostics.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(comma_pos, 1)),
          );
        }
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
    let rule = StylisticSelectorListCommaNewlineBefore;
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
  fn always_accepts_newline_before_comma() {
    let d = check("a\n, b { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_newline_before_comma() {
    let d = check("a, b { color: red; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn never_accepts_no_newline_before_comma() {
    let d = check("a, b { color: red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_newline_before_comma() {
    let d = check("a\n, b { color: red; }", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn always_multi_line_allows_single_line() {
    let d = check("a, b { color: red; }", "always-multi-line");
    assert!(d.is_empty());
  }
}
