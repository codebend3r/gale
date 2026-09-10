use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the colon in media features.
///
/// Equivalent to `@stylistic/media-feature-colon-space-after`.
pub struct StylisticMediaFeatureColonSpaceAfter;

impl Rule for StylisticMediaFeatureColonSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/media-feature-colon-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the colon in media features"
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
        let at_pos = i;
        i += 1;
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
          i += 1;
        }
        let name = &source[name_start..i];
        if !name.eq_ignore_ascii_case("media") {
          continue;
        }

        // Scan the media prelude up to the opening brace or semicolon
        let mut paren_depth = 0;
        while i < len && bytes[i] != b'{' && bytes[i] != b';' {
          // Skip strings inside media
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

          // Skip comments inside media
          if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
              i += 1;
            }
            i += 2;
            continue;
          }

          if bytes[i] == b'(' {
            paren_depth += 1;
            i += 1;
            continue;
          }

          if bytes[i] == b')' {
            paren_depth -= 1;
            i += 1;
            continue;
          }

          // Check colons inside parens (media feature colons)
          if bytes[i] == b':' && paren_depth > 0 {
            let colon_pos = i;
            let has_space_after = colon_pos + 1 < len && bytes[colon_pos + 1] == b' ';

            match option {
              "always" => {
                if !has_space_after {
                  diagnostics.push(
                    Diagnostic::new(
                      self.name(),
                      "Expected a space after \":\" in media feature".to_string(),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(colon_pos, 1)),
                  );
                }
              }
              "never" => {
                if has_space_after {
                  diagnostics.push(
                    Diagnostic::new(
                      self.name(),
                      "Unexpected space after \":\" in media feature".to_string(),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(colon_pos, 1)),
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

  fn ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn allows_space_after_colon() {
    let source = "@media (min-width: 768px) { }";
    let d = StylisticMediaFeatureColonSpaceAfter.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_space_after_colon() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media (min-width:768px) { }";
    let d = StylisticMediaFeatureColonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_allows_no_space_after_colon() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media (min-width:768px) { }";
    let d = StylisticMediaFeatureColonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space_after_colon() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media (min-width: 768px) { }";
    let d = StylisticMediaFeatureColonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn does_not_flag_non_media_colons() {
    let source = "a { color: red; }";
    let d = StylisticMediaFeatureColonSpaceAfter.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }
}
