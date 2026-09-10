use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow an empty line before the closing brace of blocks.
///
/// Equivalent to `@stylistic/block-closing-brace-empty-line-before`.
///
/// This rule checks whether there is an empty line (blank line) immediately
/// before the closing `}` of a block.  With the `"never"` option, any empty
/// line is rejected.  With `"always-multi-line"`, multi-line blocks must have
/// an empty line before `}`.
pub struct StylisticBlockClosingBraceEmptyLineBefore;

impl Rule for StylisticBlockClosingBraceEmptyLineBefore {
  fn name(&self) -> &'static str {
    "@stylistic/block-closing-brace-empty-line-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow an empty line before the closing brace of blocks"
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

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip SCSS interpolation #{...}
      if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      if bytes[i] == b'}' {
        let brace_pos = i;

        // Check if the block is empty
        if is_empty_block(source, brace_pos) {
          i += 1;
          continue;
        }

        // Check if the block is single-line
        let is_single_line = is_block_single_line(source, brace_pos);

        // Check for empty line before closing brace.
        // An "empty line" means there is a blank line (two consecutive
        // newlines with only whitespace between them) in the content
        // between the last statement and the closing brace.
        let has_empty_line = has_empty_line_before_brace(source, brace_pos);

        let violation = match option {
          "never" => has_empty_line,
          "always-multi-line" => !is_single_line && !has_empty_line,
          _ => false,
        };

        if violation {
          let msg = match option {
            "never" => "Unexpected empty line before closing brace",
            "always-multi-line" => "Expected empty line before closing brace of a multi-line block",
            _ => {
              i += 1;
              continue;
            }
          };
          diagnostics.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(brace_pos, 1)),
          );
        }
      }

      i += 1;
    }

    diagnostics
  }
}

/// Check if there is an empty line (blank line) before the closing brace.
///
/// Walks backwards from the closing brace position looking for two
/// consecutive newlines (possibly separated by whitespace-only content).
fn has_empty_line_before_brace(source: &str, brace_pos: usize) -> bool {
  let bytes = source.as_bytes();
  if brace_pos == 0 {
    return false;
  }

  // Walk backwards from brace, find the content between the last
  // statement and the closing brace.
  let mut pos = brace_pos - 1;

  // Skip whitespace/newlines immediately before brace to find
  // the region we're examining.
  // We need to check if there's a blank line in the whitespace before `}`.
  // A blank line means: \n followed by optional spaces/tabs, followed by \n.
  let mut newline_count = 0;
  let mut saw_non_ws_on_line = false;

  while pos < brace_pos {
    // pos could have wrapped on underflow, but we start from brace_pos-1
    let ch = bytes[pos];
    if ch == b'\n' {
      if newline_count > 0 && !saw_non_ws_on_line {
        // Found a blank line (two newlines with only whitespace between)
        return true;
      }
      newline_count += 1;
      saw_non_ws_on_line = false;
    } else if ch == b'\r' {
      // skip
    } else if ch == b' ' || ch == b'\t' {
      // whitespace, don't set saw_non_ws
    } else {
      // Non-whitespace character — stop looking
      break;
    }
    if pos == 0 {
      break;
    }
    pos -= 1;
  }

  false
}

fn is_empty_block(source: &str, closing_brace_pos: usize) -> bool {
  let bytes = source.as_bytes();
  // Find the matching opening brace
  let mut depth = 1;
  let mut j = closing_brace_pos;
  while j > 0 {
    j -= 1;
    if bytes[j] == b'}' {
      depth += 1;
    } else if bytes[j] == b'{' {
      depth -= 1;
      if depth == 0 {
        // Check if everything between { and } is whitespace
        let between = &source[j + 1..closing_brace_pos];
        return between.trim().is_empty();
      }
    }
  }
  true
}

fn is_block_single_line(source: &str, closing_brace_pos: usize) -> bool {
  let bytes = source.as_bytes();
  let mut depth = 1;
  let mut j = closing_brace_pos;
  while j > 0 {
    j -= 1;
    if bytes[j] == b'}' {
      depth += 1;
    } else if bytes[j] == b'{' {
      depth -= 1;
      if depth == 0 {
        let segment = &source[j..closing_brace_pos];
        return !segment.contains('\n');
      }
    }
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticBlockClosingBraceEmptyLineBefore;
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
  fn never_accepts_no_empty_line() {
    let d = check("a {\n  color: red;\n}", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_empty_line_before_brace() {
    let d = check("a {\n  color: red;\n\n}", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected empty line"));
  }

  #[test]
  fn never_rejects_empty_line_with_spaces() {
    let d = check("a {\n  color: red;\n  \n}", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected empty line"));
  }

  #[test]
  fn never_accepts_single_line() {
    let d = check("a { color: red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_skips_empty_block() {
    let d = check("a {\n\n}", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_accepts_empty_line() {
    let d = check("a {\n  color: red;\n\n}", "always-multi-line");
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_rejects_no_empty_line() {
    let d = check("a {\n  color: red;\n}", "always-multi-line");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected empty line"));
  }

  #[test]
  fn always_multi_line_accepts_single_line() {
    let d = check("a { color: red; }", "always-multi-line");
    assert!(d.is_empty());
  }
}
