use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before the commas of media query lists.
///
/// Equivalent to Stylelint's `@stylistic/media-query-list-comma-space-before` rule.
pub struct StylisticMediaQueryListCommaSpaceBefore;

impl Rule for StylisticMediaQueryListCommaSpaceBefore {
  fn name(&self) -> &'static str {
    "@stylistic/media-query-list-comma-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before the commas of media query lists"
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

      // Detect @media
      if bytes[i] == b'@' {
        i += 1;
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
          i += 1;
        }
        let name = &source[name_start..i];
        if !name.eq_ignore_ascii_case("media") {
          continue;
        }

        // Scan for commas in the media query list until { or ;
        let mut paren_depth = 0;
        while i < len && bytes[i] != b'{' && bytes[i] != b';' {
          if bytes[i] == b'(' {
            paren_depth += 1;
          } else if bytes[i] == b')' {
            paren_depth -= 1;
          } else if bytes[i] == b',' && paren_depth == 0 {
            let comma_pos = i;
            let has_space_before =
              comma_pos > 0 && (bytes[comma_pos - 1] == b' ' || bytes[comma_pos - 1] == b'\t');

            match option {
              "always" => {
                if !has_space_before {
                  diagnostics.push(
                    Diagnostic::new(self.name(), "Expected a space before \",\"".to_string())
                      .severity(self.default_severity())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "never" => {
                if has_space_before {
                  diagnostics.push(
                    Diagnostic::new(self.name(), "Unexpected space before \",\"".to_string())
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

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn allows_no_space_before_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media screen, print { }";
    let d = StylisticMediaQueryListCommaSpaceBefore.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_space_before_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media screen , print { }";
    let d = StylisticMediaQueryListCommaSpaceBefore.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_requires_space_before_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen, print { }";
    let d = StylisticMediaQueryListCommaSpaceBefore.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }
}
