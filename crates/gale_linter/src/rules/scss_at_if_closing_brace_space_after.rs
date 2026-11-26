use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::{ChainHead, ChainReading, scss_chain_closes};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-if-closing-brace-space-after";

declare_rule! {
  /// Require or disallow a space after the closing brace of `@if` statements
  /// that are followed by `@else`.
  ///
  /// Default: `"always-intermediate"` — require a single space before `@else`
  /// when on the same line.
  ///
  /// Equivalent to `scss/at-if-closing-brace-space-after`.
  pub struct ScssAtIfClosingBraceSpaceAfter => ScssAtIfClosingBraceSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the closing brace of @if before @else",
    default_severity: Warning,
  }
}

/// `scss/at-if-closing-brace-space-after` as configured.
pub struct ScssAtIfClosingBraceSpaceAfterCheck {
  /// The primary option: `"always-intermediate"` or `"never-intermediate"`.
  expectation: &'static str,
}

impl ScssAtIfClosingBraceSpaceAfterCheck {
  /// Stylelint's options: `"always-intermediate"` or `"never-intermediate"`, `"always-intermediate"` when not given.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always-intermediate", "never-intermediate"])
      .unwrap_or("always-intermediate");
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssAtIfClosingBraceSpaceAfterCheck {
  /// Flags an `@if` closing brace followed by `@else` whose separating space
  /// does not match the option.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.expectation;
    scss_chain_closes(
      ctx.source,
      ChainReading::IfRules,
      &[ChainHead::If, ChainHead::ElseIf],
    )
    .into_iter()
    .filter(|c| c.followed_by_else)
    .filter_map(|c| {
      let message = match option {
        "always-intermediate" if c.spaces != 1 => {
          "Expected single space after closing brace of @if before @else"
        }
        "never-intermediate" if c.spaces > 0 => {
          "Unexpected space after closing brace of @if before @else"
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

  use crate::testing::{CheckSource, configured, invalid_options, scss_ctx_with_source};

  #[test]
  fn allows_single_space_before_else() {
    let rule = configured(&ScssAtIfClosingBraceSpaceAfter, None);
    let ctx = scss_ctx_with_source("@if $cond { color: red; } @else { color: blue; }");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_no_space_before_else() {
    let rule = configured(&ScssAtIfClosingBraceSpaceAfter, None);
    let ctx = scss_ctx_with_source("@if $cond { color: red; }@else { color: blue; }");
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_if_not_followed_by_else() {
    let rule = configured(&ScssAtIfClosingBraceSpaceAfter, None);
    let ctx = scss_ctx_with_source("@if $cond { color: red; }\n.foo {}");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always-intermediate")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-intermediate")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/at-if-closing-brace-space-after\""]
    );
  }
}
