use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-pseudo-element-case";

declare_rule! {
  /// Specify lowercase or uppercase for pseudo-element selectors.
  ///
  /// Equivalent to Stylelint's `@stylistic/selector-pseudo-element-case` rule.
  pub struct StylisticSelectorPseudoElementCase => StylisticSelectorPseudoElementCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for pseudo-element selectors",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-pseudo-element-case` as configured.
pub struct StylisticSelectorPseudoElementCaseCheck {
  /// The primary option: `"lower"` or `"upper"`.
  option: &'static str,
}

impl StylisticSelectorPseudoElementCaseCheck {
  /// The plugin's options: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["lower", "upper"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("lower"),
    })
  }
}

impl Check for StylisticSelectorPseudoElementCaseCheck {
  /// Flags pseudo-element names not in the configured case.
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

      // Detect :: pseudo-elements
      if i + 1 < len && bytes[i] == b':' && bytes[i + 1] == b':' {
        let pseudo_start = i;
        i += 2; // skip ::

        // Read the pseudo-element name
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
          i += 1;
        }
        let name_end = i;

        if name_start == name_end {
          continue;
        }

        let name = &source[name_start..name_end];

        let is_wrong_case = match option {
          "lower" => name.chars().any(|c| c.is_ascii_uppercase()),
          "upper" => name.chars().any(|c| c.is_ascii_lowercase()),
          _ => false,
        };

        if is_wrong_case {
          let expected = match option {
            "lower" => name.to_ascii_lowercase(),
            "upper" => name.to_ascii_uppercase(),
            _ => name.to_string(),
          };
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Expected \"::{}\" to be \"::{}\"", name, expected),
            )
            .span(Span::new(pseudo_start, name_end - pseudo_start)),
          );
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

  #[test]
  fn allows_lowercase_pseudo_element() {
    let opt = serde_json::Value::String("lower".to_string());
    let source = "a::before { }";
    let d = configured(&StylisticSelectorPseudoElementCase, opt.clone())
      .check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_uppercase_pseudo_element() {
    let opt = serde_json::Value::String("lower".to_string());
    let source = "a::Before { }";
    let d = configured(&StylisticSelectorPseudoElementCase, opt.clone())
      .check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("before"));
  }

  #[test]
  fn allows_uppercase_pseudo_element_with_upper() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a::AFTER { }";
    let d = configured(&StylisticSelectorPseudoElementCase, opt.clone())
      .check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_lowercase_when_upper_expected() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a::after { }";
    let d = configured(&StylisticSelectorPseudoElementCase, opt.clone())
      .check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("AFTER"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("lower")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/selector-pseudo-element-case\""]
    );
  }
}
