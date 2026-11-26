use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::sass_operators::{OperatorSite, is_whitespace, operator_sites};

const NAME: &str = "scss/operator-no-unspaced";

declare_rule! {
  /// Disallow a Sass operator without a single space (or a line break) on
  /// each side (`10px+1`).
  ///
  /// Equivalent to stylelint-scss's `scss/operator-no-unspaced`, which runs
  /// its `calculationOperatorSpaceChecker` over declaration values, at-rule
  /// params, media feature values and the `#{...}` interpolation of
  /// properties, selectors and comments.  Operators are told from signs,
  /// hyphens and CSS slashes by
  /// [`find_operators`](crate::sass_operators::find_operators), and each
  /// unspaced side is reported on its own.
  pub struct ScssOperatorNoUnspaced => ScssOperatorNoUnspacedCheck {
    name: NAME,
    description: "Disallow unspaced Sass operators",
    default_severity: Warning,
  }
}

/// `scss/operator-no-unspaced`, which takes no options.
pub struct ScssOperatorNoUnspacedCheck;

impl ScssOperatorNoUnspacedCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssOperatorNoUnspacedCheck {
  /// Reports each operator without exactly one space (or a line break) on
  /// either side (`checkSpaces`): once for the side before, once for the
  /// side after.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(context.syntax, Syntax::Scss | Syntax::Less | Syntax::Sass) {
      return vec![];
    }
    operator_sites(tree)
      .into_iter()
      .flat_map(spacing_problems)
      .collect()
  }
}

/// The side of an operator missing its space.
#[derive(Clone, Copy)]
enum Side {
  Before,
  After,
}

/// What `checkSpaces` reports about one operator.
fn spacing_problems(site: OperatorSite<'_>) -> Vec<Diagnostic> {
  [
    (Side::Before, is_spaced_before(site.before())),
    (Side::After, is_spaced_after(site.after())),
  ]
  .into_iter()
  .filter(|&(_, is_spaced)| !is_spaced)
  .map(|(side, _)| {
    let symbol = site.symbol();
    let message = match side {
      Side::Before => format!("Expected single space before \"{symbol}\""),
      Side::After => format!("Expected single space after \"{symbol}\""),
    };
    Diagnostic::new(NAME, message)
      .message_args([symbol])
      .span(site.span())
  })
  .collect()
}

/// One space that is not part of a longer run of whitespace, or a line
/// break somewhere in the whitespace (upstream's `newlineBefore`, whose scan
/// stops before the string's first character).
fn is_spaced_before(before: &[u8]) -> bool {
  let single_space = before.last() == Some(&b' ')
    && !before
      .len()
      .checked_sub(2)
      .is_some_and(|i| is_whitespace(before[i]));
  single_space
    || before[1.min(before.len())..]
      .iter()
      .rev()
      .take_while(|&&b| is_whitespace(b))
      .any(|&b| b == b'\n')
}

/// One space that is not followed by more whitespace, or a line break right
/// after the operator.
fn is_spaced_after(after: &[u8]) -> bool {
  match after {
    [b' ', next, ..] => !is_whitespace(*next),
    [b' '] | [b'\n', ..] | [b'\r', b'\n', ..] => true,
    _ => false,
  }
}

#[cfg(test)]
mod tests;
