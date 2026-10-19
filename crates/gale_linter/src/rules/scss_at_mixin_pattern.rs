use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

const NAME: &str = "scss/at-mixin-pattern";

declare_rule! {
  /// Specify a pattern for SCSS `@mixin` names.
  ///
  /// Accepts a regex string as the primary option. Defaults to kebab-case.
  ///
  /// Equivalent to `scss/at-mixin-pattern`.
  pub struct ScssAtMixinPattern => ScssAtMixinPatternCheck {
    name: NAME,
    description: "Specify a pattern for SCSS mixin names",
    default_severity: Warning,
  }
}

/// `scss/at-mixin-pattern` as configured.
pub struct ScssAtMixinPatternCheck {
  /// The pattern every name must match.
  pattern: Pattern,
}

impl ScssAtMixinPatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option, as Stylelint reports it.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.finish()?;
    let pattern = match pattern {
      Some(pattern) => pattern,
      None => Pattern::compile(DEFAULT_PATTERN).expect("the default pattern compiles"),
    };
    Ok(Self { pattern })
  }
}

impl Check for ScssAtMixinPatternCheck {
  /// Flags a `@mixin` name that does not match the configured regex, defaulting
  /// to kebab-case.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "mixin" {
      return vec![];
    }

    let pattern_str = self.pattern.source.as_str();

    // The mixin name is the first word in params (before `(` or whitespace)
    let first_token = at.params.split_whitespace().next().unwrap_or("");
    let mixin_name = first_token.split('(').next().unwrap_or("");

    if mixin_name.is_empty() {
      return vec![];
    }

    if !self.pattern.is_match(mixin_name) {
      vec![
        Diagnostic::new(
          NAME,
          format!(
            "Expected @mixin name \"{}\" to match pattern \"{}\"",
            mixin_name, pattern_str
          ),
        )
        .span(at.span),
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
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn mixin(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: params.to_string(),
      span: Span::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn allows_kebab_case() {
    let rule = configured(&ScssAtMixinPattern, None);
    assert!(rule.check(&mixin("my-mixin($a)"), &scss_ctx()).is_empty());
  }

  #[test]
  fn reports_non_matching_pattern() {
    let rule = configured(&ScssAtMixinPattern, None);
    let d = rule.check(&mixin("MyMixin($a)"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MyMixin"));
  }

  #[test]
  fn skips_non_mixin() {
    let rule = configured(&ScssAtMixinPattern, None);
    let node = CssNode::AtRule(AtRule {
      name: "function".to_string(),
      params: "MyFunc".to_string(),
      span: Span::new(0, 10),
      children: vec![],
    });
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtMixinPattern, None);
    let css_ctx = ctx();
    assert!(rule.check(&mixin("MyMixin"), &css_ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("^[a-z]+$")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("/^[a-z]+$/i")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"scss/at-mixin-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-z\" for rule \"scss/at-mixin-pattern\": "),
      "{broken:?}"
    );
  }
}
