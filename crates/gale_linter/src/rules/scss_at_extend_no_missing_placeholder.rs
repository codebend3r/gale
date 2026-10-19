use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-extend-no-missing-placeholder";

declare_rule! {
  /// Disallow `@extend` without a `%placeholder` selector.
  pub struct ScssAtExtendNoMissingPlaceholder => ScssAtExtendNoMissingPlaceholderCheck {
    name: NAME,
    description: "Disallow @extend without a %placeholder selector",
    default_severity: Warning,
  }
}

/// `scss/at-extend-no-missing-placeholder`, which takes no options.
pub struct ScssAtExtendNoMissingPlaceholderCheck;

impl ScssAtExtendNoMissingPlaceholderCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssAtExtendNoMissingPlaceholderCheck {
  /// Flags an `@extend` whose target is not a `%placeholder` selector.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "extend" {
      return vec![];
    }

    let params = at.params.trim();
    if !params.starts_with('%') {
      // Point to the selector, not the `@extend` keyword.
      let at_src_end = (at.span.offset + at.span.length).min(ctx.source.len());
      let at_src = &ctx.source[at.span.offset..at_src_end];
      let sel_off = at_src.find(params).unwrap_or(0);
      vec![
        Diagnostic::new(
          NAME,
          "Expected a placeholder selector (e.g. %placeholder) to be used in @extend".to_string(),
        )
        .span(Span::new(at.span.offset + sel_off, params.len())),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::AtRule;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn extend(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "extend".to_string(),
      params: params.to_string(),
      span: Span::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_non_placeholder() {
    let rule = configured(&ScssAtExtendNoMissingPlaceholder, None);
    let d = rule.check(&extend(".foo"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("placeholder"));
  }

  #[test]
  fn allows_placeholder() {
    let rule = configured(&ScssAtExtendNoMissingPlaceholder, None);
    let d = rule.check(&extend("%placeholder"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_extend() {
    let rule = configured(&ScssAtExtendNoMissingPlaceholder, None);
    let node = CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "foo".to_string(),
      span: Span::new(0, 10),
      children: vec![],
    });
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtExtendNoMissingPlaceholder, None);
    let css_ctx = ctx();
    assert!(rule.check(&extend(".foo"), &css_ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-extend-no-missing-placeholder\""]
    );
  }
}
