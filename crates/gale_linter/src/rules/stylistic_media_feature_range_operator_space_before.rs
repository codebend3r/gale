use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/media-feature-range-operator-space-before";

declare_rule! {
  /// Require or disallow a space before the range operator in media features.
  ///
  /// Equivalent to `@stylistic/media-feature-range-operator-space-before`.
  pub struct StylisticMediaFeatureRangeOperatorSpaceBefore => StylisticMediaFeatureRangeOperatorSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the range operator in media features",
    default_severity: Warning,
  }
}

/// `@stylistic/media-feature-range-operator-space-before` as configured.
pub struct StylisticMediaFeatureRangeOperatorSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticMediaFeatureRangeOperatorSpaceBeforeCheck {
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

impl Check for StylisticMediaFeatureRangeOperatorSpaceBeforeCheck {
  /// Flags a range operator whose preceding space does not match the option.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
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
          } else if paren_depth > 0 && (bytes[i] == b'>' || bytes[i] == b'<') {
            let op_start = i;
            let mut op_end = i + 1;
            if op_end < len && bytes[op_end] == b'=' {
              op_end += 1;
            }
            let op_len = op_end - op_start;

            // Check for space before the operator
            let has_space_before =
              op_start > 0 && (bytes[op_start - 1] == b' ' || bytes[op_start - 1] == b'\t');

            match option {
              "always" => {
                if !has_space_before {
                  diagnostics.push(
                    Diagnostic::new(
                      NAME,
                      format!("Expected a space before \"{}\"", &source[op_start..op_end]),
                    )
                    .span(Span::new(op_start, op_len)),
                  );
                }
              }
              "never" => {
                if has_space_before {
                  diagnostics.push(
                    Diagnostic::new(
                      NAME,
                      format!("Unexpected space before \"{}\"", &source[op_start..op_end]),
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

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StylisticMediaFeatureRangeOperatorSpaceBefore, opts.clone());
    let ctx = ctx_with_source(source);
    rule.check_source(&ctx)
  }

  #[test]
  fn always_accepts_space_before_range_operator() {
    let d = check("@media (width >= 600px) { }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_missing_space_before_range_operator() {
    let d = check("@media (width>= 600px) { }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_reports_space_before_range_operator() {
    let d = check("@media (width >= 600px) { }", "never");
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn never_accepts_no_space_before() {
    let d = check("@media (width>=600px) { }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/media-feature-range-operator-space-before\""
      ]
    );
  }
}
