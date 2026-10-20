use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/media-query-list-comma-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the commas of media query lists.
  ///
  /// Equivalent to Stylelint's `@stylistic/media-query-list-comma-newline-after` rule.
  pub struct StylisticMediaQueryListCommaNewlineAfter => StylisticMediaQueryListCommaNewlineAfterCheck {
    name: NAME,
    description: "Require a newline or disallow whitespace after the commas of media query lists",
    default_severity: Warning,
  }
}

/// `@stylistic/media-query-list-comma-newline-after` as configured.
pub struct StylisticMediaQueryListCommaNewlineAfterCheck {
  /// The primary option: `"always"`, `"always-multi-line"`,
  /// `"never-multi-line"` or gale's own `"never"`.
  option: &'static str,
}

impl StylisticMediaQueryListCommaNewlineAfterCheck {
  /// The plugin's options: `"always"`, `"always-multi-line"` or
  /// `"never-multi-line"`.  Gale also accepts `"never"`, which the plugin does
  /// not.  Gale reads no option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "always-multi-line", "never-multi-line", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticMediaQueryListCommaNewlineAfterCheck {
  /// Flags a media query list comma whose trailing newline does not match the option.
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

        // Find the extent of the media query list (until { or ;)
        let media_start = i;
        let mut media_end = i;
        while media_end < len && bytes[media_end] != b'{' && bytes[media_end] != b';' {
          media_end += 1;
        }

        let media_query = &source[media_start..media_end];
        let is_multi_line = media_query.contains('\n');

        // Scan for commas in the media query list
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

            // Check what follows the comma
            let mut k = after;
            let mut has_newline = false;
            let mut has_space = false;
            while k < media_end {
              if bytes[k] == b'\n' || bytes[k] == b'\r' {
                has_newline = true;
                break;
              } else if bytes[k] == b' ' || bytes[k] == b'\t' {
                has_space = true;
                k += 1;
              } else {
                break;
              }
            }

            match option {
              "always" => {
                if !has_newline {
                  diagnostics.push(
                    Diagnostic::new(NAME, "Expected newline after \",\"".to_string())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "never" => {
                if has_newline || has_space {
                  diagnostics.push(
                    Diagnostic::new(NAME, "Unexpected whitespace after \",\"".to_string())
                      .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "always-multi-line" => {
                if is_multi_line && !has_newline {
                  diagnostics.push(
                    Diagnostic::new(
                      NAME,
                      "Expected newline after \",\" in a multi-line media query list".to_string(),
                    )
                    .span(Span::new(comma_pos, 1)),
                  );
                }
              }
              "never-multi-line" => {
                if is_multi_line && (has_newline || has_space) {
                  diagnostics.push(
                    Diagnostic::new(
                      NAME,
                      "Unexpected whitespace after \",\" in a multi-line list",
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

  use crate::testing::{configured, ctx_with_source_and_options, invalid_options};

  #[test]
  fn allows_newline_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen,\nprint { }";
    let d = configured(&StylisticMediaQueryListCommaNewlineAfter, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_newline_after_comma() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "@media screen, print { }";
    let d = configured(&StylisticMediaQueryListCommaNewlineAfter, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn never_disallows_whitespace_after_comma() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "@media screen, print { }";
    let d = configured(&StylisticMediaQueryListCommaNewlineAfter, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-multi-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-multi-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/media-query-list-comma-newline-after\""
      ]
    );
  }

  #[test]
  fn never_multi_line_checks_multi_line_lists_only() {
    use gale_css_parser::Syntax;

    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-multi-line"),
      "@media screen, print,\nprojection {}",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
      messages,
      [
        "Unexpected whitespace after \",\" in a multi-line list",
        "Unexpected whitespace after \",\" in a multi-line list"
      ] as [&str; 2]
    );
    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-multi-line"),
      "@media screen, print {}",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, [] as [&str; 0]);
  }
}
