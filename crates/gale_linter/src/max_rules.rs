//! The `selector-max-*` rules that count one thing per comma-separated
//! selector, as [`ListRule`](crate::list_rules) does for the list rules.

use std::borrow::Cow;
use std::marker::PhantomData;

use gale_css_parser::postcss_tree::{PostcssTree, RuleRef};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Options, Setting};
use crate::rule::{Check, RuleContext};

/// A `selector-max-*` rule that counts something in each comma-separated
/// selector of a rule and reports the rule when the count passes the
/// maximum, its primary option.
///
/// The selector-max rules that share this read the selector as PostCSS
/// stores it (without its comments), split it at every comma and report at
/// the whole rule; `selector-max-id`, `selector-max-attribute` and
/// `selector-max-specificity` read their selectors differently and keep
/// checks of their own.  The family lives in this file because the rules
/// directory holds one rule per file.
pub trait MaxRule: Send + Sync + 'static {
  /// The rule's name.
  const NAME: &'static str;
  /// The maximum when the config gives none.
  const DEFAULT: usize;

  /// The rule's secondary options, read once.
  type Options: Default + Send + Sync;

  /// Read the rule's secondary options.  A rule that takes none reads
  /// nothing, so whatever secondary options it is given are ignored, as
  /// Stylelint does for a rule that validates none.
  fn options(options: &mut Options<'_>) -> Self::Options {
    let _ = options;
    Self::Options::default()
  }

  /// The selector of `rule` as it is counted.
  fn selector<'t>(rule: RuleRef<'t>, ctx: &RuleContext) -> Cow<'t, str> {
    let _ = ctx;
    Cow::Borrowed(rule.selector())
  }

  /// How many of the counted things one selector holds.
  fn count(selector: &str, options: &Self::Options) -> usize;

  /// The problem's message for `selector`, which holds `count` against a
  /// maximum of `max`.
  fn message(selector: &str, max: usize, count: usize) -> String;
}

/// A [`MaxRule`] configured with its maximum.
pub struct MaxCheck<R: MaxRule> {
  /// The most of the counted things a selector may hold.
  max: usize,
  options: R::Options,
  rule: PhantomData<fn() -> R>,
}

impl<R: MaxRule> MaxCheck<R> {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`), then the rule's own.
  /// Gale reads no number as the rule's default.
  pub fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_max(R::DEFAULT);
    let extra = R::options(&mut options);
    options.finish()?;
    Ok(Self {
      max,
      options: extra,
      rule: PhantomData,
    })
  }
}

impl<R: MaxRule> Check for MaxCheck<R> {
  /// Flags each comma-separated selector holding more than allowed, at the
  /// rule it is in.
  ///
  /// Rules that are not standard syntax are skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_rules()
      .filter(|rule| rule.is_standard_syntax())
      .flat_map(|rule| {
        let selector = R::selector(rule, ctx);
        selector
          .split(',')
          .map(str::trim)
          // An empty selector counts nothing, so it never passes the maximum.
          .filter(|sel| !sel.is_empty())
          .filter_map(|sel| {
            let count = R::count(sel, &self.options);
            (count > self.max)
              .then(|| Diagnostic::new(R::NAME, R::message(sel, self.max, count)).span(rule.span()))
          })
          .collect::<Vec<_>>()
      })
      .collect()
  }
}
