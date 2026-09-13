use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a newline inside the parentheses of functions.
///
/// Equivalent to `@stylistic/function-parentheses-newline-inside`.
pub struct StylisticFunctionParenthesesNewlineInside;

impl Rule for StylisticFunctionParenthesesNewlineInside {
  fn name(&self) -> &'static str {
    "@stylistic/function-parentheses-newline-inside"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a newline inside the parentheses of functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("always-multi-line");
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
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        continue;
      }

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Detect function call: identifier followed by `(`
      if bytes[i] == b'(' && i > 0 && is_function_call(bytes, i) {
        let open_paren = i;

        // Find matching closing paren
        let close_paren = match find_matching_paren(bytes, open_paren) {
          Some(pos) => pos,
          None => {
            i += 1;
            continue;
          }
        };

        // Check if function args span multiple lines
        let is_multi_line = bytes[open_paren..=close_paren].contains(&b'\n');

        if !is_multi_line {
          i += 1;
          continue;
        }

        // Multi-line function call — check based on option
        match option {
          "always-multi-line" => {
            // After `(` should be a newline
            let after_open = open_paren + 1;
            let mut j = after_open;
            while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
              j += 1;
            }
            if j < len && bytes[j] != b'\n' {
              diagnostics.push(
                Diagnostic::new(self.name(), "Expected newline after \"(\"")
                  .severity(self.default_severity())
                  .span(Span::new(open_paren, 1)),
              );
            }

            // Before `)` should be a newline
            if close_paren > 0 {
              let mut k = close_paren - 1;
              while k > open_paren && (bytes[k] == b' ' || bytes[k] == b'\t') {
                k -= 1;
              }
              if bytes[k] != b'\n' {
                diagnostics.push(
                  Diagnostic::new(self.name(), "Expected newline before \")\"")
                    .severity(self.default_severity())
                    .span(Span::new(close_paren, 1)),
                );
              }
            }
          }
          "never-multi-line" => {
            // After `(` should NOT be a newline
            let after_open = open_paren + 1;
            let mut j = after_open;
            while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
              j += 1;
            }
            if j < len && bytes[j] == b'\n' {
              diagnostics.push(
                Diagnostic::new(self.name(), "Unexpected newline after \"(\"")
                  .severity(self.default_severity())
                  .span(Span::new(open_paren, 1)),
              );
            }
          }
          _ => {}
        }

        // Skip past the closing paren to avoid re-processing
        i = close_paren + 1;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

/// Check if the `(` at position `pos` is preceded by a function name identifier.
fn is_function_call(bytes: &[u8], pos: usize) -> bool {
  if pos == 0 {
    return false;
  }
  let prev = bytes[pos - 1];
  // Function names end with an alphanumeric char, `-`, or `_`
  prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_'
}

/// Find the matching `)` for the `(` at `open`.
fn find_matching_paren(bytes: &[u8], open: usize) -> Option<usize> {
  let mut depth = 1;
  let mut j = open + 1;
  while j < bytes.len() && depth > 0 {
    match bytes[j] {
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 {
          return Some(j);
        }
      }
      b'"' | b'\'' => {
        let quote = bytes[j];
        j += 1;
        while j < bytes.len() && bytes[j] != quote {
          if bytes[j] == b'\\' {
            j += 1;
          }
          j += 1;
        }
      }
      _ => {}
    }
    j += 1;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn always_multi_line_allows_newlines_inside_parens() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { background: url(\n  \"foo.png\"\n); }";
    let d =
      StylisticFunctionParenthesesNewlineInside.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn always_multi_line_reports_missing_newline_after_open() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { background: url(\"foo.png\"\n); }";
    let d =
      StylisticFunctionParenthesesNewlineInside.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected newline after \"(\""));
  }

  #[test]
  fn always_multi_line_reports_missing_newline_before_close() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { background: url(\n  \"foo.png\"); }";
    let d =
      StylisticFunctionParenthesesNewlineInside.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(
      d.iter()
        .any(|d| d.message.contains("Expected newline before \")\""),),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn always_multi_line_ignores_single_line() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { background: url(\"foo.png\"); }";
    let d =
      StylisticFunctionParenthesesNewlineInside.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn never_multi_line_reports_newline_after_open() {
    let opt = serde_json::json!("never-multi-line");
    let source = "a { transform: translate(\n  1px,\n  2px); }";
    let d =
      StylisticFunctionParenthesesNewlineInside.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected newline after \"(\""));
  }

  #[test]
  fn never_multi_line_allows_no_newline_after_open() {
    let opt = serde_json::json!("never-multi-line");
    let source = "a { transform: translate(1px,\n  2px); }";
    let d =
      StylisticFunctionParenthesesNewlineInside.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }
}
