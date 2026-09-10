use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports `//` comments in CSS files (valid in SCSS but not CSS).
///
/// Equivalent to Stylelint's `no-invalid-double-slash-comments` rule.
pub struct NoInvalidDoubleSlashComments;

impl Rule for NoInvalidDoubleSlashComments {
  fn name(&self) -> &'static str {
    "no-invalid-double-slash-comments"
  }

  fn description(&self) -> &'static str {
    "Disallow double-slash comments (//...) which are not supported in CSS"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    // Only flag in plain CSS mode.
    if context.syntax != Syntax::Css {
      return vec![];
    }

    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;

    while i < len {
      let b = bytes[i];

      // Skip string literals.
      if b == b'"' || b == b'\'' {
        let quote = b;
        i += 1;
        while i < len {
          if bytes[i] == b'\\' {
            i += 2; // skip escaped character
            continue;
          }
          if bytes[i] == quote {
            i += 1;
            break;
          }
          i += 1;
        }
        continue;
      }

      // Check for slash.
      if b == b'/' && i + 1 < len {
        if bytes[i + 1] == b'*' {
          // Block comment — skip to closing `*/`.
          i += 2;
          while i + 1 < len {
            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
              i += 2;
              break;
            }
            i += 1;
          }
          continue;
        }

        if bytes[i + 1] == b'/' {
          // Double-slash comment found.
          let start = i;
          // Find end of line.
          let end = source[i..].find('\n').map(|pos| i + pos).unwrap_or(len);
          diagnostics.push(
            Diagnostic::new(self.name(), "Unexpected double-slash CSS comment")
              .severity(self.default_severity())
              .span(Span::new(start, end - start)),
          );
          i = end;
          continue;
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

  #[test]
  fn reports_double_slash_comment_in_css() {
    let rule = NoInvalidDoubleSlashComments;
    let context = RuleContext {
      file_path: "test.css",
      source: "a { color: red; } // bad comment",
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected double-slash CSS comment");
  }

  #[test]
  fn ignores_double_slash_in_scss() {
    let rule = NoInvalidDoubleSlashComments;
    let context = RuleContext {
      file_path: "test.scss",
      source: "// this is fine in SCSS",
      syntax: Syntax::Scss,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_block_comments() {
    let rule = NoInvalidDoubleSlashComments;
    let context = RuleContext {
      file_path: "test.css",
      source: "/* this is fine */ a { color: red; }",
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_double_slash_inside_string() {
    let rule = NoInvalidDoubleSlashComments;
    let context = RuleContext {
      file_path: "test.css",
      source: "a { content: \"//not-a-comment\"; }",
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_double_slash_inside_block_comment() {
    let rule = NoInvalidDoubleSlashComments;
    let context = RuleContext {
      file_path: "test.css",
      source: "/* // inside block */ a { color: red; }",
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }
}
