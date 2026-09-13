use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline or disallow whitespace after the commas of value lists.
///
/// Equivalent to Stylelint's `@stylistic/value-list-comma-newline-after` rule.
pub struct StylisticValueListCommaNewlineAfter;

impl Rule for StylisticValueListCommaNewlineAfter {
  fn name(&self) -> &'static str {
    "@stylistic/value-list-comma-newline-after"
  }

  fn description(&self) -> &'static str {
    "Require a newline or disallow whitespace after the commas of value lists"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("always");
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut in_value = false;
    let mut paren_depth = 0;
    let mut value_start = 0;
    let value_end = 0;

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

      if bytes[i] == b':' && !in_value && paren_depth == 0 {
        // Only enter value context if this looks like a declaration colon
        // (not a pseudo-selector like :hover, :focus, :not(), etc.)
        // A pseudo-selector colon is followed by an ASCII letter.
        let next = i + 1;
        if next < len && bytes[next].is_ascii_alphabetic() {
          // This is a pseudo-selector colon, skip
          i += 1;
          continue;
        }
        if next < len && bytes[next] == b':' {
          // Double-colon pseudo-element, skip
          i += 1;
          continue;
        }
        // Entering a value after a property colon
        in_value = true;
        value_start = i + 1;
        i += 1;
        continue;
      }

      if bytes[i] == b'(' {
        paren_depth += 1;
        i += 1;
        continue;
      }

      if bytes[i] == b')' {
        if paren_depth > 0 {
          paren_depth -= 1;
        }
        i += 1;
        continue;
      }

      if (bytes[i] == b';' || bytes[i] == b'}') && paren_depth == 0 {
        in_value = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'{' {
        in_value = false;
        i += 1;
        continue;
      }

      // Check commas in value lists (not inside function parens)
      if bytes[i] == b',' && in_value && paren_depth == 0 {
        let comma_pos = i;

        // Find where the value ends to determine if it's multi-line
        let mut ve = i + 1;
        while ve < len && bytes[ve] != b';' && bytes[ve] != b'}' {
          ve += 1;
        }
        // Check if the entire value (from value_start to ve) is multi-line
        let val_slice = &source[value_start..ve];
        let is_multi_line = val_slice.contains('\n');

        let after = i + 1;
        // Check what follows the comma, treating comments as transparent
        let mut k = after;
        let mut has_newline = false;
        let mut has_space = false;
        while k < len && k < ve {
          if bytes[k] == b'\n' || bytes[k] == b'\r' {
            has_newline = true;
            break;
          } else if bytes[k] == b' ' || bytes[k] == b'\t' {
            has_space = true;
            k += 1;
          } else if k + 1 < len && bytes[k] == b'/' && bytes[k + 1] == b'/' {
            // SCSS line comment — implicitly ends with newline
            has_newline = true;
            break;
          } else if k + 1 < len && bytes[k] == b'/' && bytes[k + 1] == b'*' {
            // Block comment — skip to end, then keep checking
            k += 2;
            while k + 1 < len && !(bytes[k] == b'*' && bytes[k + 1] == b'/') {
              k += 1;
            }
            if k + 1 < len {
              k += 2; // skip past `*/`
            }
          } else {
            break;
          }
        }

        match option {
          "always" => {
            if !has_newline {
              diagnostics.push(
                Diagnostic::new(self.name(), "Expected newline after \",\"".to_string())
                  .severity(self.default_severity())
                  .span(Span::new(comma_pos, 1)),
              );
            }
          }
          "never" => {
            if has_newline || has_space {
              diagnostics.push(
                Diagnostic::new(self.name(), "Unexpected whitespace after \",\"".to_string())
                  .severity(self.default_severity())
                  .span(Span::new(comma_pos, 1)),
              );
            }
          }
          "always-multi-line" => {
            if is_multi_line && !has_newline {
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  "Expected newline after \",\" in a multi-line value list".to_string(),
                )
                .severity(self.default_severity())
                .span(Span::new(comma_pos, 1)),
              );
            }
          }
          _ => {}
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
  fn allows_newline_after_value_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a { background: url(foo.png),\n  url(bar.png); }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_newline_after_value_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a { background: url(foo.png), url(bar.png); }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn never_reports_whitespace_after_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "a { background: url(foo.png), url(bar.png); }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn allows_space_on_single_line_with_multi_line_option() {
    let opt = serde_json::Value::String("always-multi-line".to_string());
    let source = "a { background: url(foo.png), url(bar.png); }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_selector_commas() {
    // Commas in selectors should not be treated as value list commas
    let opt = serde_json::Value::String("always".to_string());
    let source = "a:hover, a:focus { color: red; }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "Should not flag selector commas, got: {:?}",
      d.len()
    );
  }

  #[test]
  fn allows_scss_line_comment_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a { background: url(foo.png), // comment\n  url(bar.png); }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "Should not flag comma followed by SCSS line comment, got {} diagnostics",
      d.len()
    );
  }

  #[test]
  fn allows_block_comment_newline_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a { background: url(foo.png), /* comment */\n  url(bar.png); }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "Should not flag comma followed by block comment then newline, got {} diagnostics",
      d.len()
    );
  }

  #[test]
  fn ignores_scss_interpolation() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a { prop: #{$a, $b}; }";
    let d = StylisticValueListCommaNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }
}
