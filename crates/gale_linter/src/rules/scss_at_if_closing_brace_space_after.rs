use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the closing brace of `@if` statements
/// that are followed by `@else`.
///
/// Default: `"always-intermediate"` — require a single space before `@else`
/// when on the same line.
///
/// Equivalent to `scss/at-if-closing-brace-space-after`.
pub struct ScssAtIfClosingBraceSpaceAfter;

impl Rule for ScssAtIfClosingBraceSpaceAfter {
  fn name(&self) -> &'static str {
    "scss/at-if-closing-brace-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the closing brace of @if before @else"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = ctx.primary_option_str().unwrap_or("always-intermediate");
    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();

    let mut i = 0;
    while i < len {
      if bytes[i] == b'@' && i + 2 < len {
        let rest = &source[i..];
        let is_if = rest.starts_with("@if ") || rest.starts_with("@if(");
        let is_else_if = rest.starts_with("@else if ") || rest.starts_with("@else if(");

        if (is_if || is_else_if)
          && let Some(close_brace) = find_closing_brace(source, i)
        {
          let after = close_brace + 1;
          // Count spaces after `}`
          let mut j = after;
          let mut space_count = 0;
          while j < len && bytes[j] == b' ' {
            space_count += 1;
            j += 1;
          }

          // Check if followed by @else
          let followed_by_else = j < len && source[j..].starts_with("@else");

          if followed_by_else {
            match option {
              "always-intermediate" => {
                if space_count != 1 {
                  diagnostics.push(
                    Diagnostic::new(
                      self.name(),
                      "Expected single space after closing brace of @if before @else".to_string(),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(close_brace, 1)),
                  );
                }
              }
              "never-intermediate" => {
                if space_count > 0 {
                  diagnostics.push(
                    Diagnostic::new(
                      self.name(),
                      "Unexpected space after closing brace of @if before @else".to_string(),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(close_brace, 1)),
                  );
                }
              }
              _ => {}
            }
          }

          i = after;
          continue;
        }
      }
      i += 1;
    }

    diagnostics
  }
}

/// Find the closing `}` that matches the first `{` found after `start`.
/// Handles SCSS interpolation `#{...}`, strings, and comments.
fn find_closing_brace(source: &str, start: usize) -> Option<usize> {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = start;

  // Find opening brace (skip `#{` interpolation openers)
  loop {
    if i >= len {
      return None;
    }
    if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
      i += 2;
      skip_interpolation(bytes, len, &mut i);
      continue;
    }
    if bytes[i] == b'{' {
      break;
    }
    i += 1;
  }

  let mut depth = 1;
  i += 1;
  while i < len && depth > 0 {
    if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
      i += 2;
      skip_interpolation(bytes, len, &mut i);
      continue;
    }
    match bytes[i] {
      b'{' => depth += 1,
      b'}' => {
        depth -= 1;
        if depth == 0 {
          return Some(i);
        }
      }
      b'\'' | b'"' => {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      b'/' if i + 1 < len => {
        if bytes[i + 1] == b'/' {
          i += 2;
          while i < len && bytes[i] != b'\n' {
            i += 1;
          }
        } else if bytes[i + 1] == b'*' {
          i += 2;
          while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
            i += 1;
          }
          if i + 1 < len {
            i += 1;
          }
        }
      }
      _ => {}
    }
    i += 1;
  }

  None
}

fn skip_interpolation(bytes: &[u8], len: usize, i: &mut usize) {
  let mut depth = 1;
  while *i < len && depth > 0 {
    match bytes[*i] {
      b'{' => depth += 1,
      b'}' => depth -= 1,
      _ => {}
    }
    *i += 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn scss_ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    }
  }

  #[test]
  fn allows_single_space_before_else() {
    let ctx = scss_ctx_with_source("@if $cond { color: red; } @else { color: blue; }");
    let d = ScssAtIfClosingBraceSpaceAfter.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_no_space_before_else() {
    let ctx = scss_ctx_with_source("@if $cond { color: red; }@else { color: blue; }");
    let d = ScssAtIfClosingBraceSpaceAfter.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_if_not_followed_by_else() {
    let ctx = scss_ctx_with_source("@if $cond { color: red; }\n.foo {}");
    let d = ScssAtIfClosingBraceSpaceAfter.check_root(&[], &ctx);
    assert!(d.is_empty());
  }
}
