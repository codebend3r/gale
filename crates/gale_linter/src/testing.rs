//! Shared helpers for rule tests.
//!
//! Tests that call a rule's check directly configure it with [`configured`]
//! and build their [`RuleContext`] with [`ctx`], [`scss_ctx_with_source`]
//! and the other builders here; [`CheckSource::check_source`]
//! runs its `check` over the context's source.  [`invalid_options`] is
//! what a rule set reports for a rule's options.
//! Tests that start from source text use [`lint`] and [`fix`], which lint a
//! snippet with one rule through the real parser and runner, then apply fixes
//! and re-lint until the source settles, the way `gale --fix` does;
//! [`located`] and [`ranges`] give each warning's line and column range,
//! as upstream's tests check them.

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, SourceLineIndex, apply_fixes};
use serde_json::Value;

use crate::{Check, LintEnv, Rule, RuleContext, RuleSet, RuleSetSpec, RuleSetting, Setting};

/// The warnings `rule` (configured with `options`) reports for `source`.
pub fn lint(rule: &str, options: Value, source: &str, syntax: Syntax) -> Vec<Diagnostic> {
  rule_set(rule, options)
    .lint(source, file_name(syntax), syntax)
    .diagnostics
}

/// Where a warning sits, as upstream's tests write it: `(line, column,
/// endLine, endColumn)`, 1-based.
pub type LineRange = (usize, usize, usize, usize);

/// The message and [`LineRange`] of each warning [`lint`] reports.
pub fn located(
  rule: &str,
  options: Value,
  source: &str,
  syntax: Syntax,
) -> Vec<(String, LineRange)> {
  let index = SourceLineIndex::build(source);
  lint(rule, options, source, syntax)
    .into_iter()
    .map(|d| {
      let (line, column) = index.offset_to_location(d.span.offset);
      let (end_line, end_column) = index.offset_to_location(d.span.end());
      (d.message, (line, column, end_line, end_column))
    })
    .collect()
}

/// The [`LineRange`] of each warning [`lint`] reports, asserting that
/// every one of them says `message`.
pub fn ranges(
  rule: &str,
  options: Value,
  source: &str,
  syntax: Syntax,
  message: &str,
) -> Vec<LineRange> {
  located(rule, options, source, syntax)
    .into_iter()
    .map(|(actual, range)| {
      assert_eq!(actual, message);
      range
    })
    .collect()
}

/// `source` after `gale --fix` with only `rule` (configured with `options`)
/// enabled.
pub fn fix(rule: &str, options: Value, source: &str, syntax: Syntax) -> String {
  let set = rule_set(rule, options);
  let mut current = source.to_string();
  for _ in 0..10 {
    let result = set.lint(&current, file_name(syntax), syntax);
    let (fixed, count) = apply_fixes(&current, &result.diagnostics);
    if count == 0 || fixed == current {
      break;
    }
    current = fixed;
  }
  current
}

/// `rule` configured with `options` (or none), for tests that call its
/// check directly.  Panics if the options are invalid.
pub fn configured(rule: &'static dyn Rule, options: impl Into<Option<Value>>) -> Box<dyn Check> {
  configured_in(rule, options, LintEnv::default())
}

/// [`configured`] for a project whose environment is `env`.
pub fn configured_in(
  rule: &'static dyn Rule,
  options: impl Into<Option<Value>>,
  env: LintEnv,
) -> Box<dyn Check> {
  let options = options.into();
  let meta = rule.meta();
  let setting = Setting::new(&meta, options.as_ref());
  match rule.configure(&setting, &env) {
    Ok(check) => check,
    Err(problems) => panic!("invalid options for {}: {problems:?}", meta.name),
  }
}

/// Run a configured rule's `check` the way the runner does.
pub trait CheckSource {
  /// What `check` reports for the tree of `ctx`'s source.
  fn check_source(&self, ctx: &RuleContext) -> Vec<Diagnostic>;
}

impl<C: Check + ?Sized> CheckSource for C {
  fn check_source(&self, ctx: &RuleContext) -> Vec<Diagnostic> {
    self.check(&PostcssTree::parse(ctx.source, ctx.syntax), ctx)
  }
}

/// The invalid-option warnings a rule set running `rule` with `options`
/// reports on every file.
pub fn invalid_options(rule: &str, options: Value) -> Vec<String> {
  rule_set(rule, options).invalid_options().to_vec()
}

/// A rule set with just `rule` enabled.
fn rule_set(rule: &str, options: Value) -> RuleSet {
  RuleSet::new(RuleSetSpec {
    rules: vec![RuleSetting::new(rule).with_options(options)],
    ..RuleSetSpec::default()
  })
}

/// A file name whose extension matches `syntax`.
fn file_name(syntax: Syntax) -> &'static str {
  match syntax {
    Syntax::Css => "test.css",
    Syntax::Scss => "test.scss",
    Syntax::Less => "test.less",
    Syntax::Sass => "test.sass",
  }
}

/// A context for `source` in `syntax`, for a file whose name matches the
/// syntax.
pub fn context(source: &str, syntax: Syntax) -> RuleContext<'_> {
  RuleContext {
    env: Default::default(),
    file_path: file_name(syntax),
    source,
    syntax,
    embedded: None,
  }
}

/// A CSS context with no source text, for tests that build their nodes by
/// hand.
pub fn ctx<'a>() -> RuleContext<'a> {
  context("", Syntax::Css)
}

/// A CSS context for `source`.
pub fn ctx_with_source(source: &str) -> RuleContext<'_> {
  context(source, Syntax::Css)
}

/// An SCSS context for `source`.
pub fn scss_ctx_with_source(source: &str) -> RuleContext<'_> {
  context(source, Syntax::Scss)
}
