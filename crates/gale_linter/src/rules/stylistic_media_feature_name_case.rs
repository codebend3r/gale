use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require lowercase for media feature names.
///
/// Equivalent to `@stylistic/media-feature-name-case`.
pub struct StylisticMediaFeatureNameCase;

impl Rule for StylisticMediaFeatureNameCase {
  fn name(&self) -> &'static str {
    "@stylistic/media-feature-name-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for media feature names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("lower");
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
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip block comments
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
        let mut depth = 1;
        while i < len && depth > 0 {
          if bytes[i] == b'{' {
            depth += 1;
          } else if bytes[i] == b'}' {
            depth -= 1;
          }
          if depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
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

        // Scan through the media query until { or ;
        let mut paren_depth: i32 = 0;
        while i < len && bytes[i] != b'{' && bytes[i] != b';' {
          // Skip SCSS interpolation inside media queries
          if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
            i += 2;
            let mut id = 1;
            while i < len && id > 0 {
              if bytes[i] == b'{' {
                id += 1;
              } else if bytes[i] == b'}' {
                id -= 1;
              }
              if id > 0 {
                i += 1;
              }
            }
            if i < len {
              i += 1;
            }
            continue;
          }

          if bytes[i] == b'(' {
            paren_depth += 1;
            i += 1;

            // We just entered a media feature parenthesized expression.
            // Extract the feature name (the identifier right after '(').
            // Skip whitespace after '('
            while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
              i += 1;
            }

            // Read the feature name
            let feat_start = i;
            while i < len
              && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
            {
              i += 1;
            }
            let feat_end = i;
            if feat_start == feat_end {
              continue;
            }

            let feat_name = &source[feat_start..feat_end];

            // Only flag if followed by ':' or ')' (i.e., it's a feature name,
            // not just a value like `screen`).
            // Skip whitespace
            let mut peek = i;
            while peek < len && (bytes[peek] == b' ' || bytes[peek] == b'\t') {
              peek += 1;
            }
            if peek >= len || (bytes[peek] != b':' && bytes[peek] != b')') {
              // Could be a range expression like (width > 100px).
              // Check if the name has uppercase chars anyway.
              let has_violation = match option {
                "lower" => feat_name.chars().any(|c| c.is_ascii_uppercase()),
                "upper" => feat_name.chars().any(|c| c.is_ascii_lowercase()),
                _ => false,
              };
              if has_violation {
                let expected = match option {
                  "lower" => feat_name.to_ascii_lowercase(),
                  "upper" => feat_name.to_ascii_uppercase(),
                  _ => feat_name.to_string(),
                };
                diagnostics.push(
                  Diagnostic::new(
                    self.name(),
                    format!("Expected \"{}\" to be \"{}\"", feat_name, expected),
                  )
                  .severity(self.default_severity())
                  .span(Span::new(feat_start, feat_end - feat_start)),
                );
              }
              continue;
            }

            let has_violation = match option {
              "lower" => feat_name.chars().any(|c| c.is_ascii_uppercase()),
              "upper" => feat_name.chars().any(|c| c.is_ascii_lowercase()),
              _ => false,
            };

            if has_violation {
              let expected = match option {
                "lower" => feat_name.to_ascii_lowercase(),
                "upper" => feat_name.to_ascii_uppercase(),
                _ => feat_name.to_string(),
              };
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Expected \"{}\" to be \"{}\"", feat_name, expected),
                )
                .severity(self.default_severity())
                .span(Span::new(feat_start, feat_end - feat_start)),
              );
            }
            continue;
          } else if bytes[i] == b')' {
            paren_depth -= 1;
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

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticMediaFeatureNameCase;
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
  fn lower_accepts_lowercase() {
    let d = check("@media (min-width: 100px) { }", "lower");
    assert!(d.is_empty());
  }

  #[test]
  fn lower_rejects_uppercase() {
    let d = check("@media (MIN-WIDTH: 100px) { }", "lower");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("min-width"));
  }

  #[test]
  fn lower_rejects_mixed_case() {
    let d = check("@media (Min-Width: 100px) { }", "lower");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("min-width"));
  }

  #[test]
  fn upper_accepts_uppercase() {
    let d = check("@media (MIN-WIDTH: 100px) { }", "upper");
    assert!(d.is_empty());
  }

  #[test]
  fn upper_rejects_lowercase() {
    let d = check("@media (min-width: 100px) { }", "upper");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MIN-WIDTH"));
  }
}
