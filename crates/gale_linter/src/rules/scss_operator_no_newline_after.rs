use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow newlines after SCSS arithmetic/string operators (+, -, *, /, %).
///
/// Equivalent to Stylelint's `scss/operator-no-newline-after`.
/// Flags lines where the last meaningful character before EOL is an operator
/// that appears to be binary (not unary).
pub struct ScssOperatorNoNewlineAfter;

// `%` is excluded because it's indistinguishable from CSS percentage values (e.g., `63%`).
const OPERATORS: &[char] = &['+', '-', '*', '/'];

impl Rule for ScssOperatorNoNewlineAfter {
  fn name(&self) -> &'static str {
    "scss/operator-no-newline-after"
  }

  fn description(&self) -> &'static str {
    "Disallow newlines after Sass operators"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let source = ctx.source;
    let mut diags = Vec::new();
    let mut byte_offset: usize = 0;

    for raw_line in source.split('\n') {
      // Strip \r for Windows line endings.
      let line = raw_line.trim_end_matches('\r');

      // Strip trailing // comment.
      let content = strip_line_comment(line);

      // Strip trailing whitespace from the effective content.
      let content_trimmed = content.trim_end();

      if !content_trimmed.is_empty() {
        let last_char = content_trimmed.chars().last().unwrap();
        if OPERATORS.contains(&last_char) {
          // Check if it looks binary: preceded by a closing character
          // or identifier/digit character (not another operator or space).
          let before_op = &content_trimmed[..content_trimmed.len() - last_char.len_utf8()];
          let before_trimmed = before_op.trim_end();
          if let Some(prev) = before_trimmed.chars().last() {
            if is_value_end_char(prev) && !is_css_color_channel_separator(last_char, before_trimmed)
            {
              let op_byte_in_line = content_trimmed.len() - 1;
              diags.push(
                Diagnostic::new(
                  self.name(),
                  format!("Unexpected newline after \"{}\"", last_char),
                )
                .severity(self.default_severity())
                .span(Span::new(byte_offset + op_byte_in_line, 1)),
              );
            }
          }
        }
      }

      // Advance past this line + '\n'.
      byte_offset += raw_line.len() + 1;
    }

    diags
  }
}

/// Strip a `//` line comment from the end of a line.
fn strip_line_comment(line: &str) -> &str {
  // Find `//` that isn't inside a string (simplified: find the first `//`
  // not preceded by a URL-like `:`).
  let bytes = line.as_bytes();
  let mut i = 0;
  let mut in_single = false;
  let mut in_double = false;
  while i < bytes.len().saturating_sub(1) {
    match bytes[i] {
      b'\'' if !in_double => in_single = !in_single,
      b'"' if !in_single => in_double = !in_double,
      b'/' if !in_single && !in_double && bytes[i + 1] == b'/' => {
        return &line[..i];
      }
      _ => {}
    }
    i += 1;
  }
  line
}

/// Returns true if `c` is a character that typically ends a value expression
/// (meaning the following operator is binary, not unary).
fn is_value_end_char(c: char) -> bool {
  c.is_alphanumeric() || matches!(c, ')' | ']' | '"' | '\'' | '_' | '%')
}

/// Returns true if the `/` operator at end of a line is a CSS Color Level 4
/// channel separator (e.g. `rgb(from ... r g b /`), not a SCSS division operator.
///
/// The pattern: operator is `/`, no SCSS variable (`$`) in the preceding content,
/// and the last "word" before the operator is a single lowercase letter (a CSS
/// color channel identifier like `r`, `g`, `b`, `a`, `h`, `s`, `l`).
fn is_css_color_channel_separator(op: char, before_trimmed: &str) -> bool {
  if op != '/' {
    return false;
  }
  // If the line contains a SCSS variable, it's arithmetic context.
  if before_trimmed.contains('$') {
    return false;
  }
  // Check if the last word is a single lowercase letter (CSS color channel).
  let last_word = before_trimmed
    .split_ascii_whitespace()
    .next_back()
    .unwrap_or("");
  last_word.len() == 1
    && last_word
      .chars()
      .next()
      .map_or(false, |c| c.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn scss_ctx(source: &'static str) -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    }
  }

  #[test]
  fn reports_plus_before_newline() {
    let source = "$a: 1 +\n  2;";
    let d = ScssOperatorNoNewlineAfter.check_root(&[], &scss_ctx(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains('+'));
  }

  #[test]
  fn allows_operator_in_middle_of_line() {
    let source = "$a: 1 + 2;";
    let d = ScssOperatorNoNewlineAfter.check_root(&[], &scss_ctx(source));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "$a: 1 +\n  2;",
      syntax: Syntax::Css,
      options: None,
    };
    let d = ScssOperatorNoNewlineAfter.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_unary_minus_at_line_end() {
    // A line ending with ` -` after a colon is likely unary (start of value)
    let source = "color:\n  -1rem;";
    let d = ScssOperatorNoNewlineAfter.check_root(&[], &scss_ctx(source));
    // No operator at end of line — "color:" ends with ":", not an operator
    assert!(d.is_empty());
  }
}
