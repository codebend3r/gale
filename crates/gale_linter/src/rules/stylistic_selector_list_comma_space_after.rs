use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the commas of selector lists.
///
/// Equivalent to `@stylistic/selector-list-comma-space-after`.
pub struct StylisticSelectorListCommaSpaceAfter;

impl Rule for StylisticSelectorListCommaSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/selector-list-comma-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the commas of selector lists"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("always-single-line");
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut brace_depth = 0;
    // Track the start of the current selector list for multi-line detection
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
        let after = i + 1;

        let has_space_after = after < len && (bytes[after] == b' ' || bytes[after] == b'\t');

        // For always-single-line, find the full selector list range
        let is_single_line = if option == "always-single-line" {
          // Find end of selector list (the next '{')
          let mut end = comma_pos;
          while end < len && bytes[end] != b'{' {
            end += 1;
          }
          !source[selector_start..end.min(len)].contains('\n')
        } else {
          true
        };

        let violation = match option {
          "always" => !has_space_after,
          "never" => has_space_after,
          "always-single-line" => is_single_line && !has_space_after,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-single-line" => "Expected a space after \",\"",
            "never" => "Unexpected space after \",\"",
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
    let rule = StylisticSelectorListCommaSpaceAfter;
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
  fn always_accepts_space_after_comma() {
    let d = check("a, b { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_after_comma() {
    let d = check("a,b { color: red; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_accepts_no_space() {
    let d = check("a,b { color: red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space() {
    let d = check("a, b { color: red; }", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_single_line_accepts_space_on_single_line() {
    let d = check("a, b { color: red; }", "always-single-line");
    assert!(d.is_empty());
  }

  #[test]
  fn always_single_line_allows_no_space_on_multi_line() {
    let d = check("a,\nb { color: red; }", "always-single-line");
    assert!(d.is_empty());
  }
}
