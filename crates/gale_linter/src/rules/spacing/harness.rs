//! Table-driven tests for the spacing rules, in the shape of the upstream
//! suites (`testRule({ config, accept, reject })`): each [`Case`] is a
//! source that must pass, or one that must report the given problems (by
//! message, line and column, and end position where upstream gives one)
//! and, where given, fix to the given source.
//!
//! The families' `cases` modules hold the upstream suites of the @stylistic
//! plugin (5.2.1) and stylelint-scss (7.2.0), followed by gale's own cases
//! from the rules' earlier tests.

use gale_css_parser::Syntax;
use gale_diagnostics::SourceLineIndex;
use serde_json::Value;

use crate::testing::{fix, lint};

/// A problem a case expects: `(line, column)`, an end position when
/// upstream checks one, and the message.
#[derive(Debug, Clone)]
pub struct Expected {
  line: usize,
  column: usize,
  end: Option<(usize, usize)>,
  message: &'static str,
}

/// One source and what the rule makes of it.
#[derive(Debug, Clone)]
pub struct Case {
  code: &'static str,
  problems: Vec<Expected>,
  /// Whether the case must report (a `reject` case).
  reject: bool,
  /// What `--fix` must turn the source into.
  fixed: Option<&'static str>,
}

/// A source the rule accepts.
pub fn ok(code: &'static str) -> Case {
  Case {
    code,
    problems: vec![],
    reject: false,
    fixed: None,
  }
}

/// A source the rule rejects; add its problems with [`Case::at`].
pub fn bad(code: &'static str) -> Case {
  Case {
    reject: true,
    ..ok(code)
  }
}

impl Case {
  /// A problem at `line:column`.
  pub fn at(mut self, line: usize, column: usize, message: &'static str) -> Self {
    self.problems.push(Expected {
      line,
      column,
      end: None,
      message,
    });
    self
  }

  /// A problem at `line:column`, ending at `end_line:end_column`.
  pub fn span(
    mut self,
    (line, column): (usize, usize),
    end: (usize, usize),
    message: &'static str,
  ) -> Self {
    self.problems.push(Expected {
      line,
      column,
      end: Some(end),
      message,
    });
    self
  }

  /// What the source fixes to.
  pub fn fixed(mut self, fixed: &'static str) -> Self {
    self.fixed = Some(fixed);
    self
  }

  /// The fix leaves the source as it is.
  pub fn unfixed(self) -> Self {
    let code = self.code;
    self.fixed(code)
  }
}

/// `(line, column, end line, end column, message)` of each problem `rule`
/// reports for `source`, positioned as the Stylelint formatter puts them.
fn problems(
  rule: &str,
  options: &Value,
  source: &str,
  syntax: Syntax,
) -> Vec<(usize, usize, usize, usize, String)> {
  let index = SourceLineIndex::build(source);
  let mut found: Vec<_> = lint(rule, options.clone(), source, syntax)
    .into_iter()
    .map(|d| {
      let (line, column) = index.offset_to_location(d.span.offset);
      let (mut end_line, mut end_column) = index.offset_to_location(d.span.end());
      let at_line_end = matches!(
        source.as_bytes().get(d.span.offset),
        None | Some(b'\n' | b'\r')
      );
      if d.span.length == 0 && at_line_end {
        (end_line, end_column) = (line, column + 1);
      }
      (line, column, end_line, end_column, d.message)
    })
    .collect();
  found.sort();
  found
}

/// Run `cases` with `rule` configured with `options` on sources of
/// `syntax`, and report every case that fails.
pub fn check(rule: &str, options: Value, syntax: Syntax, cases: &[Case]) {
  let mut failures = Vec::new();
  for case in cases {
    let found = problems(rule, &options, case.code, syntax);
    let matches = found.len() == case.problems.len()
      && case.problems.iter().all(|expected| {
        found
          .iter()
          .any(|&(line, column, end_line, end_column, ref message)| {
            (line, column) == (expected.line, expected.column)
              && expected.end.is_none_or(|end| end == (end_line, end_column))
              && message == expected.message
          })
      })
      && (case.reject || found.is_empty());
    if !matches {
      failures.push(format!(
        "{options} {:?}\n  expected {:?}\n  found    {found:?}",
        case.code, case.problems
      ));
      continue;
    }
    if let Some(expected) = case.fixed {
      let fixed = fix(rule, options.clone(), case.code, syntax);
      if fixed != expected {
        failures.push(format!(
          "{options} {:?}\n  fixes to {fixed:?}\n  expected {expected:?}",
          case.code
        ));
      }
    }
  }
  assert!(
    failures.is_empty(),
    "{rule}: {} of {} cases failed:\n{}",
    failures.len(),
    cases.len(),
    failures.join("\n")
  );
}
