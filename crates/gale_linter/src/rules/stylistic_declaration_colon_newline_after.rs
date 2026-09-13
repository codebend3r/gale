use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline after the colon if the value is multi-line.
///
/// Equivalent to `@stylistic/declaration-colon-newline-after`.
pub struct StylisticDeclarationColonNewlineAfter;

impl Rule for StylisticDeclarationColonNewlineAfter {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-colon-newline-after"
  }

  fn description(&self) -> &'static str {
    "Require a newline after the colon if the value is multi-line"
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
    let mut depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut in_value = false;

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
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip comments
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
        depth += 1;
        in_value = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' {
        depth -= 1;
        if depth < 0 {
          depth = 0;
        }
        in_value = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'(' {
        paren_depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b')' {
        paren_depth -= 1;
        if paren_depth < 0 {
          paren_depth = 0;
        }
        i += 1;
        continue;
      }

      if bytes[i] == b';' || bytes[i] == b'\n' {
        in_value = false;
      }

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Detect property: value pattern inside blocks only.
      // Skip colons inside parentheses (SCSS named function args).
      if bytes[i] == b':' && depth > 0 && paren_depth == 0 && !in_value {
        // Quick check: if the character after the colon is an alpha char
        // (not a space), this is likely a pseudo-selector like :hover, :focus.
        // CSS properties always have a space (or newline) after the colon.
        let after_colon = i + 1;
        if after_colon < len
          && bytes[after_colon] != b' '
          && bytes[after_colon] != b'\t'
          && bytes[after_colon] != b'\n'
          && bytes[after_colon] != b'\r'
          && bytes[after_colon] != b':'
        // :: pseudo-element
        {
          // If the char after colon is alphabetic, this is a pseudo-class.
          // CSS declarations always have whitespace after the colon.
          if bytes[after_colon].is_ascii_alphabetic() {
            i += 1;
            continue;
          }
        }
        // Make sure this isn't a pseudo-selector (::before) or selector (:hover)
        // A declaration colon is preceded by a property name (letters/hyphens)
        let mut k = i;
        if k > 0 {
          k -= 1;
          // Skip whitespace before colon
          while k > 0 && (bytes[k] == b' ' || bytes[k] == b'\t') {
            k -= 1;
          }
          // Check if preceded by identifier chars (property name).
          if bytes[k].is_ascii_alphanumeric() || bytes[k] == b'-' || bytes[k] == b'_' {
            // Scan backwards from the colon to the last statement boundary
            // ({, ;, or start of line after whitespace).  If we find any
            // selector-like character (., #, [, >, +, ~, *, &, :, ))
            // this colon is inside a selector, not a declaration.
            let mut is_selector_colon = false;
            let mut scan = i;
            while scan > 0 {
              scan -= 1;
              let b = bytes[scan];
              if b == b'{' || b == b';' {
                break;
              }
              if b == b'.'
                || b == b'#'
                || b == b'['
                || b == b']'
                || b == b'>'
                || b == b'+'
                || b == b'~'
                || b == b'*'
                || b == b'&'
                || b == b':'
                || b == b')'
              {
                is_selector_colon = true;
                break;
              }
            }
            if is_selector_colon {
              i += 1;
              continue;
            }
            let colon_pos = i;
            in_value = true;

            // Find the end of the value (next ; or })
            let mut end = i + 1;
            while end < len && bytes[end] != b';' && bytes[end] != b'}' {
              // Skip strings in value
              if bytes[end] == b'"' || bytes[end] == b'\'' {
                let q = bytes[end];
                end += 1;
                while end < len && bytes[end] != q {
                  if bytes[end] == b'\\' {
                    end += 1;
                  }
                  end += 1;
                }
              }
              end += 1;
            }

            // Check if value is multi-line
            let value_slice = &bytes[colon_pos + 1..end.min(len)];
            let value_is_multiline = value_slice.contains(&b'\n');

            let should_check = match option {
              "always" => true,
              "always-multi-line" => value_is_multiline,
              _ => false,
            };

            if should_check {
              // Check that character after colon (skipping spaces/tabs) is a newline
              let mut j = colon_pos + 1;
              while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
              }
              // Skip SCSS map/list literals — `(` after colon is the
              // start of a multi-line map, not a value that needs a
              // newline after the colon.
              if j < len && (bytes[j] == b'(' || bytes[j] == b'[') {
                i += 1;
                continue;
              }
              if j < len && bytes[j] != b'\n' {
                diagnostics.push(
                  Diagnostic::new(self.name(), "Expected newline after \":\"")
                    .severity(self.default_severity())
                    .span(Span::new(colon_pos, 1)),
                );
              }
            }
          }
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

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn always_multi_line_allows_single_line_value() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { color: red; }";
    let d = StylisticDeclarationColonNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_reports_multiline_value_without_newline_after_colon() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { background: url()\n    no-repeat; }";
    let d = StylisticDeclarationColonNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn always_multi_line_allows_newline_after_colon_for_multiline_value() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { background:\n    url() no-repeat; }";
    let d = StylisticDeclarationColonNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_missing_newline() {
    let opt = serde_json::json!("always");
    let source = "a { color: red; }";
    let d = StylisticDeclarationColonNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
  }

  #[test]
  fn always_allows_newline_after_colon() {
    let opt = serde_json::json!("always");
    let source = "a { color:\n  red; }";
    let d = StylisticDeclarationColonNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }
}
