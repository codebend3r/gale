use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-mixin-argumentless-call-parentheses";

declare_rule! {
  /// Require or disallow parentheses in argumentless `@include` mixin calls.
  ///
  /// By default expects `"always"` (parentheses are required):
  ///
  /// ```scss
  /// // Good (always)
  /// @include mixin-name();
  ///
  /// // Bad (always)
  /// @include mixin-name;
  /// ```
  ///
  /// Equivalent to `scss/at-mixin-argumentless-call-parentheses`.
  pub struct ScssAtMixinArgumentlessCallParentheses => ScssAtMixinArgumentlessCallParenthesesCheck {
    name: NAME,
    description: "Require or disallow parentheses in argumentless @include calls",
    default_severity: Warning,
  }
}

/// `scss/at-mixin-argumentless-call-parentheses` as configured.
pub struct ScssAtMixinArgumentlessCallParenthesesCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssAtMixinArgumentlessCallParenthesesCheck {
  /// Stylelint's options: `"always"` or `"never"`, `"always"` when not given.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always", "never"])
      .unwrap_or("always");
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssAtMixinArgumentlessCallParenthesesCheck {
  /// Flags an argumentless `@include` whose parentheses do not match the option.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "include" {
      return vec![];
    }

    let option = self.expectation;

    let params = at.params.trim();
    // An include with arguments (contains `(` with non-empty content before `)`) is not
    // "argumentless" — skip it.
    let has_args = if let Some(paren_pos) = params.find('(') {
      let after_paren = &params[paren_pos + 1..];
      let inner = after_paren.trim_end_matches(')').trim();
      !inner.is_empty()
    } else {
      false
    };

    if has_args {
      return vec![];
    }

    let has_parens = params.contains('(');

    match option {
      "always" => {
        if !has_parens {
          vec![
            Diagnostic::new(
              NAME,
              format!(
                "Expected parentheses in argumentless @include \"{}\"",
                params
              ),
            )
            .span(at.span),
          ]
        } else {
          vec![]
        }
      }
      "never" => {
        if has_parens {
          let mixin_name = params.trim_end_matches("()");
          // Point to the mixin name, not the `@include` keyword.
          let at_src_end = (at.span.offset + at.span.length).min(ctx.source.len());
          let at_src = &ctx.source[at.span.offset..at_src_end];
          let name_off = at_src.find(mixin_name).unwrap_or(0);
          vec![
            Diagnostic::new(
              NAME,
              format!(
                "Unexpected parentheses in argumentless mixin \"{}\" call",
                mixin_name
              ),
            )
            .span(Span::new(at.span.offset + name_off, mixin_name.len())),
          ]
        } else {
          vec![]
        }
      }
      _ => vec![],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::AtRule;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn include(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "include".to_string(),
      params: params.to_string(),
      span: Span::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_missing_parens_always() {
    let rule = configured(&ScssAtMixinArgumentlessCallParentheses, None);
    let d = rule.check(&include("mixin-name"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("parentheses"));
  }

  #[test]
  fn allows_parens_always() {
    let rule = configured(&ScssAtMixinArgumentlessCallParentheses, None);
    let d = rule.check(&include("mixin-name()"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_include_with_args() {
    let rule = configured(&ScssAtMixinArgumentlessCallParentheses, None);
    let d = rule.check(&include("mixin-name($arg)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtMixinArgumentlessCallParentheses, None);
    let css_ctx = ctx();
    assert!(rule.check(&include("mixin-name"), &css_ctx).is_empty());
  }

  #[test]
  fn skips_non_include() {
    let rule = configured(&ScssAtMixinArgumentlessCallParentheses, None);
    let node = CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "foo".to_string(),
      span: Span::new(0, 10),
      children: vec![],
    });
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      [
        "Invalid option value \"sideways\" for rule \"scss/at-mixin-argumentless-call-parentheses\""
      ]
    );
  }
}
