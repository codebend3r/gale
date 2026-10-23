use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/media-query-list-comma-space-after";

declare_rule! {
  /// Require or disallow a space after the commas of media query lists.
  ///
  /// Equivalent to Stylelint's `@stylistic/media-query-list-comma-space-after` rule.
  pub struct StylisticMediaQueryListCommaSpaceAfter => StylisticMediaQueryListCommaSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the commas of media query lists",
    default_severity: Warning,
  }
}

/// `@stylistic/media-query-list-comma-space-after` as configured.
pub struct StylisticMediaQueryListCommaSpaceAfterCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticMediaQueryListCommaSpaceAfterCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.  Gale reads no option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticMediaQueryListCommaSpaceAfterCheck {
  /// Flags a media query list comma whose trailing space does not match the option.
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
                    Diagnostic::new(NAME, "Expected a space after \",\"".to_string())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "never" => {
                if has_space_after || has_newline_after {
                  diagnostics.push(
                    Diagnostic::new(NAME, "Unexpected space after \",\"".to_string())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "always-single-line" => {
                if is_single_line && !has_space_after && !has_newline_after {
                  diagnostics.push(
                    Diagnostic::new(
                      NAME,
                      "Expected a space after \",\" in a single-line media query list".to_string(),
                    )
                    .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "never-single-line" => {
                if is_single_line && has_space_after {
                  diagnostics.push(
                    Diagnostic::new(
                      NAME,
                      "Unexpected whitespace after \",\" in a single-line list",
                    )
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

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_space_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen, print { }";
    let d = configured(&StylisticMediaQueryListCommaSpaceAfter, opt.clone())
      .check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_space_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen,print { }";
    let d = configured(&StylisticMediaQueryListCommaSpaceAfter, opt.clone())
      .check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_reports_space_after_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media screen, print { }";
    let d = configured(&StylisticMediaQueryListCommaSpaceAfter, opt.clone())
      .check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
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
        "Invalid option value \"sometimes\" for rule \"@stylistic/media-query-list-comma-space-after\""
      ]
    );
  }

  #[test]
  fn never_single_line_checks_single_line_lists_only() {
    use gale_css_parser::Syntax;

    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-single-line"),
      "@media screen, print {}",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
      messages,
      ["Unexpected whitespace after \",\" in a single-line list"] as [&str; 1]
    );
    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-single-line"),
      "@media screen, print,\nprojection {}",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, [] as [&str; 0]);
  }
}
