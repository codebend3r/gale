//! What the `*-empty-line-before` rules share: Stylelint's `hasEmptyLine`
//! test and the `fixEmptyLinesBefore` fix (`addEmptyLineBefore` /
//! `removeEmptyLinesBefore`), applied to a node's `raws.before` in a
//! [`PostcssTree`].

use std::sync::OnceLock;

use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};
use regex::Regex;

use crate::postcss_tree::PostcssTree;

/// Stylelint's `hasEmptyLine`: whether `text` holds a line with nothing but
/// spaces, tabs or carriage returns on it (`/\n[\r\t ]*\n/`).
pub fn has_empty_line(text: &str) -> bool {
  let bytes = text.as_bytes();
  bytes.iter().enumerate().any(|(i, &b)| {
    b == b'\n'
      && bytes[i + 1..]
        .iter()
        .find(|&&c| !matches!(c, b'\r' | b'\t' | b' '))
        == Some(&b'\n')
  })
}

/// The line break Stylelint writes in fixes (its `context.newline`): the
/// first one in the file, or `\n` when there is none.
pub fn newline_of(source: &str) -> &'static str {
  match source.find('\n') {
    Some(at) if at > 0 && source.as_bytes()[at - 1] == b'\r' => "\r\n",
    _ => "\n",
  }
}

/// Stylelint's `addEmptyLineBefore`: double the first line break of
/// `before`, keeping the indentation after it, or put two line breaks in
/// front when it has none.
pub fn add_empty_line(before: &str, newline: &str) -> String {
  match before.find('\n') {
    None => format!("{newline}{newline}{before}"),
    Some(at) => {
      let start = if at > 0 && before.as_bytes()[at - 1] == b'\r' {
        at - 1
      } else {
        at
      };
      format!("{}{newline}{}", &before[..start], &before[start..])
    }
  }
}

/// Stylelint's `removeEmptyLinesBefore`: collapse every run of line breaks
/// with only whitespace between them (`/(\r?\n\s*\n)+/g`) into one line
/// break.
pub fn remove_empty_lines(before: &str, newline: &str) -> String {
  static EMPTY_LINES: OnceLock<Regex> = OnceLock::new();
  let re = EMPTY_LINES.get_or_init(|| Regex::new(r"(\r?\n\s*\n)+").expect("valid regex"));
  re.replace_all(before, regex::NoExpand(newline))
    .into_owned()
}

/// Whether an empty line must be added before a node or removed from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyLineAction {
  /// Stylelint's `add`.
  Add,
  /// Stylelint's `remove`.
  Remove,
}

/// Stylelint's `fixEmptyLinesBefore` for `node`: one edit rewriting its
/// `raws.before`, using the file's own line break.
pub fn fix_empty_lines_before(tree: &PostcssTree, node: usize, action: EmptyLineAction) -> Fix {
  let newline = newline_of(tree.source());
  let before = tree.before(node);
  let (description, new_before) = match action {
    EmptyLineAction::Add => ("Add an empty line", add_empty_line(before, newline)),
    EmptyLineAction::Remove => (
      "Remove the empty lines",
      remove_empty_lines(before, newline),
    ),
  };
  let range = &tree.nodes[node].before;
  Fix::new(
    description,
    vec![Edit::new(
      Span::from_range(range.start, range.end),
      new_before,
    )],
  )
}

/// The report an `*-empty-line-before` rule makes when `node` does not
/// match the expectation: `Expected empty line before <what>` or `Expected
/// no empty line before <what>`, at the node, with the fix that adds or
/// removes the empty line.
pub fn empty_line_report(
  rule_name: &str,
  severity: Severity,
  tree: &PostcssTree,
  node: usize,
  expect_empty_line: bool,
  what: &str,
) -> Diagnostic {
  let (message, action) = if expect_empty_line {
    (
      format!("Expected empty line before {what}"),
      EmptyLineAction::Add,
    )
  } else {
    (
      format!("Expected no empty line before {what}"),
      EmptyLineAction::Remove,
    )
  };
  let n = &tree.nodes[node];
  Diagnostic::new(rule_name, message)
    .severity(severity)
    .span(Span::from_range(n.start, n.end.max(n.start)))
    .fix(fix_empty_lines_before(tree, node, action))
}

/// Lint `source` with only `rule` enabled and apply its fixes the way
/// `gale --fix` does, until the output stops changing.  For rule tests.
#[cfg(test)]
pub(crate) fn fix_with(
  rule: &str,
  options: serde_json::Value,
  source: &str,
  syntax: gale_css_parser::Syntax,
) -> String {
  use crate::{LintRunner, RuleRegistry};
  let mut opts = std::collections::HashMap::new();
  opts.insert(rule.to_string(), options);
  let runner = LintRunner::with_options(RuleRegistry::default(), vec![rule.to_string()], opts);
  let path = match syntax {
    gale_css_parser::Syntax::Scss => "test.scss",
    gale_css_parser::Syntax::Less => "test.less",
    gale_css_parser::Syntax::Sass => "test.sass",
    gale_css_parser::Syntax::Css => "test.css",
  };
  let mut current = source.to_string();
  for _ in 0..10 {
    let result = runner.lint_source(&current, path, syntax);
    let (fixed, count) = gale_diagnostics::apply_fixes(&current, &result.diagnostics);
    if count == 0 || fixed == current {
      break;
    }
    current = fixed;
  }
  current
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_lines_are_lines_of_spaces_tabs_or_carriage_returns() {
    assert!(has_empty_line("\n\n"));
    assert!(has_empty_line("x\n \t\r\n  "));
    assert!(!has_empty_line("\n  "));
    assert!(!has_empty_line("\n;\n"));
    assert!(!has_empty_line(""));
  }

  #[test]
  fn newline_follows_the_first_line_break() {
    assert_eq!(newline_of("a {}\r\nb {}\n"), "\r\n");
    assert_eq!(newline_of("a {}\nb {}\r\n"), "\n");
    assert_eq!(newline_of("a {}"), "\n");
  }

  #[test]
  fn adding_doubles_the_first_line_break_and_keeps_indentation() {
    assert_eq!(add_empty_line("\n  ", "\n"), "\n\n  ");
    assert_eq!(add_empty_line("\r\n\t", "\r\n"), "\r\n\r\n\t");
    assert_eq!(add_empty_line(" ", "\n"), "\n\n ");
    assert_eq!(add_empty_line("", "\r\n"), "\r\n\r\n");
    assert_eq!(add_empty_line(";\n  ", "\n"), ";\n\n  ");
  }

  #[test]
  fn removing_collapses_runs_of_empty_lines() {
    assert_eq!(remove_empty_lines("\n\n  ", "\n"), "\n  ");
    assert_eq!(remove_empty_lines("\n  \n\n\t", "\n"), "\n\t");
    assert_eq!(remove_empty_lines("\r\n\r\n  ", "\r\n"), "\r\n  ");
    assert_eq!(remove_empty_lines("\n  ", "\n"), "\n  ");
  }
}
