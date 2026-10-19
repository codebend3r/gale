use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/at-rule-name-case";

declare_rule! {
  /// Specify lowercase or uppercase for at-rule names.
  ///
  /// Equivalent to `@stylistic/at-rule-name-case`.
  pub struct StylisticAtRuleNameCase => StylisticAtRuleNameCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for at-rule names",
    default_severity: Warning,
  }
}

/// `@stylistic/at-rule-name-case` as configured.
pub struct StylisticAtRuleNameCaseCheck {
  /// The primary option: `"lower"` or `"upper"`.
  option: &'static str,
}

impl StylisticAtRuleNameCaseCheck {
  /// The plugin's options: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.  The plugin takes `"upper"` only where the syntax reads an
  /// upper-case at-rule name, which gale cannot know until it lints a file, so it
  /// accepts it everywhere.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["lower", "upper"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("lower"),
    })
  }
}

impl Check for StylisticAtRuleNameCaseCheck {
  /// Flags at-rule names not in the configured case.
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

        let name = &source[name_start..name_end];

        let is_wrong = match option {
          "lower" => name.chars().any(|c| c.is_ascii_uppercase()),
          "upper" => name.chars().any(|c| c.is_ascii_lowercase()),
          _ => false,
        };

        if is_wrong {
          let expected = if option == "lower" {
            "lowercase"
          } else {
            "uppercase"
          };
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Expected at-rule name \"@{}\" to be {}", name, expected),
            )
            .span(Span::new(at_pos, name_end - at_pos)),
          );
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
  fn allows_lowercase_at_rule_names() {
    let rule = configured(&StylisticAtRuleNameCase, None);
    let source = "@media screen { } @keyframes foo { }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_uppercase_at_rule_name_with_lower_option() {
    let opt = serde_json::Value::String("lower".to_string());
    let rule = configured(&StylisticAtRuleNameCase, opt.clone());
    let source = "@Media screen { }";
    let d = rule.check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("lowercase"));
  }

  #[test]
  fn allows_uppercase_with_upper_option() {
    let opt = serde_json::Value::String("upper".to_string());
    let rule = configured(&StylisticAtRuleNameCase, opt.clone());
    let source = "@MEDIA screen { }";
    let d = rule.check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_lowercase_at_rule_name_with_upper_option() {
    let opt = serde_json::Value::String("upper".to_string());
    let rule = configured(&StylisticAtRuleNameCase, opt.clone());
    let source = "@media screen { }";
    let d = rule.check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("uppercase"));
  }

  #[test]
  fn skips_comments() {
    let rule = configured(&StylisticAtRuleNameCase, None);
    let source = "/* @Media */ @media screen { }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The rule validates no secondary options, so it ignores them.
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("camel")),
      ["Invalid option value \"camel\" for rule \"@stylistic/at-rule-name-case\""]
    );
    assert!(invalid_options(NAME, serde_json::json!(["lower", { "ignore": ["x"] }])).is_empty());
  }
}
