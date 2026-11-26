//! The stylelint-scss rules that match the names an SCSS at-rule defines
//! against a pattern.

use std::marker::PhantomData;

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext};

/// Kebab-case, the names Gale expects when the config gives no pattern.
const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// A stylelint-scss rule that matches the names an SCSS at-rule defines
/// against a pattern: `scss/at-function-pattern` and
/// `scss/at-mixin-pattern`, which differ only in their names and at-rules.
pub trait AtNamePattern: Send + Sync + 'static {
  /// The rule's name.
  const NAME: &'static str;
  /// The at-rule whose names are matched, without its `@`.
  const AT_RULE: &'static str;
}

/// An [`AtNamePattern`] rule as configured.
pub struct AtNamePatternCheck<R: AtNamePattern> {
  /// The pattern every name must match.
  pattern: Pattern,
  rule: PhantomData<fn() -> R>,
}

impl<R: AtNamePattern> AtNamePatternCheck<R> {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option, as Stylelint reports it.
  pub fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern_or(DEFAULT_PATTERN);
    options.finish()?;
    Ok(Self {
      pattern,
      rule: PhantomData,
    })
  }
}

impl<R: AtNamePattern> Check for AtNamePatternCheck<R> {
  /// Flags a name the at-rule defines that does not match the configured
  /// regex, defaulting to kebab-case, at the name as stylelint-scss finds
  /// it.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let pattern_str = self.pattern.source.as_str();
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if at.name() != R::AT_RULE {
        continue;
      }
      // The name is the first word in params (before `(` or whitespace)
      let first_token = at.params().split_whitespace().next().unwrap_or("");
      let name = first_token.split('(').next().unwrap_or("");
      if name.is_empty() || self.pattern.is_match(name) {
        continue;
      }
      diags.push(
        Diagnostic::new(
          R::NAME,
          format!(
            "Expected @{} name \"{name}\" to match pattern \"{pattern_str}\"",
            R::AT_RULE
          ),
        )
        .span(at.word_span(name)),
      );
    }
    diags
  }
}
