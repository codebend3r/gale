use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

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
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if at.name() != "include" {
        continue;
      }
      let params = at.params().trim();
      // An include with arguments (contains `(` with non-empty content
      // before `)`) is not "argumentless" — skip it.
      let has_args = params.find('(').is_some_and(|paren_pos| {
        let after_paren = &params[paren_pos + 1..];
        !after_paren.trim_end_matches(')').trim().is_empty()
      });
      if has_args {
        continue;
      }
      let has_parens = params.contains('(');
      match self.expectation {
        // Point to the mixin name, as stylelint-scss does.
        "always" if !has_parens => diags.push(
          Diagnostic::new(
            NAME,
            format!("Expected parentheses in argumentless @include \"{params}\""),
          )
          .span(at.word_span(params)),
        ),
        "never" if has_parens => {
          let mixin_name = params.trim_end_matches("()");
          // Point to the mixin name, not the `@include` keyword.
          diags.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected parentheses in argumentless mixin \"{mixin_name}\" call"),
            )
            .span(at.word_span(mixin_name)),
          );
        }
        _ => {}
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{Value, json};

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The reports for `a { @include <params>; }` in `syntax`.
  fn include(options: Option<Value>, params: &str, syntax: Syntax) -> Vec<Diagnostic> {
    let source = format!("a {{ @include {params}; }}");
    configured(&ScssAtMixinArgumentlessCallParentheses, options)
      .check_source(&context(&source, syntax))
  }

  #[test]
  fn reports_missing_parens_always() {
    let d = include(None, "mixin-name", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("parentheses"));
    assert_eq!((d[0].span.offset, d[0].span.length), (13, 10));
  }

  #[test]
  fn allows_parens_always() {
    assert!(include(None, "mixin-name()", Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_parens_never_at_the_name() {
    let d = include(Some(json!("never")), "mixin-name()", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].span.offset, d[0].span.length), (13, 10));
    assert!(include(Some(json!("never")), "mixin-name", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_include_with_args() {
    assert!(include(None, "mixin-name($arg)", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(include(None, "mixin-name", Syntax::Css).is_empty());
  }

  #[test]
  fn skips_non_include() {
    let rule = configured(&ScssAtMixinArgumentlessCallParentheses, None);
    assert!(
      rule
        .check_source(&context("@mixin foo { }", Syntax::Scss))
        .is_empty()
    );
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
