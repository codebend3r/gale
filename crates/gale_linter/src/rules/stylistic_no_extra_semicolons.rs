use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow extra semicolons.
///
/// Equivalent to `@stylistic/no-extra-semicolons`.
pub struct StylisticNoExtraSemicolons;

impl Rule for StylisticNoExtraSemicolons {
  fn name(&self) -> &'static str {
    "@stylistic/no-extra-semicolons"
  }

  fn description(&self) -> &'static str {
    "Disallow extra semicolons"
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

      if bytes[i] == b';' {
        // Check for ;; (double semicolons)
        if i + 1 < len && bytes[i + 1] == b';' {
          diagnostics.push(
            Diagnostic::new(self.name(), "Unexpected extra semicolon")
              .severity(self.default_severity())
              .span(Span::new(i + 1, 1)),
          );
          i += 1;
          continue;
        }

        // Check for ; followed by only whitespace then }
        let mut j = i + 1;
        while j < len
          && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r')
        {
          j += 1;
        }
        if j < len && bytes[j] == b'}' {
          // This is fine -- a trailing semicolon before } is normal.
          // But check if there's ONLY a semicolon (no property) -- i.e.
          // the semicolon itself is extra if there's another semicolon
          // right before it (already caught above).
        }
      }

      i += 1;
    }

    // Second pass: find standalone ; before } (extra semicolons at top level or
    // semicolons that are the only content before })
    i = 0;
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

      // Look for ; at the top level (outside blocks) which is extra
      if bytes[i] == b';' {
        // Check backwards to see if there's another ; before this one
        // with only whitespace in between
        let mut k = i.wrapping_sub(1);
        if i > 0 {
          k = i - 1;
          while k > 0
            && (bytes[k] == b' ' || bytes[k] == b'\t' || bytes[k] == b'\n' || bytes[k] == b'\r')
          {
            k -= 1;
          }
          if bytes[k] == b';' {
            // Already reported as ;; or separated by whitespace
            let already_reported = diagnostics.iter().any(|d| d.span.offset == i);
            if !already_reported {
              diagnostics.push(
                Diagnostic::new(self.name(), "Unexpected extra semicolon")
                  .severity(self.default_severity())
                  .span(Span::new(i, 1)),
              );
            }
          }
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

  fn ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn allows_normal_semicolons() {
    let source = "a { color: red; display: block; }";
    let d = StylisticNoExtraSemicolons.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_double_semicolons() {
    let source = "a { color: red;; }";
    let d = StylisticNoExtraSemicolons.check_root(&[], &ctx(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("extra semicolon"));
  }

  #[test]
  fn reports_semicolons_separated_by_whitespace() {
    let source = "a { color: red; ; }";
    let d = StylisticNoExtraSemicolons.check_root(&[], &ctx(source));
    assert!(!d.is_empty());
  }
}
