use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the bang (`!`) of declarations.
///
/// Primary: "always" | "never"
pub struct StylisticDeclarationBangSpaceAfter;

impl Rule for StylisticDeclarationBangSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-bang-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the bang of declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = ctx.primary_option_str().unwrap_or("never");
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
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

      // Look for `!` followed by an alphabetic character (like `!important`)
      if bytes[i] == b'!'
        && i + 1 < len
        && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b' ')
      {
        let bang_pos = i;
        let after = bang_pos + 1;
        let has_space_after = after < len && bytes[after] == b' ';

        // Only flag if followed by a keyword (after possible space)
        let keyword_start = if has_space_after { after + 1 } else { after };
        if keyword_start >= len || !bytes[keyword_start].is_ascii_alphabetic() {
          i += 1;
          continue;
        }

        let violation = match option {
          "always" => !has_space_after,
          "never" => has_space_after,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" => "Expected a space after \"!\"",
            "never" => "Unexpected space after \"!\"",
            _ => "Expected a space after \"!\"",
          };
          diagnostics.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(bang_pos, 1)),
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
    let rule = StylisticDeclarationBangSpaceAfter;
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
  fn never_accepts_no_space_after_bang() {
    let d = check("a { color: red !important; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_after_bang() {
    let d = check("a { color: red ! important; }", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_accepts_space_after_bang() {
    let d = check("a { color: red ! important; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_after_bang() {
    let d = check("a { color: red !important; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }
}
