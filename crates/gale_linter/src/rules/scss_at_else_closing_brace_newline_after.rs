use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::{ChainHead, ChainReading, scss_chain_closes};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-else-closing-brace-newline-after";

declare_rule! {
  /// Require a newline after the closing brace of `@else` statements.
  ///
  /// Default: `"always-last-in-chain"` — require a newline after the closing
  /// brace of the last `@else` in a chain.
  ///
  /// Equivalent to `scss/at-else-closing-brace-newline-after`.
  pub struct ScssAtElseClosingBraceNewlineAfter => ScssAtElseClosingBraceNewlineAfterCheck {
    name: NAME,
    description: "Require or disallow a newline after the closing brace of @else statements",
    default_severity: Warning,
  }
}

/// `scss/at-else-closing-brace-newline-after` as configured.
pub struct ScssAtElseClosingBraceNewlineAfterCheck {
  /// The primary option; `"always-last-in-chain"` when none is given.
  option: &'static str,
}

impl ScssAtElseClosingBraceNewlineAfterCheck {
  /// stylelint-scss's options: `"always-last-in-chain"`, then a secondary
  /// object whose only key is `disableFix` (which every rule takes), so any
  /// other key is unknown.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always-last-in-chain"]);
    options.disable_fix();
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always-last-in-chain"),
    })
  }
}

impl Check for ScssAtElseClosingBraceNewlineAfterCheck {
  /// Scans the SCSS source for `@else` chains and flags a closing brace whose
  /// following newline does not match the option.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    if self.option != "always-last-in-chain" {
      return vec![];
    }

    scss_chain_closes(ctx.source, ChainReading::ElseRules, &[ChainHead::Else])
      .into_iter()
      .filter(|c| !c.line_ends_after)
      .map(|c| {
        Diagnostic::new(
          NAME,
          "Expected newline after closing brace of @else".to_string(),
        )
        .span(Span::new(c.close, 1))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, scss_ctx_with_source};

  #[test]
  fn allows_newline_after_else() {
    let rule = configured(&ScssAtElseClosingBraceNewlineAfter, None);
    let ctx = scss_ctx_with_source("@if $c { a: b; } @else { c: d; }\n.foo {}");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_eof_after_else() {
    let rule = configured(&ScssAtElseClosingBraceNewlineAfter, None);
    let ctx = scss_ctx_with_source("@if $c { a: b; } @else { c: d; }");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtElseClosingBraceNewlineAfter, None);
    let ctx = ctx_with_source("@else { } .foo {}");
    assert!(rule.check_source(&ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint_scss() {
    use crate::testing::invalid_options;
    let valid = serde_json::json!(["always-last-in-chain", { "disableFix": true }]);
    assert!(invalid_options(NAME, valid).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"scss/at-else-closing-brace-newline-after\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always-last-in-chain", { "ignore": ["x"] }])
      ),
      ["Invalid option name \"ignore\" for rule \"scss/at-else-closing-brace-newline-after\""]
    );
  }
}
