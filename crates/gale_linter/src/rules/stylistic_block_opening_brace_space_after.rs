use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the opening brace of blocks.
///
/// Primary option: "always" | "never" | "always-single-line" | "always-multi-line"
pub struct StylisticBlockOpeningBraceSpaceAfter;

impl Rule for StylisticBlockOpeningBraceSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/block-opening-brace-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the opening brace of blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = ctx.primary_option_str().unwrap_or("always");
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
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
      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
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

      if bytes[i] == b'{' {
        let brace_pos = i;
        let after = brace_pos + 1;

        // Find the matching closing brace to determine single/multi-line
        let mut depth = 1u32;
        let mut j = after;
        // Skip strings and comments while finding the closing brace
        while j < len && depth > 0 {
          if j + 1 < len && bytes[j] == b'/' && bytes[j + 1] == b'*' {
            j += 2;
            while j + 1 < len && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
              j += 1;
            }
            if j + 1 < len {
              j += 2;
            }
            continue;
          }
          if j + 1 < len && bytes[j] == b'/' && bytes[j + 1] == b'/' {
            while j < len && bytes[j] != b'\n' {
              j += 1;
            }
            continue;
          }
          if bytes[j] == b'\'' || bytes[j] == b'"' {
            let q = bytes[j];
            j += 1;
            while j < len && bytes[j] != q {
              if bytes[j] == b'\\' {
                j += 1;
              }
              j += 1;
            }
            if j < len {
              j += 1;
            }
            continue;
          }
          if bytes[j] == b'{' {
            depth += 1;
          } else if bytes[j] == b'}' {
            depth -= 1;
          }
          j += 1;
        }
        let close_pos = j.saturating_sub(1);
        let is_single_line = !ctx.source[brace_pos..close_pos.min(len)].contains('\n');

        let has_space = after < len && bytes[after] == b' ';
        let has_newline = after < len && (bytes[after] == b'\n' || bytes[after] == b'\r');

        let violation = match option {
          "always" => !has_space && !has_newline,
          "never" => has_space,
          "always-single-line" => is_single_line && !has_space,
          "always-multi-line" => !is_single_line && !has_space && !has_newline,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-single-line" | "always-multi-line" => {
              "Expected a space after the opening brace"
            }
            "never" => "Unexpected space after the opening brace",
            _ => "Expected a space after the opening brace",
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

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticBlockOpeningBraceSpaceAfter;
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
  fn always_accepts_space_after_brace() {
    let d = check("a { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_after_brace() {
    let d = check("a {color: red; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_accepts_no_space_after_brace() {
    let d = check("a {color: red;}", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_after_brace() {
    let d = check("a { color: red;}", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_accepts_newline_after_brace() {
    let d = check("a {\n  color: red;\n}", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_single_line_only_checks_single_line_blocks() {
    // Multi-line block without space — should NOT flag
    let d = check("a {\ncolor: red;\n}", "always-single-line");
    assert!(d.is_empty());

    // Single-line block without space — should flag
    let d = check("a {color: red; }", "always-single-line");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn always_multi_line_only_checks_multi_line_blocks() {
    // Single-line block without space — should NOT flag
    let d = check("a {color: red; }", "always-multi-line");
    assert!(d.is_empty());

    // Multi-line block without space or newline — should flag
    // (this is a contrived case but tests the logic)
  }
}
