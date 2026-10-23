use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-attribute-brackets-space-inside";

declare_rule! {
  /// Require or disallow a space inside the brackets of attribute selectors.
  ///
  /// Equivalent to `@stylistic/selector-attribute-brackets-space-inside`.
  pub struct StylisticSelectorAttributeBracketsSpaceInside => StylisticSelectorAttributeBracketsSpaceInsideCheck {
    name: NAME,
    description: "Require or disallow a space inside the brackets of attribute selectors",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-attribute-brackets-space-inside` as configured.
pub struct StylisticSelectorAttributeBracketsSpaceInsideCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticSelectorAttributeBracketsSpaceInsideCheck {
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

impl Check for StylisticSelectorAttributeBracketsSpaceInsideCheck {
  /// Flags attribute selector brackets whose inner spaces do not match the option.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    // Track whether we're in a selector context (before `{`)
    let mut in_selector = true;

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

      if bytes[i] == b'{' {
        in_selector = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' {
        in_selector = true;
        i += 1;
        continue;
      }

      // Also treat `;` as returning to selector context (for at-rules, the
      // next thing could be a selector)
      if bytes[i] == b';' {
        in_selector = true;
        i += 1;
        continue;
      }

      // Detect attribute selector brackets in selector context
      if bytes[i] == b'[' && in_selector {
        let open_pos = i;
        // Find matching close bracket
        let mut j = i + 1;
        let mut depth = 1;
        while j < len && depth > 0 {
          if bytes[j] == b'[' {
            depth += 1;
          } else if bytes[j] == b']' {
            depth -= 1;
          }
          if depth > 0 {
            j += 1;
          }
        }

        if depth != 0 {
          i += 1;
          continue;
        }

        let close_pos = j;
        // Check space after opening bracket
        let has_space_after_open = open_pos + 1 < len && bytes[open_pos + 1] == b' ';

        // Check space before closing bracket
        let has_space_before_close = close_pos > 0 && bytes[close_pos - 1] == b' ';

        match option {
          "always" => {
            if !has_space_after_open {
              diagnostics.push(
                Diagnostic::new(NAME, "Expected a space after \"[\"".to_string())
                  .span(Span::new(open_pos, 1)),
              );
            }
            if !has_space_before_close {
              diagnostics.push(
                Diagnostic::new(NAME, "Expected a space before \"]\"".to_string())
                  .span(Span::new(close_pos, 1)),
              );
            }
          }
          "never" => {
            if has_space_after_open {
              diagnostics.push(
                Diagnostic::new(NAME, "Unexpected space after \"[\"".to_string())
                  .span(Span::new(open_pos + 1, 1)),
              );
            }
            if has_space_before_close {
              diagnostics.push(
                Diagnostic::new(NAME, "Unexpected space before \"]\"".to_string())
                  .span(Span::new(close_pos - 1, 1)),
              );
            }
          }
          _ => {}
        }

        i = close_pos + 1;
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
  fn never_allows_no_space_inside_brackets() {
    let source = "a[href] { }";
    let d = configured(&StylisticSelectorAttributeBracketsSpaceInside, None)
      .check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space_inside_brackets() {
    let source = "a[ href ] { }";
    let d = configured(&StylisticSelectorAttributeBracketsSpaceInside, None)
      .check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn always_allows_space_inside_brackets() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a[ href ] { }";
    let d = configured(&StylisticSelectorAttributeBracketsSpaceInside, opt.clone())
      .check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_missing_space_inside_brackets() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a[href] { }";
    let d = configured(&StylisticSelectorAttributeBracketsSpaceInside, opt.clone())
      .check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn skips_brackets_inside_strings() {
    let source = "a::after { content: \"[foo]\"; }";
    let d = configured(&StylisticSelectorAttributeBracketsSpaceInside, None)
      .check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/selector-attribute-brackets-space-inside\""
      ]
    );
  }
}
