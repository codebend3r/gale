use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow or require an empty line before `@else`.
///
/// Primary option: `"never"` (default) or `"always"`.
///
/// Equivalent to `scss/at-else-empty-line-before`.
pub struct ScssAtElseEmptyLineBefore;

impl Rule for ScssAtElseEmptyLineBefore {
  fn name(&self) -> &'static str {
    "scss/at-else-empty-line-before"
  }

  fn description(&self) -> &'static str {
    "Disallow or require an empty line before @else"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = ctx.primary_option_str().unwrap_or("never");
    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();

    let mut i = 0;
    while i < len {
      // Skip string literals
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < len {
          if bytes[i] == b'\\' {
            i += 2;
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

      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
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

      // Skip line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Look for `@else`
      if bytes[i] == b'@' && source[i..].starts_with("@else") {
        let at_else_offset = i;

        // Check if there's an empty line before this @else
        let has_empty_line = has_empty_line_before(source, at_else_offset);

        match option {
          "never" => {
            if has_empty_line {
              diagnostics.push(
                Diagnostic::new(self.name(), "Unexpected empty line before @else")
                  .severity(self.default_severity())
                  .span(Span::new(at_else_offset, 5)),
              );
            }
          }
          "always" => {
            if !has_empty_line {
              diagnostics.push(
                Diagnostic::new(self.name(), "Expected empty line before @else")
                  .severity(self.default_severity())
                  .span(Span::new(at_else_offset, 5)),
              );
            }
          }
          _ => {}
        }

        i += 5;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

/// Check if there is an empty line (a line containing only whitespace)
/// between the previous content and the position at `offset`.
fn has_empty_line_before(source: &str, offset: usize) -> bool {
  let before = &source[..offset];

  // Walk backwards through newlines. We need to find at least one blank line.
  // A blank line means two consecutive newlines with only whitespace between them.
  let mut pos = before.len();

  // Skip whitespace (spaces/tabs) immediately before @else
  while pos > 0 && matches!(before.as_bytes()[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }

  // Skip the first newline (the line break right before @else's line)
  if pos > 0 && before.as_bytes()[pos - 1] == b'\n' {
    pos -= 1;
    if pos > 0 && before.as_bytes()[pos - 1] == b'\r' {
      pos -= 1;
    }
  } else {
    // @else is on the same line as previous content or at start of file
    return false;
  }

  // Now skip whitespace again
  while pos > 0 && matches!(before.as_bytes()[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }

  // If we hit another newline (or start of file), that means there was an empty line
  if pos == 0 {
    return true;
  }
  before.as_bytes()[pos - 1] == b'\n'
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn scss_ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn scss_ctx_with_option<'a>(source: &'a str, opts: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "t.scss",
      source,
      syntax: Syntax::Scss,
      options: Some(opts),
    }
  }

  #[test]
  fn never_allows_no_empty_line() {
    let src = "@if $a { color: red; }\n@else { color: blue; }";
    let d = ScssAtElseEmptyLineBefore.check_root(&[], &scss_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_empty_line() {
    let src = "@if $a { color: red; }\n\n@else { color: blue; }";
    let d = ScssAtElseEmptyLineBefore.check_root(&[], &scss_ctx(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected empty line"));
  }

  #[test]
  fn always_allows_empty_line() {
    let opts = serde_json::json!("always");
    let src = "@if $a { color: red; }\n\n@else { color: blue; }";
    let d = ScssAtElseEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_empty_line() {
    let opts = serde_json::json!("always");
    let src = "@if $a { color: red; }\n@else { color: blue; }";
    let d = ScssAtElseEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected empty line"));
  }

  #[test]
  fn never_same_line_no_report() {
    let src = "@if $a { color: red; } @else { color: blue; }";
    let d = ScssAtElseEmptyLineBefore.check_root(&[], &scss_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "@if $a {}\n\n@else {}",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(ScssAtElseEmptyLineBefore.check_root(&[], &ctx).is_empty());
  }
}
