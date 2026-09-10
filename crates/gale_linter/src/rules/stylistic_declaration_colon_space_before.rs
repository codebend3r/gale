use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before the colon in declarations.
///
/// Primary: "always" | "never"
pub struct StylisticDeclarationColonSpaceBefore;

impl Rule for StylisticDeclarationColonSpaceBefore {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-colon-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before the colon in declarations"
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

      if bytes[i] == b':' && is_declaration_colon(bytes, i) {
        let colon_pos = i;
        let has_space_before = colon_pos > 0 && bytes[colon_pos - 1] == b' ';

        let violation = match option {
          "always" => !has_space_before,
          "never" => has_space_before,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" => "Expected a space before the colon",
            "never" => "Unexpected space before the colon",
            _ => "Unexpected space before the colon",
          };
          diagnostics.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(colon_pos, 1)),
          );
        }
      }
      i += 1;
    }

    diagnostics
  }
}

fn is_declaration_colon(bytes: &[u8], pos: usize) -> bool {
  if pos == 0 {
    return false;
  }

  // Check what follows the colon: if it's immediately a letter leading to `(`,
  // this is a pseudo-class like `:where(`, `:not(`, `:is(`, `:has(` — not a declaration.
  if pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_alphabetic() {
    let mut m = pos + 1;
    while m < bytes.len() && (bytes[m].is_ascii_alphanumeric() || bytes[m] == b'-') {
      m += 1;
    }
    if m < bytes.len() && bytes[m] == b'(' {
      return false;
    }
  }

  // Check for `::` pseudo-element
  if pos + 1 < bytes.len() && bytes[pos + 1] == b':' {
    return false;
  }

  let mut j = pos - 1;
  while j > 0 && (bytes[j] == b' ' || bytes[j] == b'\t') {
    j -= 1;
  }
  let ch = bytes[j];
  if !(ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'_') {
    return false;
  }
  let mut k = j;
  loop {
    let c = bytes[k];
    if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' {
      if k == 0 {
        return false;
      }
      k -= 1;
    } else {
      break;
    }
  }
  while k > 0 && (bytes[k] == b' ' || bytes[k] == b'\t' || bytes[k] == b'\n' || bytes[k] == b'\r') {
    k -= 1;
  }
  matches!(bytes[k], b'{' | b';' | b'}')
    || (k == 0 && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'-' || bytes[k] == b'_'))
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticDeclarationColonSpaceBefore;
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
  fn never_accepts_no_space() {
    let d = check("a { color: red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_before() {
    let d = check("a { color : red; }", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_accepts_space_before() {
    let d = check("a { color : red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space() {
    let d = check("a { color: red; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }
}
