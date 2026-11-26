use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::{ChainHead, ChainReading, scss_chain_closes};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-else-closing-brace-space-after";

declare_rule! {
  /// Require or disallow a space after the closing brace of `@else` when
  /// followed by `@else if`.
  ///
  /// Default: `"always-intermediate"`.
  ///
  /// Equivalent to `scss/at-else-closing-brace-space-after`.
  pub struct ScssAtElseClosingBraceSpaceAfter => ScssAtElseClosingBraceSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the closing brace of @else before @else",
    default_severity: Warning,
  }
}

/// `scss/at-else-closing-brace-space-after` as configured.
pub struct ScssAtElseClosingBraceSpaceAfterCheck {
  /// The primary option; `"always-intermediate"` when none is given.
  option: &'static str,
}

impl ScssAtElseClosingBraceSpaceAfterCheck {
  /// stylelint-scss's options: `"always-intermediate"` or
  /// `"never-intermediate"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always-intermediate", "never-intermediate"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always-intermediate"),
    })
  }
}

impl Check for ScssAtElseClosingBraceSpaceAfterCheck {
  /// Scans the SCSS source for `@else` chains and flags a closing brace whose
  /// following space does not match the option.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.option;
    scss_chain_closes(ctx.source, ChainReading::ElseRules, &[ChainHead::ElseIf])
      .into_iter()
      .filter(|c| c.followed_by_else)
      .filter_map(|c| {
        let message = match option {
          "always-intermediate" if c.spaces != 1 => {
            "Expected single space after closing brace of @else if before @else"
          }
          "never-intermediate" if c.spaces > 0 => {
            "Unexpected space after closing brace of @else if before @else"
          }
          _ => return None,
        };
        Some(Diagnostic::new(NAME, message.to_string()).span(Span::new(c.close, 1)))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, scss_ctx_with_source};

  #[test]
  fn allows_space_between_else_if_and_else() {
    let rule = configured(&ScssAtElseClosingBraceSpaceAfter, None);
    let ctx = scss_ctx_with_source("@if $a { a: b; } @else if $b { c: d; } @else { e: f; }");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_no_space_between_else_if_and_else() {
    let rule = configured(&ScssAtElseClosingBraceSpaceAfter, None);
    let ctx = scss_ctx_with_source("@if $a { a: b; } @else if $b { c: d; }@else { e: f; }");
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtElseClosingBraceSpaceAfter, None);
    let ctx = ctx_with_source("@else if { }@else { }");
    assert!(rule.check_source(&ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint_scss() {
    use crate::testing::invalid_options;
    assert!(invalid_options(NAME, serde_json::json!("never-intermediate")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"scss/at-else-closing-brace-space-after\""]
    );
  }
}
