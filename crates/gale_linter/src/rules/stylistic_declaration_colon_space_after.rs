use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the colon in declarations.
///
/// Primary: "always" | "never" | "always-single-line"
pub struct StylisticDeclarationColonSpaceAfter;

impl Rule for StylisticDeclarationColonSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-colon-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the colon in declarations"
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
      // Skip selectors / at-rule preludes — only check inside declaration blocks
      // We detect property: value patterns by looking for colons that are NOT
      // inside selectors (pseudo-classes like :hover) or @-rules.
      // A declaration colon is one where the left side is a property name.
      if bytes[i] == b':' {
        // Check if this is a declaration colon (preceded by a property-like identifier)
        if is_declaration_colon(bytes, i) {
          let colon_pos = i;
          let after = colon_pos + 1;
          let has_space_after = after < len && bytes[after] == b' ';
          let is_single_line = {
            // Find the end of the declaration (semicolon or closing brace)
            let mut end = after;
            while end < len && bytes[end] != b';' && bytes[end] != b'}' {
              end += 1;
            }
            !ctx.source[colon_pos..end.min(len)].contains('\n')
          };

          let violation = match option {
            "always" => !has_space_after,
            "never" => has_space_after,
            "always-single-line" => is_single_line && !has_space_after,
            _ => false,
          };

          if violation {
            let msg = match option {
              "always" => "Expected single space after \":\"",
              "always-single-line" => {
                "Expected single space after \":\" with a single-line declaration"
              }
              "never" => "Unexpected space after \":\"",
              _ => "Expected single space after \":\"",
            };
            // Point after the colon (where the space should be)
            let report_pos = if option == "never" {
              colon_pos
            } else {
              colon_pos + 1
            };
            diagnostics.push(
              Diagnostic::new(self.name(), msg)
                .severity(self.default_severity())
                .span(Span::new(report_pos, 1)),
            );
          }
        }
      }
      i += 1;
    }

    diagnostics
  }
}

/// Heuristic: a colon at `pos` is a declaration colon if the non-whitespace
/// character(s) before it look like a CSS property name (alphanumeric, hyphen,
/// underscore, or custom property `--`), and not a selector pseudo-class context.
fn is_declaration_colon(bytes: &[u8], pos: usize) -> bool {
  if pos == 0 {
    return false;
  }

  // Handle double-colon pseudo-elements (::before, ::after)
  if pos + 1 < bytes.len() && bytes[pos + 1] == b':' {
    return false;
  }

  // If the character immediately after the colon is an ASCII letter (no space),
  // check if the word after the colon is followed by selector-like characters
  // (`{`, `,`, `.`, `#`, `[`, `:`, or whitespace then `{`/`,`) which indicates
  // a pseudo-class (e.g. input:focus { }), OR if it's followed by `;`/`!`/`}`
  // which indicates a declaration value (e.g. color:red;).
  // Note: `(` is NOT included because `color:var(...)` is a valid declaration,
  // not a pseudo-class.
  if pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_alphabetic() {
    // Scan forward past the word after the colon
    let mut f = pos + 1;
    while f < bytes.len()
      && (bytes[f].is_ascii_alphanumeric() || bytes[f] == b'-' || bytes[f] == b'_')
    {
      f += 1;
    }
    // Skip whitespace
    while f < bytes.len() && (bytes[f] == b' ' || bytes[f] == b'\t') {
      f += 1;
    }
    // If followed by `{`, `,`, `.`, `#`, `[`, `:`, `&`, or a combinator
    // (`~`, `+`, `>`), it's a selector context.
    if f < bytes.len()
      && matches!(
        bytes[f],
        b'{' | b',' | b'.' | b'#' | b'[' | b':' | b'&' | b'~' | b'+' | b'>'
      )
    {
      return false;
    }
    // `(` after an identifier means pseudo-class function (`:not(`, `:has(`)
    // UNLESS the identifier is a known CSS value function (`var(`, `calc(`).
    if f < bytes.len() && bytes[f] == b'(' {
      let word = std::str::from_utf8(&bytes[pos + 1..f]).unwrap_or("");
      let word_lower = word.trim().to_ascii_lowercase();
      let is_css_fn = matches!(
        word_lower.as_str(),
        "var"
          | "calc"
          | "min"
          | "max"
          | "clamp"
          | "env"
          | "rgb"
          | "rgba"
          | "hsl"
          | "hsla"
          | "hwb"
          | "lab"
          | "lch"
          | "oklch"
          | "oklab"
          | "color"
          | "color-mix"
          | "linear-gradient"
          | "radial-gradient"
          | "conic-gradient"
          | "url"
          | "counter"
          | "counters"
          | "attr"
          | "translate"
          | "rotate"
          | "scale"
          | "skew"
          | "matrix"
          | "perspective"
          | "cubic-bezier"
          | "steps"
          | "image-set"
          | "cross-fade"
          | "paint"
          | "format"
          | "local"
          | "minmax"
          | "repeat"
          | "fit-content"
          | "inset"
          | "circle"
          | "ellipse"
          | "polygon"
          | "path"
      );
      if !is_css_fn {
        return false; // pseudo-class function like :not(), :has()
      }
    }
  }

  // Walk back over whitespace
  let mut j = pos - 1;
  while j > 0 && (bytes[j] == b' ' || bytes[j] == b'\t') {
    j -= 1;
  }
  // The character before should be a valid property-name char
  let ch = bytes[j];
  if !(ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'_') {
    return false;
  }
  // Walk back over the identifier to check it's not inside a selector context
  // (e.g. `a:hover`). Declaration properties start after `{`, `;`, or start-of-line
  // within a block.
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
  // Skip whitespace before the property name
  while k > 0 && (bytes[k] == b' ' || bytes[k] == b'\t' || bytes[k] == b'\n' || bytes[k] == b'\r') {
    k -= 1;
  }
  // If preceded by `{`, `;`, start of string, or `}` (nested), it's a declaration
  matches!(bytes[k], b'{' | b';' | b'}')
    || (k == 0 && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'-' || bytes[k] == b'_'))
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticDeclarationColonSpaceAfter;
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
  fn always_accepts_space_after() {
    let d = check("a { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space() {
    let d = check("a { color:red; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected single space"));
  }

  #[test]
  fn never_accepts_no_space() {
    let d = check("a { color:red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space() {
    let d = check("a { color: red; }", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn ignores_pseudo_classes() {
    let d = check("a:hover { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_nested_pseudo_classes() {
    // In SCSS nesting, input:focus inside a block should not be flagged
    let d = check(".parent { input:focus { color: red; } }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_pseudo_elements() {
    let d = check("a::before { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_not_pseudo() {
    let d = check("a:not(.foo) { color: red; }", "always");
    assert!(d.is_empty());
  }
}
