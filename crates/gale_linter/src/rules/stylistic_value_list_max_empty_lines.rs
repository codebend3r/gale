use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/value-list-max-empty-lines";

declare_rule! {
  /// Limit the number of adjacent empty lines within value lists.
  ///
  /// Primary option: integer (default 0).
  pub struct StylisticValueListMaxEmptyLines => StylisticValueListMaxEmptyLinesCheck {
    name: NAME,
    description: "Limit the number of adjacent empty lines within value lists",
    default_severity: Warning,
  }
}

/// `@stylistic/value-list-max-empty-lines` as configured.
pub struct StylisticValueListMaxEmptyLinesCheck {
  /// The most empty lines allowed in a row.
  max: usize,
}

impl StylisticValueListMaxEmptyLinesCheck {
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

impl Check for StylisticValueListMaxEmptyLinesCheck {
  /// Flags runs of blank lines inside a declaration value longer than the maximum.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diagnostics = Vec::new();

    for decl in &rule.declarations {
      let value = &decl.value;
      let mut consecutive_empty = 0usize;
      let mut max_found = 0usize;

      for line in value.lines() {
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
        diagnostics.push(
          Diagnostic::new(
            NAME,
            format!("Expected no more than {max} empty line(s) in value list"),
          )
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  fn check_value(value: &str, max: u64) -> Vec<Diagnostic> {
    let opts = serde_json::json!(max);
    let rule = configured(&StylisticValueListMaxEmptyLines, opts.clone());
    let ctx = ctx();
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: value.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    });
    rule.check(&node, &ctx)
  }

  #[test]
  fn allows_no_empty_lines_in_value() {
    let d = check_value("Arial,\nsans-serif", 0);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_empty_line_in_value_when_max_zero() {
    let d = check_value("Arial,\n\nsans-serif", 0);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 0"));
  }

  #[test]
  fn allows_one_empty_line_when_max_one() {
    let d = check_value("Arial,\n\nsans-serif", 1);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_two_empty_lines_when_max_one() {
    let d = check_value("Arial,\n\n\nsans-serif", 1);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(0)).is_empty());
    assert!(invalid_options(NAME, serde_json::json!(2)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("2")),
      ["Invalid option \"2\" for rule \"@stylistic/value-list-max-empty-lines\""]
    );
  }
}
