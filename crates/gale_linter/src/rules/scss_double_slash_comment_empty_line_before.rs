use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow an empty line before `//` comments.
///
/// Primary option: `"always"` or `"never"`.
///
/// Secondary options:
/// - `except: ["first-nested"]` — reverse the primary option for comments that
///   are the first child of a block.
/// - `ignore: ["between-comments", "stylelint-commands"]` — skip checking in
///   these situations.
///
/// Equivalent to `scss/double-slash-comment-empty-line-before`.
pub struct ScssDoubleSlashCommentEmptyLineBefore;

impl Rule for ScssDoubleSlashCommentEmptyLineBefore {
  fn name(&self) -> &'static str {
    "scss/double-slash-comment-empty-line-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow an empty line before //-comments"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = ctx.primary_option_str().unwrap_or("always");
    let secondary = ctx.secondary_options();

    let except_first_nested = secondary
      .and_then(|s| s.get("except"))
      .and_then(|v| v.as_array())
      .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("first-nested")));

    let ignore_between_comments = secondary
      .and_then(|s| s.get("ignore"))
      .and_then(|v| v.as_array())
      .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("between-comments")));

    let ignore_stylelint_commands = secondary
      .and_then(|s| s.get("ignore"))
      .and_then(|v| v.as_array())
      .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("stylelint-commands")));

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

      // Found a `//` comment
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        let comment_start = i;

        // Check if this is inline (non-whitespace before // on same line)
        // Inline comments are not subject to this rule.
        if has_non_whitespace_before_on_line(bytes, comment_start) {
          // Skip to end of line
          while i < len && bytes[i] != b'\n' {
            i += 1;
          }
          continue;
        }

        // Get comment text for checking stylelint commands
        let end = source[i..].find('\n').map(|p| i + p).unwrap_or(len);
        let comment_text = &source[i + 2..end];

        // Skip comments that are inside a selector list (between selector parts
        // separated by commas in a multi-line selector). PostCSS treats these as
        // part of the selector's raw text, not as comment nodes, so Stylelint's
        // walkComments() never visits them. We must match that behavior.
        if is_inside_selector_list(source, comment_start) {
          i = end;
          continue;
        }

        // Check ignore conditions
        if ignore_stylelint_commands && is_stylelint_command(comment_text) {
          i = end;
          continue;
        }

        if ignore_between_comments && is_preceded_by_slash_comment(source, comment_start) {
          i = end;
          continue;
        }

        // Comment at the very start of the file — no preceding content
        if comment_start == 0 || is_first_content_in_source(source, comment_start) {
          i = end;
          continue;
        }

        let has_empty_line = has_empty_line_before(source, comment_start);
        let is_first_nested = is_first_nested_in_block(source, comment_start);

        // Determine the effective expectation
        let mut expect_empty_line = option == "always";
        if except_first_nested && is_first_nested {
          expect_empty_line = !expect_empty_line;
        }

        let comment_len = end - comment_start;

        if expect_empty_line && !has_empty_line {
          diagnostics.push(
            Diagnostic::new(self.name(), "Expected empty line before comment")
              .severity(self.default_severity())
              .span(Span::new(comment_start, comment_len)),
          );
        } else if !expect_empty_line && has_empty_line {
          diagnostics.push(
            Diagnostic::new(self.name(), "Unexpected empty line before comment")
              .severity(self.default_severity())
              .span(Span::new(comment_start, comment_len)),
          );
        }

        i = end;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

/// Returns true if the comment is the first non-whitespace content in the source.
fn is_first_content_in_source(source: &str, offset: usize) -> bool {
  let bytes = source.as_bytes();
  let mut j = offset;
  while j > 0 {
    j -= 1;
    match bytes[j] {
      b' ' | b'\t' | b'\n' | b'\r' => continue,
      _ => return false,
    }
  }
  true
}

fn has_non_whitespace_before_on_line(bytes: &[u8], pos: usize) -> bool {
  let mut j = pos;
  while j > 0 {
    j -= 1;
    match bytes[j] {
      b'\n' | b'\r' => return false,
      b' ' | b'\t' => continue,
      _ => return true,
    }
  }
  false
}

fn has_empty_line_before(source: &str, offset: usize) -> bool {
  let before = &source[..offset];
  let bytes = before.as_bytes();
  let mut pos = before.len();

  // Skip whitespace immediately before the comment
  while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }

  // Skip the newline
  if pos > 0 && bytes[pos - 1] == b'\n' {
    pos -= 1;
    if pos > 0 && bytes[pos - 1] == b'\r' {
      pos -= 1;
    }
  } else {
    return false;
  }

  // Skip whitespace
  while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }

  // Empty line if we hit another newline or start of file
  pos == 0 || bytes[pos - 1] == b'\n'
}

/// Check if this comment is the first non-whitespace content after an opening brace `{`.
fn is_first_nested_in_block(source: &str, offset: usize) -> bool {
  let bytes = source.as_bytes();
  let mut j = offset;
  while j > 0 {
    j -= 1;
    match bytes[j] {
      b' ' | b'\t' | b'\n' | b'\r' => continue,
      b'{' => return true,
      _ => return false,
    }
  }
  false
}

/// Check if the previous content is also a `//` comment.
/// This handles:
/// - Previous line is a standalone `//` comment
/// - Previous line is code with an inline `//` comment at the end
/// - Previous non-blank lines contain a `//` comment (skipping blank lines)
fn is_preceded_by_slash_comment(source: &str, offset: usize) -> bool {
  let bytes = source.as_bytes();
  let mut pos = offset;

  // Skip whitespace before the comment on the current line
  while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }

  // Skip the newline
  if pos > 0 && bytes[pos - 1] == b'\n' {
    pos -= 1;
    if pos > 0 && bytes[pos - 1] == b'\r' {
      pos -= 1;
    }
  } else {
    return false;
  }

  // Skip blank lines to find the previous content line
  loop {
    let line_end = pos;
    while pos > 0 && bytes[pos - 1] != b'\n' {
      pos -= 1;
    }
    let line_start = pos;

    let line = &source[line_start..line_end];
    let trimmed = line.trim();

    if !trimmed.is_empty() {
      // Check if this line starts with `//` (standalone comment)
      // or contains `//` (inline comment at end of code)
      if trimmed.starts_with("//") {
        return true;
      }
      // Check for inline comment: code followed by //
      // Look for `//` that is not inside a string
      if line_contains_slash_comment(trimmed) {
        return true;
      }
      return false;
    }

    // Line is blank — keep looking back
    if pos == 0 {
      return false;
    }
    // Skip the newline before this blank line
    if pos > 0 && bytes[pos - 1] == b'\n' {
      pos -= 1;
      if pos > 0 && bytes[pos - 1] == b'\r' {
        pos -= 1;
      }
    } else {
      return false;
    }
  }
}

/// Check if a line contains a `//` comment (not inside a string).
fn line_contains_slash_comment(line: &str) -> bool {
  let bytes = line.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i < len {
    match bytes[i] {
      b'"' | b'\'' => {
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
      }
      b'/' if i + 1 < len && bytes[i + 1] == b'/' => return true,
      _ => i += 1,
    }
  }
  false
}

/// Returns true if the `//` comment at `offset` appears to be inside a
/// multi-line selector (between selector parts before the opening `{`).
///
/// PostCSS/postcss-scss embeds `//` comments in the raw selector text when
/// they appear between selector parts (before the opening `{`). Stylelint's
/// `walkComments` never visits these embedded comments, so we must skip them.
///
/// Detection:
/// 1. Walk backward, skipping whitespace and other `//` comment lines, to
///    find the last non-comment, non-blank line before this comment.
/// 2. Examine that line's trailing content:
///    - Ends with `,` → definitely inside a selector list → skip
///    - Ends with `{` → inside a block body → real comment
///    - Ends with `}` or `;` → after completed statement → real comment
///    - Other (selector text) → use forward scan to confirm selector context
/// 3. Forward scan: if the first `{`/`}`/`;` after the comment is `{` → skip
fn is_inside_selector_list(source: &str, offset: usize) -> bool {
  let bytes = source.as_bytes();
  let len = bytes.len();

  // Walk backward from start of the comment's line, skipping `//` comment
  // lines and blank lines, to find the first real content line.

  // First, move to the start of the comment line
  let mut pos = offset;
  // Skip whitespace before `//` on this line
  while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }
  // Step back over the newline
  if pos > 0 && bytes[pos - 1] == b'\n' {
    pos -= 1;
    if pos > 0 && bytes[pos - 1] == b'\r' {
      pos -= 1;
    }
  } else {
    // No newline before → first line of file or inline
    return false;
  }

  // Now scan backward over blank lines and `//` comment lines
  loop {
    // Skip trailing whitespace on the current line
    while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
      pos -= 1;
    }

    if pos == 0 {
      break;
    }

    // Skip over newlines (blank lines)
    if bytes[pos - 1] == b'\n' {
      pos -= 1;
      if pos > 0 && bytes[pos - 1] == b'\r' {
        pos -= 1;
      }
      continue;
    }

    // Found the last character of the previous non-blank line.
    // Now determine the full content of that line.
    let line_end = pos;
    let mut line_start = pos;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
      line_start -= 1;
    }
    let line_content = source[line_start..line_end].trim();

    if line_content.is_empty() {
      // Empty line (only whitespace) — keep going backward
      if line_start == 0 {
        break;
      }
      pos = line_start - 1;
      continue;
    }

    // Skip `//` comment lines
    if line_content.starts_with("//") {
      if line_start == 0 {
        break;
      }
      pos = line_start - 1;
      if pos > 0 && bytes[pos - 1] == b'\r' {
        // Already handled — just set pos
      }
      continue;
    }

    // Found a real non-comment non-blank line. Examine its last char.
    let last = *line_content.as_bytes().last().unwrap_or(&0);
    return match last {
      // Inside a block body → real comment, not selector context
      b'{' | b'}' | b';' => false,
      // Selector list separator → definitely in selector context
      b',' => true,
      // Other (selector text like `.foo` or `span`) → forward scan
      _ => forward_scan_for_opening_brace(source, offset, len),
    };
  }

  // Reached start of file without finding decisive content
  forward_scan_for_opening_brace(source, offset, len)
}

/// Scan forward from after the `//` comment at `offset`, skipping other `//`
/// comment lines and whitespace. Return true if the first `{`, `}`, or `;`
/// found is `{` (indicating the comment was in selector-before-block context).
fn forward_scan_for_opening_brace(source: &str, offset: usize, len: usize) -> bool {
  let bytes = source.as_bytes();
  let mut pos = offset;

  // Skip to end of current comment line
  while pos < len && bytes[pos] != b'\n' {
    pos += 1;
  }
  if pos < len {
    pos += 1;
  }

  while pos < len {
    // Skip whitespace
    if matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
      pos += 1;
      continue;
    }

    // Skip block comments
    if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
      pos += 2;
      while pos + 1 < len {
        if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
          pos += 2;
          break;
        }
        pos += 1;
      }
      continue;
    }

    // Skip `//` comment lines
    if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
      while pos < len && bytes[pos] != b'\n' {
        pos += 1;
      }
      continue;
    }

    match bytes[pos] {
      b'{' => return true,
      b'}' | b';' => return false,
      _ => {
        pos += 1;
      }
    }
  }

  false
}

fn is_stylelint_command(comment_text: &str) -> bool {
  let trimmed = comment_text.trim();
  trimmed.starts_with("stylelint-disable")
    || trimmed.starts_with("stylelint-enable")
    || trimmed.starts_with("gale-disable")
    || trimmed.starts_with("gale-enable")
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
  fn always_allows_empty_line_before() {
    let src = ".foo { color: red; }\n\n// comment\n.bar {}";
    let d = ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_empty_line_before() {
    let src = ".foo { color: red; }\n// comment\n.bar {}";
    let d = ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected empty line"));
  }

  #[test]
  fn never_allows_no_empty_line() {
    let opts = serde_json::json!("never");
    let src = ".foo { color: red; }\n// comment\n.bar {}";
    let d =
      ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_empty_line() {
    let opts = serde_json::json!("never");
    let src = ".foo { color: red; }\n\n// comment\n.bar {}";
    let d =
      ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected empty line"));
  }

  #[test]
  fn always_except_first_nested() {
    let opts = serde_json::json!(["always", { "except": ["first-nested"] }]);
    let src = ".foo {\n  // first nested comment\n  color: red;\n}";
    let d =
      ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    // first-nested reverses "always" to "never", so no empty line is fine
    assert!(d.is_empty());
  }

  #[test]
  fn always_except_first_nested_reports_empty_line() {
    let opts = serde_json::json!(["always", { "except": ["first-nested"] }]);
    let src = ".foo {\n\n  // first nested comment\n  color: red;\n}";
    let d =
      ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    // first-nested reverses "always" to "never", so empty line is bad
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected empty line"));
  }

  #[test]
  fn ignore_between_comments() {
    let opts = serde_json::json!(["always", { "ignore": ["between-comments"] }]);
    let src = ".foo { color: red; }\n\n// first comment\n// second comment\n.bar {}";
    let d =
      ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    // first comment has empty line before - ok
    // second comment has no empty line but previous line is a // comment - ignored
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_stylelint_commands() {
    let opts = serde_json::json!(["always", { "ignore": ["stylelint-commands"] }]);
    let src = ".foo { color: red; }\n// stylelint-disable color-no-invalid-hex\n.bar {}";
    let d =
      ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx_with_option(src, &opts));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_inline_comments() {
    let src = ".foo { color: red; // inline\n}";
    let d = ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: ".foo {}\n// comment",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssDoubleSlashCommentEmptyLineBefore
        .check_root(&[], &ctx)
        .is_empty()
    );
  }

  #[test]
  fn allows_first_line_comment() {
    // Comment at the very start of the file — no previous content
    let src = "// first line comment\n.foo {}";
    let d = ScssDoubleSlashCommentEmptyLineBefore.check_root(&[], &scss_ctx(src));
    // At start of file, there's effectively an "empty line" (start of file)
    assert!(d.is_empty());
  }
}
