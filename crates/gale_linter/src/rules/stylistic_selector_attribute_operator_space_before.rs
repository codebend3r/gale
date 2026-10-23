use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-attribute-operator-space-before";

declare_rule! {
  /// Require or disallow a space before the operator within attribute selectors.
  ///
  /// Equivalent to `@stylistic/selector-attribute-operator-space-before`.
  pub struct StylisticSelectorAttributeOperatorSpaceBefore => StylisticSelectorAttributeOperatorSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the operator within attribute selectors",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-attribute-operator-space-before` as configured.
pub struct StylisticSelectorAttributeOperatorSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticSelectorAttributeOperatorSpaceBeforeCheck {
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

/// CSS attribute selector operators.
const ATTR_OPERATORS: &[&str] = &["~=", "|=", "^=", "$=", "*=", "="];

impl Check for StylisticSelectorAttributeOperatorSpaceBeforeCheck {
  /// Flags an attribute operator whose preceding space does not match the option.
  ///
  /// Reads the selector of every standard-syntax rule as written, as the
  /// plugin does.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      let selector = rule.raw_selector();
      let sel_offset = rule.selector_start();

      // Find attribute selectors [...]
      let bytes = selector.as_bytes();
      let len = bytes.len();
      let mut i = 0;

      while i < len {
        if bytes[i] == b'[' {
          let bracket_start = i;
          // Find the closing bracket
          let mut depth = 1;
          i += 1;
          while i < len && depth > 0 {
            if bytes[i] == b'[' {
              depth += 1;
            } else if bytes[i] == b']' {
              depth -= 1;
            }
            if depth > 0 {
              i += 1;
            }
          }
          let bracket_end = i;
          if bracket_end < len {
            let attr_content = &selector[bracket_start + 1..bracket_end];
            check_attr_operator(
              attr_content,
              sel_offset + bracket_start + 1,
              option,
              &mut diags,
            );
          }
        }
        i += 1;
      }
    }

    diags
  }
}

/// Checks the operator inside one `[…]` block, offsets relative to `base_offset`.
fn check_attr_operator(
  attr_content: &str,
  base_offset: usize,
  option: &str,
  diags: &mut Vec<Diagnostic>,
) {
  for op in ATTR_OPERATORS {
    if let Some(pos) = attr_content.find(op) {
      if pos == 0 {
        continue;
      }
      let char_before = attr_content.as_bytes()[pos - 1];
      let has_space = char_before == b' ' || char_before == b'\t';

      let violation = match option {
        "always" => !has_space,
        "never" => has_space,
        _ => false,
      };

      if violation {
        let msg = match option {
          "always" => format!("Expected single space before \"{op}\""),
          "never" => format!("Unexpected whitespace before \"{op}\""),
          _ => return,
        };
        diags.push(Diagnostic::new(NAME, msg).span(Span::new(base_offset + pos, op.len())));
      }
      return; // Only check the first operator found
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_space_before_operator_when_never() {
    let rule = configured(&StylisticSelectorAttributeOperatorSpaceBefore, None);
    let d = rule.check_source(&context(&style_with_selector("[attr =value]"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn allows_no_space_before_operator_when_never() {
    let rule = configured(&StylisticSelectorAttributeOperatorSpaceBefore, None);
    let d = rule.check_source(&context(&style_with_selector("[attr=value]"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn no_report_for_simple_attribute() {
    let rule = configured(&StylisticSelectorAttributeOperatorSpaceBefore, None);
    let d = rule.check_source(&context(&style_with_selector("[disabled]"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn reads_the_selector_as_written() {
    let never = configured(&StylisticSelectorAttributeOperatorSpaceBefore, None);
    let source = "a,\n  b[x  ^=\"y\"] {}";
    let d = never.check_source(&context(source, Syntax::Css));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span.offset, source.find("^=").unwrap());
    let always = configured(
      &StylisticSelectorAttributeOperatorSpaceBefore,
      serde_json::json!("always"),
    );
    assert!(
      always
        .check_source(&context(source, Syntax::Css))
        .is_empty()
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/selector-attribute-operator-space-before\""
      ]
    );
  }
}
