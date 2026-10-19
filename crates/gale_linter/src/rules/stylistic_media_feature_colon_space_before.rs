use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/media-feature-colon-space-before";

declare_rule! {
  /// Require or disallow a space before the colon in media features.
  ///
  /// Equivalent to Stylelint's `@stylistic/media-feature-colon-space-before` rule.
  pub struct StylisticMediaFeatureColonSpaceBefore => StylisticMediaFeatureColonSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the colon in media features",
    default_severity: Warning,
  }
}

/// `@stylistic/media-feature-colon-space-before` as configured.
pub struct StylisticMediaFeatureColonSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticMediaFeatureColonSpaceBeforeCheck {
  /// The plugin's options: `"always"` or `"never"`.  Gale reads no option as
  /// `"never"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticMediaFeatureColonSpaceBeforeCheck {
  /// Flags a media feature colon whose preceding space does not match the option.
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

        // Scan through the media query until { or ;
        let mut paren_depth = 0;
        while i < len && bytes[i] != b'{' && bytes[i] != b';' {
          if bytes[i] == b'(' {
            paren_depth += 1;
          } else if bytes[i] == b')' {
            paren_depth -= 1;
          } else if bytes[i] == b':' && paren_depth > 0 {
            // This is a colon inside a media feature
            let colon_pos = i;
            let has_space_before =
              colon_pos > 0 && (bytes[colon_pos - 1] == b' ' || bytes[colon_pos - 1] == b'\t');

            match option {
              "always" => {
                if !has_space_before {
                  diagnostics.push(
                    Diagnostic::new(NAME, "Expected a space before \":\"".to_string())
                      .span(Span::new(colon_pos, 1)),
                  );
                }
              }
              "never" => {
                if has_space_before {
                  diagnostics.push(
                    Diagnostic::new(NAME, "Unexpected space before \":\"".to_string())
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

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_no_space_before_colon() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media (min-width: 100px) { }";
    let d = configured(&StylisticMediaFeatureColonSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_space_before_colon() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media (min-width : 100px) { }";
    let d = configured(&StylisticMediaFeatureColonSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_requires_space_before_colon() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media (min-width: 100px) { }";
    let d = configured(&StylisticMediaFeatureColonSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/media-feature-colon-space-before\""
      ]
    );
  }
}
