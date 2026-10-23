use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/block-opening-brace-newline-after";

declare_rule! {
  /// Require a newline after opening braces.
  ///
  /// Equivalent to `@stylistic/block-opening-brace-newline-after`.
  pub struct StylisticBlockOpeningBraceNewlineAfter => StylisticBlockOpeningBraceNewlineAfterCheck {
    name: NAME,
    description: "Require a newline after opening braces",
    default_severity: Warning,
  }
}

/// `@stylistic/block-opening-brace-newline-after` as configured.
pub struct StylisticBlockOpeningBraceNewlineAfterCheck {
  /// The primary option: `"always"`, `"always-multi-line"` or
  /// `"never-multi-line"`.
  option: &'static str,
}

impl StylisticBlockOpeningBraceNewlineAfterCheck {
  /// The plugin's options: `"always"`, `"always-multi-line"` or
  /// `"never-multi-line"`.  Gale reads no option as `"always"`.  `ignore` is
  /// accepted but not yet applied.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "always-multi-line", "never-multi-line"]);
    options.keywords("ignore", &["rules"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticBlockOpeningBraceNewlineAfterCheck {
  /// Flags an opening brace whose trailing newline does not match the option.
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
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      if bytes[i] == b'{' {
        let brace_pos = i;

        // Find matching closing brace to determine if block is multi-line
        let mut depth = 1;
        let mut j = i + 1;
        let mut block_is_multi_line = false;
        while j < len && depth > 0 {
          if bytes[j] == b'{' {
            depth += 1;
          } else if bytes[j] == b'}' {
            depth -= 1;
          } else if bytes[j] == b'\n' {
            block_is_multi_line = true;
          }
          j += 1;
        }

        // Check what follows the opening brace (skip spaces/tabs)
        let mut after = brace_pos + 1;
        while after < len && (bytes[after] == b' ' || bytes[after] == b'\t') {
          after += 1;
        }
        // If next non-whitespace is a comment, skip it, then check for newline
        let mut check_pos = after;
        if check_pos + 1 < len && bytes[check_pos] == b'/' && bytes[check_pos + 1] == b'*' {
          // Skip block comment /* ... */
          check_pos += 2;
          while check_pos + 1 < len && !(bytes[check_pos] == b'*' && bytes[check_pos + 1] == b'/') {
            check_pos += 1;
          }
          if check_pos + 1 < len {
            check_pos += 2; // skip */
          }
          // Skip trailing spaces/tabs after the comment
          while check_pos < len && (bytes[check_pos] == b' ' || bytes[check_pos] == b'\t') {
            check_pos += 1;
          }
        }
        let has_newline = check_pos < len
          && (bytes[check_pos] == b'\n'
            || (bytes[check_pos] == b'\r' && check_pos + 1 < len && bytes[check_pos + 1] == b'\n'))
          || (after + 1 < len && bytes[after] == b'/' && bytes[after + 1] == b'/');

        let should_check = match option {
          "always" => true,
          "always-multi-line" => block_is_multi_line,
          "never-multi-line" => block_is_multi_line,
          _ => false,
        };

        if should_check {
          let expect_newline = option != "never-multi-line";
          if expect_newline && !has_newline {
            diagnostics.push(
              Diagnostic::new(NAME, "Expected newline after \"{\"".to_string())
                .span(Span::new(brace_pos, 1)),
            );
          } else if !expect_newline && has_newline {
            diagnostics.push(
              Diagnostic::new(NAME, "Unexpected newline after \"{\"".to_string())
                .span(Span::new(brace_pos, 1)),
            );
          }
        }
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
  fn allows_newline_after_brace() {
    let rule = configured(&StylisticBlockOpeningBraceNewlineAfter, None);
    let source = "a {\n  color: red;\n}";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_newline_after_brace() {
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticBlockOpeningBraceNewlineAfter, opt.clone());
    let source = "a { color: red;\n}";
    let d = rule.check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn always_multi_line_allows_single_line_without_newline() {
    let opt = serde_json::Value::String("always-multi-line".to_string());
    let rule = configured(&StylisticBlockOpeningBraceNewlineAfter, opt.clone());
    let source = "a { color: red; }";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_reports_multi_line_without_newline_after_brace() {
    let opt = serde_json::Value::String("always-multi-line".to_string());
    let rule = configured(&StylisticBlockOpeningBraceNewlineAfter, opt.clone());
    let source = "a { color: red;\nbackground: blue; }";
    let d = rule.check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_comments() {
    let rule = configured(&StylisticBlockOpeningBraceNewlineAfter, None);
    let source = "/* { no newline } */ a {\n  color: red;\n}";
    let d = rule.check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never-multi-line", { "ignore": ["rules"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("never")),
      ["Invalid option value \"never\" for rule \"@stylistic/block-opening-brace-newline-after\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "ignore": ["at-rules"] }])
      ),
      [
        "Invalid value \"at-rules\" for option \"ignore\" of rule \"@stylistic/block-opening-brace-newline-after\""
      ]
    );
  }
}
