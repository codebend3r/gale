use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-max-empty-lines";

declare_rule! {
  /// Limit the number of adjacent empty lines within selectors.
  ///
  /// Equivalent to `@stylistic/selector-max-empty-lines`.
  pub struct StylisticSelectorMaxEmptyLines => StylisticSelectorMaxEmptyLinesCheck {
    name: NAME,
    description: "Limit the number of adjacent empty lines within selectors",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-max-empty-lines` as configured.
pub struct StylisticSelectorMaxEmptyLinesCheck {
  /// The most empty lines allowed in a row.
  max: usize,
}

impl StylisticSelectorMaxEmptyLinesCheck {
  /// The plugin's options: a number (`possible: isNumber`).  Gale reads no
  /// number as 0, and a number that is not a whole number of zero or more as 0
  /// too, as it always has.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(serde_json::Value::is_number);
    options.finish()?;
    Ok(Self {
      max: max.and_then(serde_json::Value::as_u64).unwrap_or(0) as usize,
    })
  }
}

impl Check for StylisticSelectorMaxEmptyLinesCheck {
  /// Flags runs of blank lines inside a selector longer than the maximum.
  ///
  /// Reads the selector of every rule as written, as the plugin does.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;
    let mut diags = Vec::new();
    for rule in tree.walk_rules() {
      // Check for consecutive empty lines within the selector string.
      let selector = rule.raw_selector();
      let mut consecutive_empty = 0usize;
      let mut max_found = 0usize;

      for line in selector.lines() {
        if line.trim().is_empty() {
          consecutive_empty += 1;
          if consecutive_empty > max_found {
            max_found = consecutive_empty;
          }
        } else {
          consecutive_empty = 0;
        }
      }

      if max_found > max {
        diags.push(
          Diagnostic::new(NAME, format!("Expected no more than {max} empty line(s)"))
            .span(rule.span()),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// What the rule allowing `max` empty lines reports for a CSS rule with
  /// the selector `selector`.
  fn check_selector(selector: &str, max: u64) -> Vec<Diagnostic> {
    let rule = configured(&StylisticSelectorMaxEmptyLines, serde_json::json!(max));
    rule.check_source(&context(
      &format!("{selector} {{ color: red; }}"),
      Syntax::Css,
    ))
  }

  #[test]
  fn allows_no_empty_lines_in_selector() {
    let d = check_selector("a,\nb", 0);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_empty_line_in_selector_when_max_zero() {
    let d = check_selector("a,\n\nb", 0);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 0"));
  }

  #[test]
  fn allows_one_empty_line_when_max_one() {
    let d = check_selector("a,\n\nb", 1);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_two_empty_lines_when_max_one() {
    let d = check_selector("a,\n\n\nb", 1);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(0)).is_empty());
    assert!(invalid_options(NAME, serde_json::json!(2)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("2")),
      ["Invalid option \"2\" for rule \"@stylistic/selector-max-empty-lines\""]
    );
  }
}
