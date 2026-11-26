use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::chars::is_alnum_or_hyphen;
use crate::chars::is_word_or_hyphen_byte;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, LetterCase, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/media-feature-name-case";

declare_rule! {
  /// Require lowercase for media feature names.
  ///
  /// Equivalent to `@stylistic/media-feature-name-case`.
  pub struct StylisticMediaFeatureNameCase => StylisticMediaFeatureNameCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for media feature names",
    default_severity: Warning,
  }
}

/// `@stylistic/media-feature-name-case` as configured.
pub struct StylisticMediaFeatureNameCaseCheck {
  /// The case media feature names are written in.
  case: LetterCase,
}

impl StylisticMediaFeatureNameCaseCheck {
  /// The plugin's options: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let case = options.primary_case();
    options.finish()?;
    Ok(Self { case })
  }
}

impl Check for StylisticMediaFeatureNameCaseCheck {
  /// Flags media feature names not in the configured case.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let case = self.case;
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
        while i < len && is_alnum_or_hyphen(char::from(bytes[i])) {
          i += 1;
        }
        let name = &source[name_start..i];
        if !name.eq_ignore_ascii_case("media") {
          continue;
        }

        // Scan through the media query until { or ;
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
            i += 1;

            // We just entered a media feature parenthesized expression.
            // Extract the feature name (the identifier right after '(').
            // Skip whitespace after '('
            while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
              i += 1;
            }

            // Read the feature name
            let feat_start = i;
            while i < len && is_word_or_hyphen_byte(bytes[i]) {
              i += 1;
            }
            let feat_end = i;
            if feat_start == feat_end {
              continue;
            }

            let feat_name = &source[feat_start..feat_end];

            // The name is checked whatever follows it, `:` or `)` for a
            // feature, or a range operator as in `(width > 100px)`.
            if case.is_wrong(feat_name) {
              let expected = case.apply(feat_name);
              diagnostics.push(
                Diagnostic::new(
                  NAME,
                  format!("Expected \"{}\" to be \"{}\"", feat_name, expected),
                )
                .span(Span::new(feat_start, feat_end - feat_start)),
              );
            }
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
    let rule = configured(&StylisticMediaFeatureNameCase, opts.clone());
    let ctx = ctx_with_source(source);
    rule.check_source(&ctx)
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

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("lower")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/media-feature-name-case\""]
    );
  }
}
