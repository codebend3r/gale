use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the commas of media query lists.
///
/// Equivalent to Stylelint's `@stylistic/media-query-list-comma-space-after` rule.
pub struct StylisticMediaQueryListCommaSpaceAfter;

impl Rule for StylisticMediaQueryListCommaSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/media-query-list-comma-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the commas of media query lists"
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

        // Find the extent of the media query list
        let media_start = i;
        let mut media_end = i;
        while media_end < len && bytes[media_end] != b'{' && bytes[media_end] != b';' {
          media_end += 1;
        }

        let media_query = &source[media_start..media_end];
        let is_single_line = !media_query.contains('\n');

        // Scan for commas
        let mut j = media_start;
        let mut paren_depth = 0;
        while j < media_end {
          if bytes[j] == b'(' {
            paren_depth += 1;
          } else if bytes[j] == b')' {
            paren_depth -= 1;
          } else if bytes[j] == b',' && paren_depth == 0 {
            let comma_pos = j;
            let after = j + 1;

            let has_space_after =
              after < media_end && (bytes[after] == b' ' || bytes[after] == b'\t');
            let has_newline_after =
              after < media_end && (bytes[after] == b'\n' || bytes[after] == b'\r');

            match option {
              "always" => {
                if !has_space_after && !has_newline_after {
                  diagnostics.push(
                    Diagnostic::new(self.name(), "Expected a space after \",\"".to_string())
                      .severity(self.default_severity())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "never" => {
                if has_space_after || has_newline_after {
                  diagnostics.push(
                    Diagnostic::new(self.name(), "Unexpected space after \",\"".to_string())
                      .severity(self.default_severity())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "always-single-line" => {
                if is_single_line && !has_space_after && !has_newline_after {
                  diagnostics.push(
                    Diagnostic::new(
                      self.name(),
                      "Expected a space after \",\" in a single-line media query list".to_string(),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              _ => {}
            }
          }
          j += 1;
        }

        i = media_end;
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
  fn allows_space_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen, print { }";
    let d = StylisticMediaQueryListCommaSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_space_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen,print { }";
    let d = StylisticMediaQueryListCommaSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_reports_space_after_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media screen, print { }";
    let d = StylisticMediaQueryListCommaSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }
}
