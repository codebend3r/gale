use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-if-no-null";

declare_rule! {
  /// Disallow `@if` conditions that compare to `null`.
  ///
  /// Users should use `not $variable` instead of `$variable != null`,
  /// and `$variable` instead of `$variable == null`.
  pub struct ScssAtIfNoNull => ScssAtIfNoNullCheck {
    name: NAME,
    description: "Disallow null comparisons in @if conditions",
    default_severity: Warning,
  }
}

/// `scss/at-if-no-null`, which takes no options.
pub struct ScssAtIfNoNullCheck;

impl ScssAtIfNoNullCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssAtIfNoNullCheck {
  /// Flags an `@if` condition comparing against `null` — `not $x` and `$x` say
  /// the same thing more directly.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "if" {
      return vec![];
    }

    let params = at.params.to_ascii_lowercase();

    // Stylelint's scss/at-if-no-null skips `!= null and ...` patterns —
    // these are compound conditions where the null check is part of a
    // larger boolean expression and removing it would change semantics.
    if params.contains("!= null and ") {
      return vec![];
    }

    if params.contains("== null") || params.contains("!= null") {
      vec![Diagnostic::new(NAME, "Unexpected null comparison in @if condition").span(at.span)]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::AtRule;
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn if_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "if".to_string(),
      params: params.to_string(),
      span: Span::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_equals_null() {
    let rule = configured(&ScssAtIfNoNull, None);
    let d = rule.check(&if_rule("$var == null"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_not_equals_null() {
    let rule = configured(&ScssAtIfNoNull, None);
    let d = rule.check(&if_rule("$var != null"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_non_null_comparison() {
    let rule = configured(&ScssAtIfNoNull, None);
    let d = rule.check(&if_rule("$var"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_not_variable() {
    let rule = configured(&ScssAtIfNoNull, None);
    let d = rule.check(&if_rule("not $var"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_not_equals_null_and_compound() {
    let rule = configured(&ScssAtIfNoNull, None);
    // `!= null and <expr>` is a compound condition — Stylelint skips these.
    let d = rule.check(&if_rule("$var != null and $var != ''"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtIfNoNull, None);
    let ctx = ctx();
    assert!(rule.check(&if_rule("$var == null"), &ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-if-no-null\""]
    );
  }
}
