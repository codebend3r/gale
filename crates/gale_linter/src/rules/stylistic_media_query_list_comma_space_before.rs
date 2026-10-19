use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/media-query-list-comma-space-before";

declare_rule! {
  /// Require or disallow a space before the commas of media query lists.
  ///
  /// Equivalent to Stylelint's `@stylistic/media-query-list-comma-space-before` rule.
  pub struct StylisticMediaQueryListCommaSpaceBefore => StylisticMediaQueryListCommaSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the commas of media query lists",
    default_severity: Warning,
  }
}

/// `@stylistic/media-query-list-comma-space-before` as configured.
pub struct StylisticMediaQueryListCommaSpaceBeforeCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticMediaQueryListCommaSpaceBeforeCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.  Gale reads no option as `"never"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticMediaQueryListCommaSpaceBeforeCheck {
  /// Flags a media query list comma whose preceding space does not match the option.
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
                    Diagnostic::new(NAME, "Expected a space before \",\"".to_string())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "never" => {
                if has_space_before {
                  diagnostics.push(
                    Diagnostic::new(NAME, "Unexpected space before \",\"".to_string())
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

  use crate::testing::{configured, ctx_with_source_and_options, invalid_options};

  #[test]
  fn allows_no_space_before_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media screen, print { }";
    let d = configured(&StylisticMediaQueryListCommaSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_space_before_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media screen , print { }";
    let d = configured(&StylisticMediaQueryListCommaSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_requires_space_before_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen, print { }";
    let d = configured(&StylisticMediaQueryListCommaSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-single-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/media-query-list-comma-space-before\""
      ]
    );
  }
}
