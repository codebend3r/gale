use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::sass_operators::{is_whitespace, operator_sites};

const NAME: &str = "scss/operator-no-newline-after";

declare_rule! {
  /// Disallow a line break right after a Sass operator (`10px +\n  1`).
  ///
  /// Equivalent to stylelint-scss's `scss/operator-no-newline-after`, which
  /// runs `operator-no-unspaced`'s `calculationOperatorSpaceChecker` over the
  /// stylesheet and reports each operator with only whitespace, a line break
  /// among it, after it.  Operators are told from signs, hyphens and CSS
  /// slashes by [`find_operators`](crate::sass_operators::find_operators).
  pub struct ScssOperatorNoNewlineAfter => ScssOperatorNoNewlineAfterCheck {
    name: NAME,
    description: "Disallow newlines after Sass operators",
    default_severity: Warning,
  }
}

/// `scss/operator-no-newline-after`, which takes no options.
pub struct ScssOperatorNoNewlineAfterCheck;

impl ScssOperatorNoNewlineAfterCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssOperatorNoNewlineAfterCheck {
  /// Reports each operator followed by whitespace with a line break in it
  /// (upstream's `checkNewlineBefore`, misnamed in `operator-no-newline-after`).
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    operator_sites(tree)
      .into_iter()
      .filter(|site| {
        site
          .after()
          .iter()
          .take_while(|&&b| is_whitespace(b))
          .any(|&b| b == b'\n')
      })
      .map(|site| {
        let symbol = site.symbol();
        Diagnostic::new(NAME, format!("Unexpected newline after \"{symbol}\""))
          .message_args([symbol])
          .span(site.span())
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
  };

  #[test]
  fn reports_plus_before_newline() {
    let rule = configured(&ScssOperatorNoNewlineAfter, None);
    let source = "$a: 1 +\n  2;";
    let d = rule.check_source(&scss_ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains('+'));
  }

  #[test]
  fn allows_operator_in_middle_of_line() {
    let rule = configured(&ScssOperatorNoNewlineAfter, None);
    let source = "$a: 1 + 2;";
    let d = rule.check_source(&scss_ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssOperatorNoNewlineAfter, None);
    let ctx = ctx_with_source("$a: 1 +\n  2;");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_operators_inside_block_comments() {
    let rule = configured(&ScssOperatorNoNewlineAfter, None);
    let ctx = scss_ctx_with_source(
      "/* two-line options (title +\n * description) render in a popover */\n.a { width: 1px + 2px; }\n",
    );
    assert!(rule.check_source(&ctx).is_empty());
  }

  #[test]
  fn reports_operator_after_block_comment_on_same_line() {
    let rule = configured(&ScssOperatorNoNewlineAfter, None);
    let ctx = scss_ctx_with_source(".a { width: /* c */ 1px +\n 2px; }\n");
    assert_eq!(rule.check_source(&ctx).len(), 1);
  }

  #[test]
  fn allows_unary_minus_at_line_end() {
    let rule = configured(&ScssOperatorNoNewlineAfter, None);
    // A line ending with ` -` after a colon is likely unary (start of value)
    let source = "color:\n  -1rem;";
    let d = rule.check_source(&scss_ctx_with_source(source));
    // No operator at end of line — "color:" ends with ":", not an operator
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/operator-no-newline-after\""]
    );
  }
}
