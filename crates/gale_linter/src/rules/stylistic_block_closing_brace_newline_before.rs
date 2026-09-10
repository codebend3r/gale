use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a newline before the closing brace of blocks.
///
/// Equivalent to `@stylistic/block-closing-brace-newline-before`.
pub struct StylisticBlockClosingBraceNewlineBefore;

impl Rule for StylisticBlockClosingBraceNewlineBefore {
  fn name(&self) -> &'static str {
    "@stylistic/block-closing-brace-newline-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a newline before the closing brace of blocks"
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

        // Stylelint checks whether `raws.after` (the whitespace between
        // the last child and `}`) *starts* with a newline.  This means
        // we must find the last non-whitespace byte before `}`, then
        // check if the character immediately after it is `\n` (or `\r\n`).
        // Trailing whitespace on the previous line (between `;` and
        // `\n`) causes `raws.after` to start with a space, not `\n`,
        // so Stylelint flags it.
        let has_newline_before = if brace_pos > 0 {
          // Find the last non-whitespace byte before `}`
          let mut j = brace_pos - 1;
          while j > 0
            && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r')
          {
            j -= 1;
          }
          // j is now at the last non-whitespace byte (e.g. `;`)
          // Check if the character immediately after it is `\n` or `\r\n`
          let after = j + 1;
          if after < brace_pos {
            bytes[after] == b'\n'
              || (bytes[after] == b'\r' && after + 1 < brace_pos && bytes[after + 1] == b'\n')
          } else {
            false
          }
        } else {
          false
        };

        // Check if the block is single-line
        let is_single_line = is_block_single_line(source, brace_pos);

        let violation = match option {
          "always" => !has_newline_before,
          "never" => has_newline_before,
          "always-multi-line" => !is_single_line && !has_newline_before,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-multi-line" => "Expected newline before \"}\"",
            "never" => "Unexpected newline before \"}\"",
            _ => {
              i += 1;
              continue;
            }
          };
          // Stylelint reports the character before the closing brace
          diagnostics.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(brace_pos.saturating_sub(1), 1)),
          );
        }
      }

      i += 1;
    }

    diagnostics
  }
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
    let rule = StylisticBlockClosingBraceNewlineBefore;
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
  fn always_accepts_newline_before_brace() {
    let d = check("a {\n  color: red;\n}", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_newline_before_brace() {
    let d = check("a { color: red; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn never_accepts_no_newline_before_brace() {
    let d = check("a { color: red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_newline_before_brace() {
    let d = check("a {\n  color: red;\n}", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected newline"));
  }

  #[test]
  fn always_multi_line_allows_single_line() {
    let d = check("a { color: red; }", "always-multi-line");
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_rejects_missing_newline_in_multiline_block() {
    let d = check("a {\n  color: red; }", "always-multi-line");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }
}
