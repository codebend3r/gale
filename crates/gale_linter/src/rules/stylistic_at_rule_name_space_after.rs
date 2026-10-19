use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/at-rule-name-space-after";

declare_rule! {
  /// Require a space after at-rule names.
  ///
  /// Equivalent to Stylelint's `@stylistic/at-rule-name-space-after` rule.
  pub struct StylisticAtRuleNameSpaceAfter => StylisticAtRuleNameSpaceAfterCheck {
    name: NAME,
    description: "Require a space after at-rule names",
    default_severity: Warning,
  }
}

/// `@stylistic/at-rule-name-space-after` as configured.
pub struct StylisticAtRuleNameSpaceAfterCheck {
  /// The primary option: `"always"` or `"always-single-line"`.
  option: &'static str,
}

impl StylisticAtRuleNameSpaceAfterCheck {
  /// The plugin's options: `"always"` or `"always-single-line"`.  Gale reads no
  /// option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "always-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticAtRuleNameSpaceAfterCheck {
  /// Flags an at-rule name whose space before its prelude does not match the option.
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

      if bytes[i] == b'@' {
        let at_pos = i;
        i += 1;

        // Read the at-rule name
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
          i += 1;
        }
        let name_end = i;

        if name_start == name_end {
          continue;
        }

        // After the name, check for a space
        if i >= len {
          continue;
        }

        // Some at-rules have no prelude (like @charset, etc.)
        // Check what follows the name
        let char_after = bytes[i];

        // Skip at-rules that go directly to semicolon or opening brace
        if char_after == b';' || char_after == b'{' {
          continue;
        }

        let has_space = char_after == b' ';
        let has_newline = char_after == b'\n' || char_after == b'\r';

        // Determine if the at-rule is single-line
        let is_single_line = if option == "always-single-line" {
          // Find the end of the at-rule (semicolon or opening brace)
          let mut k = i;
          let mut single = true;
          while k < len && bytes[k] != b';' && bytes[k] != b'{' {
            if bytes[k] == b'\n' {
              single = false;
              break;
            }
            k += 1;
          }
          single
        } else {
          true
        };

        match option {
          "always" => {
            if !has_space && !has_newline {
              diagnostics.push(
                Diagnostic::new(
                  NAME,
                  format!(
                    "Expected a space after at-rule name \"@{}\"",
                    &source[name_start..name_end]
                  ),
                )
                .span(Span::new(at_pos, name_end - at_pos)),
              );
            }
          }
          "always-single-line" => {
            if is_single_line && !has_space && !has_newline {
              diagnostics.push(
                Diagnostic::new(
                  NAME,
                  format!(
                    "Expected a space after at-rule name \"@{}\"",
                    &source[name_start..name_end]
                  ),
                )
                .span(Span::new(at_pos, name_end - at_pos)),
              );
            }
          }
          _ => {}
        }
      } else {
        i += 1;
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, ctx_with_source_and_options, invalid_options};

  #[test]
  fn allows_space_after_at_rule_name() {
    let rule = configured(&StylisticAtRuleNameSpaceAfter, None);
    let source = "@media screen { }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_space_after_at_rule_name() {
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticAtRuleNameSpaceAfter, opt.clone());
    let source = "@media(min-width: 100px) { }";
    let d = rule.check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn allows_charset() {
    let rule = configured(&StylisticAtRuleNameSpaceAfter, None);
    let source = "@charset \"UTF-8\";";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("never")),
      ["Invalid option value \"never\" for rule \"@stylistic/at-rule-name-space-after\""]
    );
  }
}
