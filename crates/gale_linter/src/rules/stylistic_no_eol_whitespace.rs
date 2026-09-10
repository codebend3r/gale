use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow end-of-line whitespace.
///
/// Equivalent to `@stylistic/no-eol-whitespace`.
pub struct StylisticNoEolWhitespace;

impl Rule for StylisticNoEolWhitespace {
  fn name(&self) -> &'static str {
    "@stylistic/no-eol-whitespace"
  }

  fn description(&self) -> &'static str {
    "Disallow end-of-line whitespace"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
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

      // Skip comments
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

      if bytes[i] == b'\n' {
        // Check for trailing whitespace before this newline
        let trail_end = i;
        let mut trail_start = i;
        while trail_start > 0 && (bytes[trail_start - 1] == b' ' || bytes[trail_start - 1] == b'\t')
        {
          trail_start -= 1;
        }
        if trail_start < trail_end {
          diagnostics.push(
            Diagnostic::new(self.name(), "Unexpected end-of-line whitespace")
              .severity(self.default_severity())
              .span(Span::new(trail_start, trail_end - trail_start)),
          );
        }
      }

      i += 1;
    }

    // Also check the last line if it doesn't end with \n
    if len > 0 && bytes[len - 1] != b'\n' {
      let mut trail_start = len;
      while trail_start > 0 && (bytes[trail_start - 1] == b' ' || bytes[trail_start - 1] == b'\t') {
        trail_start -= 1;
      }
      if trail_start < len {
        diagnostics.push(
          Diagnostic::new(self.name(), "Unexpected end-of-line whitespace")
            .severity(self.default_severity())
            .span(Span::new(trail_start, len - trail_start)),
        );
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn allows_no_trailing_whitespace() {
    let source = "a {\n  color: red;\n}\n";
    let d = StylisticNoEolWhitespace.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_trailing_spaces() {
    let source = "a {  \n  color: red;\n}\n";
    let d = StylisticNoEolWhitespace.check_root(&[], &ctx(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("end-of-line whitespace"));
  }

  #[test]
  fn reports_trailing_tab() {
    let source = "a {\t\n  color: red;\n}\n";
    let d = StylisticNoEolWhitespace.check_root(&[], &ctx(source));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_multiple_lines() {
    let source = "a { \n  color: red; \n}\n";
    let d = StylisticNoEolWhitespace.check_root(&[], &ctx(source));
    assert_eq!(d.len(), 2);
  }
}
