use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/at-rule-semicolon-space-before";

declare_rule! {
  /// Require or disallow a space before at-rule semicolons.
  ///
  /// Equivalent to `@stylistic/at-rule-semicolon-space-before`.
  pub struct StylisticAtRuleSemicolonSpaceBefore => StylisticAtRuleSemicolonSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before at-rule semicolons",
    default_severity: Warning,
  }
}

/// `@stylistic/at-rule-semicolon-space-before` as configured.
pub struct StylisticAtRuleSemicolonSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticAtRuleSemicolonSpaceBeforeCheck {
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

impl Check for StylisticAtRuleSemicolonSpaceBeforeCheck {
  /// Flags an at-rule semicolon whose preceding space does not match the option.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut in_at_rule = false;
    let mut brace_depth: usize = 0;

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

      if bytes[i] == b'@' {
        in_at_rule = true;
        i += 1;
        continue;
      }

      if bytes[i] == b'{' {
        brace_depth += 1;
        in_at_rule = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' {
        brace_depth = brace_depth.saturating_sub(1);
        i += 1;
        continue;
      }

      // Check semicolons that terminate at-rules (not inside rule blocks)
      if bytes[i] == b';' && in_at_rule {
        let semi_pos = i;
        let has_space_before = semi_pos > 0 && bytes[semi_pos - 1] == b' ';

        match option {
          "always" => {
            if !has_space_before {
              diagnostics.push(
                Diagnostic::new(NAME, "Expected a space before \";\"".to_string())
                  .span(Span::new(semi_pos, 1)),
              );
            }
          }
          "never" => {
            if has_space_before {
              diagnostics.push(
                Diagnostic::new(NAME, "Unexpected space before \";\"".to_string())
                  .span(Span::new(semi_pos - 1, 1)),
              );
            }
          }
          _ => {}
        }
        in_at_rule = false;
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
  fn never_allows_no_space_before_semicolon() {
    let opt = serde_json::Value::String("never".to_string());
    let rule = configured(&StylisticAtRuleSemicolonSpaceBefore, opt.clone());
    let source = "@import url(foo.css);";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space_before_semicolon() {
    let rule = configured(&StylisticAtRuleSemicolonSpaceBefore, None);
    let source = "@import url(foo.css) ;";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_allows_space_before_semicolon() {
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticAtRuleSemicolonSpaceBefore, opt.clone());
    let source = "@import url(foo.css) ;";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_missing_space_before_semicolon() {
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticAtRuleSemicolonSpaceBefore, opt.clone());
    let source = "@import url(foo.css);";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn skips_comments() {
    let rule = configured(&StylisticAtRuleSemicolonSpaceBefore, None);
    let source = "/* @import url(foo.css) ; */ @import url(bar.css);";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always-single-line")),
      [
        "Invalid option value \"always-single-line\" for rule \"@stylistic/at-rule-semicolon-space-before\""
      ]
    );
  }
}
