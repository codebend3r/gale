use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::chars::is_word_or_hyphen_byte;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, LetterCase, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-pseudo-class-case";

declare_rule! {
  /// Specify lowercase or uppercase for pseudo-class selectors.
  ///
  /// Equivalent to `@stylistic/selector-pseudo-class-case`.
  pub struct StylisticSelectorPseudoClassCase => StylisticSelectorPseudoClassCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for pseudo-class selectors",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-pseudo-class-case` as configured.
pub struct StylisticSelectorPseudoClassCaseCheck {
  /// The case pseudo-class names are written in.
  case: LetterCase,
}

impl StylisticSelectorPseudoClassCaseCheck {
  /// The plugin's options: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let case = options.primary_case();
    options.finish()?;
    Ok(Self { case })
  }
}

impl Check for StylisticSelectorPseudoClassCaseCheck {
  /// Flags pseudo-class names not in the configured case.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let case = self.case;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
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

      if bytes[i] == b'}' || bytes[i] == b';' {
        in_selector = true;
        i += 1;
        continue;
      }

      // Detect pseudo-class (single colon, not double colon for pseudo-elements)
      if bytes[i] == b':' && in_selector {
        // Check for double colon (pseudo-element) -- skip those
        if i + 1 < len && bytes[i + 1] == b':' {
          i += 2;
          // Skip past pseudo-element name
          while i < len && is_word_or_hyphen_byte(bytes[i]) {
            i += 1;
          }
          continue;
        }

        let colon_pos = i;
        i += 1;

        // Read the pseudo-class name
        let name_start = i;
        while i < len && is_word_or_hyphen_byte(bytes[i]) {
          i += 1;
        }
        let name_end = i;

        if name_start == name_end {
          continue;
        }

        let name = &source[name_start..name_end];

        if case.is_wrong(name) {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Expected pseudo-class \":{}\" to be {}", name, case.word()),
            )
            .span(Span::new(colon_pos, name_end - colon_pos)),
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
  fn allows_lowercase_pseudo_class() {
    let source = "a:hover { }";
    let d =
      configured(&StylisticSelectorPseudoClassCase, None).check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_uppercase_pseudo_class() {
    let source = "a:Hover { }";
    let d =
      configured(&StylisticSelectorPseudoClassCase, None).check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("lowercase"));
  }

  #[test]
  fn allows_uppercase_with_upper_option() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a:HOVER { }";
    let d = configured(&StylisticSelectorPseudoClassCase, opt.clone())
      .check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_lowercase_with_upper_option() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a:hover { }";
    let d = configured(&StylisticSelectorPseudoClassCase, opt.clone())
      .check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("uppercase"));
  }

  #[test]
  fn does_not_flag_pseudo_elements() {
    let source = "a::Before { }";
    let d =
      configured(&StylisticSelectorPseudoClassCase, None).check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_comments() {
    let source = "/* a:Hover {} */ a:hover { }";
    let d =
      configured(&StylisticSelectorPseudoClassCase, None).check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("lower")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/selector-pseudo-class-case\""]
    );
  }
}
