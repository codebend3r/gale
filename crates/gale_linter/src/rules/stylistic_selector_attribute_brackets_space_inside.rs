use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space inside the brackets of attribute selectors.
///
/// Equivalent to `@stylistic/selector-attribute-brackets-space-inside`.
pub struct StylisticSelectorAttributeBracketsSpaceInside;

impl Rule for StylisticSelectorAttributeBracketsSpaceInside {
  fn name(&self) -> &'static str {
    "@stylistic/selector-attribute-brackets-space-inside"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space inside the brackets of attribute selectors"
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
    // Track whether we're in a selector context (before `{`)
    let mut in_selector = true;

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

      if bytes[i] == b'{' {
        in_selector = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' {
        in_selector = true;
        i += 1;
        continue;
      }

      // Also treat `;` as returning to selector context (for at-rules, the
      // next thing could be a selector)
      if bytes[i] == b';' {
        in_selector = true;
        i += 1;
        continue;
      }

      // Detect attribute selector brackets in selector context
      if bytes[i] == b'[' && in_selector {
        let open_pos = i;
        // Find matching close bracket
        let mut j = i + 1;
        let mut depth = 1;
        while j < len && depth > 0 {
          if bytes[j] == b'[' {
            depth += 1;
          } else if bytes[j] == b']' {
            depth -= 1;
          }
          if depth > 0 {
            j += 1;
          }
        }

        if depth != 0 {
          i += 1;
          continue;
        }

        let close_pos = j;
        // Check space after opening bracket
        let has_space_after_open = open_pos + 1 < len && bytes[open_pos + 1] == b' ';

        // Check space before closing bracket
        let has_space_before_close = close_pos > 0 && bytes[close_pos - 1] == b' ';

        match option {
          "always" => {
            if !has_space_after_open {
              diagnostics.push(
                Diagnostic::new(self.name(), "Expected a space after \"[\"".to_string())
                  .severity(self.default_severity())
                  .span(Span::new(open_pos, 1)),
              );
            }
            if !has_space_before_close {
              diagnostics.push(
                Diagnostic::new(self.name(), "Expected a space before \"]\"".to_string())
                  .severity(self.default_severity())
                  .span(Span::new(close_pos, 1)),
              );
            }
          }
          "never" => {
            if has_space_after_open {
              diagnostics.push(
                Diagnostic::new(self.name(), "Unexpected space after \"[\"".to_string())
                  .severity(self.default_severity())
                  .span(Span::new(open_pos + 1, 1)),
              );
            }
            if has_space_before_close {
              diagnostics.push(
                Diagnostic::new(self.name(), "Unexpected space before \"]\"".to_string())
                  .severity(self.default_severity())
                  .span(Span::new(close_pos - 1, 1)),
              );
            }
          }
          _ => {}
        }

        i = close_pos + 1;
        continue;
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

  fn ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn never_allows_no_space_inside_brackets() {
    let source = "a[href] { }";
    let d = StylisticSelectorAttributeBracketsSpaceInside.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space_inside_brackets() {
    let source = "a[ href ] { }";
    let d = StylisticSelectorAttributeBracketsSpaceInside.check_root(&[], &ctx(source));
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn always_allows_space_inside_brackets() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a[ href ] { }";
    let d =
      StylisticSelectorAttributeBracketsSpaceInside.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_missing_space_inside_brackets() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a[href] { }";
    let d =
      StylisticSelectorAttributeBracketsSpaceInside.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn skips_brackets_inside_strings() {
    let source = "a::after { content: \"[foo]\"; }";
    let d = StylisticSelectorAttributeBracketsSpaceInside.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }
}
