//! Test helpers for autofixes: lint a snippet with one rule through the real
//! runner, then apply fixes and re-lint until the source settles, the way
//! `gale --fix` does.

use std::collections::HashMap;

use gale_css_parser::Syntax;
use gale_diagnostics::{Diagnostic, apply_fixes};

use crate::{LintRunner, RuleRegistry};

/// The warnings `rule` (configured with `options`) reports for `source`.
pub fn warnings(
  rule: &str,
  options: serde_json::Value,
  source: &str,
  syntax: Syntax,
) -> Vec<Diagnostic> {
  runner(rule, options)
    .lint_source(source, file_name(syntax), syntax)
    .diagnostics
}

/// `source` after `gale --fix` with only `rule` (configured with `options`)
/// enabled.
pub fn fix(rule: &str, options: serde_json::Value, source: &str, syntax: Syntax) -> String {
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
fn runner(rule: &str, options: serde_json::Value) -> LintRunner {
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
