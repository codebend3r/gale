use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-rule-conditional-no-parentheses";

declare_rule! {
  /// Disallow parentheses in conditional `@` rules (`@if`, `@elsif`, `@else if`, `@while`).
  ///
  /// ```scss
  /// // Bad
  /// @if ($condition) { }
  /// @elsif ($condition) { }
  /// @while ($condition) { }
  ///
  /// // Good
  /// @if $condition { }
  /// @elsif $condition { }
  /// @while $condition { }
  /// ```
  ///
  /// Equivalent to `scss/at-rule-conditional-no-parentheses`.
  pub struct ScssAtRuleConditionalNoParentheses => ScssAtRuleConditionalNoParenthesesCheck {
    name: NAME,
    description: "Disallow parentheses in conditional @ rules (if, elsif, while)",
    default_severity: Warning,
  }
}

/// `scss/at-rule-conditional-no-parentheses`, which takes no options.
pub struct ScssAtRuleConditionalNoParenthesesCheck;

impl ScssAtRuleConditionalNoParenthesesCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssAtRuleConditionalNoParenthesesCheck {
  /// Flags parentheses wrapping the condition of `@if`, `@elsif` or `@while`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    // Check if this is a conditional at-rule
    let name_lower = at.name.to_ascii_lowercase();
    if name_lower != "if"
      && name_lower != "elsif"
      && name_lower != "else if"
      && name_lower != "while"
    {
      return vec![];
    }

    let params = at.params.trim();

    // Check if the entire parameter is wrapped in parentheses
    if is_wrapped_in_parens(params) {
      // Stylelint points to the `(` of the condition, not the `@if` keyword.
      let at_src_end = (at.span.offset + at.span.length).min(ctx.source.len());
      let at_src = ctx.source_slice(at.span.offset, at_src_end).unwrap_or("");
      let paren_off = at_src.find('(').unwrap_or(0);
      vec![
        Diagnostic::new(
          NAME,
          "Unexpected () used to surround statements for @-rules".to_string(),
        )
        .span(Span::new(at.span.offset + paren_off, 1)),
      ]
    } else {
      vec![]
    }
  }
}

/// Check if the string is wrapped in matching outer parentheses.
/// e.g., `($condition)` -> true, `($a) and ($b)` -> false, `$condition` -> false
fn is_wrapped_in_parens(s: &str) -> bool {
  let bytes = s.as_bytes();
  if bytes.is_empty() || bytes[0] != b'(' || bytes[bytes.len() - 1] != b')' {
    return false;
  }

  // Check that the opening paren at index 0 matches the closing paren
  // at the end, not some intermediate one.
  let mut depth = 0;
  for (i, &b) in bytes.iter().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 && i != bytes.len() - 1 {
          return false;
        }
      }
      _ => {}
    }
  }
  depth == 0
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::AtRule;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn at_node(name: &str, params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: params.to_string(),
      span: Span::new(0, 20),
      children: vec![],
    })
  }

  #[test]
  fn reports_parenthesized_if() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("if", "($condition)"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected ()"));
  }

  #[test]
  fn reports_parenthesized_elsif() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("elsif", "($x > 1)"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_parenthesized_while() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("while", "($i > 0)"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_no_parens_if() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("if", "$condition"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_parens_elsif() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("elsif", "$x > 1"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_parens_while() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("while", "$i > 0"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_inner_parens_not_wrapping() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    // `($a) and ($b)` — not wrapped in a single outer paren pair
    let d = rule.check(&at_node("if", "($a) and ($b)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_function_call_in_condition() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("if", "length($list) > 0"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_conditional_at_rules() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let d = rule.check(&at_node("media", "(min-width: 768px)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtRuleConditionalNoParentheses, None);
    let ctx = ctx();
    let d = rule.check(&at_node("if", "($x)"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-rule-conditional-no-parentheses\""]
    );
  }
}
