use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/media-feature-range-operator-space-after";

declare_rule! {
  /// Require or disallow a space after the range operator in media features.
  ///
  /// Equivalent to Stylelint's `@stylistic/media-feature-range-operator-space-after` rule.
  pub struct StylisticMediaFeatureRangeOperatorSpaceAfter => StylisticMediaFeatureRangeOperatorSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the range operator in media features",
    default_severity: Warning,
  }
}

/// `@stylistic/media-feature-range-operator-space-after` as configured.
pub struct StylisticMediaFeatureRangeOperatorSpaceAfterCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticMediaFeatureRangeOperatorSpaceAfterCheck {
  /// The plugin's options: `"always"` or `"never"`.  Gale reads no option as
  /// `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticMediaFeatureRangeOperatorSpaceAfterCheck {
  /// Flags a range operator whose trailing space does not match the option.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
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

        // Scan inside media feature parentheses for range operators
        let mut paren_depth = 0;
        while i < len && bytes[i] != b'{' && bytes[i] != b';' {
          if bytes[i] == b'(' {
            paren_depth += 1;
          } else if bytes[i] == b')' {
            paren_depth -= 1;
          } else if paren_depth > 0 {
            // Check for range operators: >=, <=, >, <
            // But not inside colons (those are for name:value syntax)
            let is_range_op = match bytes[i] {
              b'>' | b'<' => true,
              _ => false,
            };

            if is_range_op {
              let op_start = i;
              let mut op_end = i + 1;
              // Check for >= or <=
              if op_end < len && bytes[op_end] == b'=' {
                op_end += 1;
              }
              let op_len = op_end - op_start;

              // Check for space after the operator
              let has_space_after =
                op_end < len && (bytes[op_end] == b' ' || bytes[op_end] == b'\t');

              match option {
                "always" => {
                  if !has_space_after {
                    diagnostics.push(
                      Diagnostic::new(
                        NAME,
                        format!("Expected a space after \"{}\"", &source[op_start..op_end]),
                      )
                      .span(Span::new(op_start, op_len)),
                    );
                  }
                }
                "never" => {
                  if has_space_after {
                    diagnostics.push(
                      Diagnostic::new(
                        NAME,
                        format!("Unexpected space after \"{}\"", &source[op_start..op_end]),
                      )
                      .span(Span::new(op_start, op_len)),
                    );
                  }
                }
                _ => {}
              }

              i = op_end;
              continue;
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

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_space_after_range_operator() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media (width >= 600px) { }";
    let d = configured(&StylisticMediaFeatureRangeOperatorSpaceAfter, opt.clone())
      .check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_space_after_range_operator() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media (width >=600px) { }";
    let d = configured(&StylisticMediaFeatureRangeOperatorSpaceAfter, opt.clone())
      .check_root(&[], &ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_reports_space_after_range_operator() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media (width >= 600px) { }";
    let d = configured(&StylisticMediaFeatureRangeOperatorSpaceAfter, opt.clone())
      .check_root(&[], &ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/media-feature-range-operator-space-after\""
      ]
    );
  }
}
