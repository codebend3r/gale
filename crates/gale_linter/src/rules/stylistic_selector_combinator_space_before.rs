use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-combinator-space-before";

declare_rule! {
  /// Require or disallow a space before combinators of selectors.
  ///
  /// Equivalent to Stylelint's `@stylistic/selector-combinator-space-before` rule.
  pub struct StylisticSelectorCombinatorSpaceBefore => StylisticSelectorCombinatorSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before combinators of selectors",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-combinator-space-before` as configured.
pub struct StylisticSelectorCombinatorSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticSelectorCombinatorSpaceBeforeCheck {
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

const COMBINATORS: &[u8] = b">~+";

impl Check for StylisticSelectorCombinatorSpaceBeforeCheck {
  /// Flags a selector combinator whose preceding space does not match the option.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut in_selector = true;
    let mut depth = 0_i32;
    let mut paren_depth = 0_i32;

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

      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
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
        in_selector = false;
        depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b'}' {
        depth -= 1;
        if depth < 0 {
          depth = 0;
        }
        in_selector = depth > 0;
        i += 1;
        continue;
      }
      if bytes[i] == b';' {
        in_selector = depth > 0;
        i += 1;
        continue;
      }

      // Track parentheses — combinators inside parens are either
      // :has()/:not() relative selectors or function-call arguments,
      // neither of which Stylelint flags.
      if bytes[i] == b'(' {
        paren_depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b')' {
        paren_depth -= 1;
        if paren_depth < 0 {
          paren_depth = 0;
        }
        i += 1;
        continue;
      }

      // Only check combinators in selector context and outside parens
      if in_selector && paren_depth == 0 && COMBINATORS.contains(&bytes[i]) {
        let comb_pos = i;

        // Check if preceded by a space
        let has_space_before =
          comb_pos > 0 && (bytes[comb_pos - 1] == b' ' || bytes[comb_pos - 1] == b'\t');

        match option {
          "always" => {
            if !has_space_before {
              diagnostics.push(
                Diagnostic::new(
                  NAME,
                  format!(
                    "Expected a space before \"{}\"",
                    char::from(bytes[comb_pos])
                  ),
                )
                .span(Span::new(comb_pos, 1)),
              );
            }
          }
          "never" => {
            if has_space_before {
              diagnostics.push(
                Diagnostic::new(
                  NAME,
                  format!(
                    "Unexpected space before \"{}\"",
                    char::from(bytes[comb_pos])
                  ),
                )
                .span(Span::new(comb_pos, 1)),
              );
            }
          }
          _ => {}
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

  use crate::testing::{configured, ctx_with_source_and_options, invalid_options};

  #[test]
  fn allows_space_before_combinator() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a > b { }";
    let d = configured(&StylisticSelectorCombinatorSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_space_before_combinator() {
    let opt = serde_json::Value::String("always".to_string());
    let source = "a> b { }";
    let d = configured(&StylisticSelectorCombinatorSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_reports_space_before_combinator() {
    let opt = serde_json::Value::String("never".to_string());
    let source = "a > b { }";
    let d = configured(&StylisticSelectorCombinatorSpaceBefore, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
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
        "Invalid option value \"sometimes\" for rule \"@stylistic/selector-combinator-space-before\""
      ]
    );
  }
}
