//! Shared helpers for rule tests.
//!
//! Tests that call a rule's `check` or `check_root` directly build their
//! [`RuleContext`] with [`ctx`], [`scss_ctx`] and the other builders here.
//! Tests that start from source text use [`lint`] and [`fix`], which lint a
//! snippet with one rule through the real parser and runner, then apply fixes
//! and re-lint until the source settles, the way `gale --fix` does.

use std::collections::HashMap;

use gale_css_parser::Syntax;
use gale_diagnostics::{Diagnostic, apply_fixes};
use serde_json::Value;

use crate::{LintRunner, RuleContext, RuleRegistry};

/// The warnings `rule` (configured with `options`) reports for `source`.
pub fn lint(rule: &str, options: Value, source: &str, syntax: Syntax) -> Vec<Diagnostic> {
  runner(rule, options)
    .lint_source(source, file_name(syntax), syntax)
    .diagnostics
}

/// `source` after `gale --fix` with only `rule` (configured with `options`)
/// enabled.
pub fn fix(rule: &str, options: Value, source: &str, syntax: Syntax) -> String {
  let runner = runner(rule, options);
  let mut current = source.to_string();
  for _ in 0..10 {
    let result = runner.lint_source(&current, file_name(syntax), syntax);
    let (fixed, count) = apply_fixes(&current, &result.diagnostics);
    if count == 0 || fixed == current {
      break;
    }
    current = fixed;
  }
  current
}

/// A runner with just `rule` enabled.
fn runner(rule: &str, options: Value) -> LintRunner {
  let mut all = HashMap::new();
  all.insert(rule.to_string(), options);
  LintRunner::with_options(RuleRegistry::default(), vec![rule.to_string()], all)
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

/// Rule options in whatever form a test has them: borrowed, owned, or an
/// owned `Option`.  Owned values are leaked, which lets a context built from
/// a temporary `json!` outlive the statement that made it.
pub trait TestOptions<'a> {
  /// The options as a [`RuleContext`] holds them.
  fn into_options(self) -> Option<&'a Value>;
}

impl<'a> TestOptions<'a> for &'a Value {
  /// Borrow the options as they are.
  fn into_options(self) -> Option<&'a Value> {
    Some(self)
  }
}

impl<'a> TestOptions<'a> for Value {
  /// Leak the options so the context can keep a reference to them.
  fn into_options(self) -> Option<&'a Value> {
    Some(Box::leak(Box::new(self)))
  }
}

impl<'a> TestOptions<'a> for Option<Value> {
  /// Leak the options, if any, so the context can keep a reference to them.
  fn into_options(self) -> Option<&'a Value> {
    self.map(|value| &*Box::leak(Box::new(value)))
  }
}

/// A context for `source` in `syntax`, configured with `options`, for a file
/// whose name matches the syntax.  It has no per-file cache, so every
/// accessor builds afresh, as it does for any context outside the runner.
pub fn context<'a>(
  source: &'a str,
  syntax: Syntax,
  options: impl TestOptions<'a>,
) -> RuleContext<'a> {
  RuleContext {
    file_path: file_name(syntax),
    source,
    syntax,
    options: options.into_options(),
    cache: None,
  }
}

/// A CSS context with no source text and no options, for tests that build
/// their nodes by hand.
pub fn ctx<'a>() -> RuleContext<'a> {
  context("", Syntax::Css, None)
}

/// A CSS context for `source`, with no options.
pub fn ctx_with_source(source: &str) -> RuleContext<'_> {
  context(source, Syntax::Css, None)
}

/// A CSS context with no source text, configured with `options`.
pub fn ctx_with_options<'a>(options: impl TestOptions<'a>) -> RuleContext<'a> {
  context("", Syntax::Css, options)
}

/// A CSS context for `source`, configured with `options`.
pub fn ctx_with_source_and_options<'a>(
  source: &'a str,
  options: impl TestOptions<'a>,
) -> RuleContext<'a> {
  context(source, Syntax::Css, options)
}

/// An SCSS context with no source text and no options.
pub fn scss_ctx<'a>() -> RuleContext<'a> {
  context("", Syntax::Scss, None)
}

/// An SCSS context for `source`, with no options.
pub fn scss_ctx_with_source(source: &str) -> RuleContext<'_> {
  context(source, Syntax::Scss, None)
}

/// An SCSS context with no source text, configured with `options`.
pub fn scss_ctx_with_options<'a>(options: impl TestOptions<'a>) -> RuleContext<'a> {
  context("", Syntax::Scss, options)
}

/// An SCSS context for `source`, configured with `options`.
pub fn scss_ctx_with_source_and_options<'a>(
  source: &'a str,
  options: impl TestOptions<'a>,
) -> RuleContext<'a> {
  context(source, Syntax::Scss, options)
}

/// A Less context with no source text and no options.
pub fn less_ctx<'a>() -> RuleContext<'a> {
  context("", Syntax::Less, None)
}
