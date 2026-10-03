//! What the `*-empty-line-before` rules share: Stylelint's `hasEmptyLine`
//! test and the `fixEmptyLinesBefore` fix (`addEmptyLineBefore` /
//! `removeEmptyLinesBefore`), applied to a node's `raws.before` in a
//! [`PostcssTree`].

use std::sync::OnceLock;

use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};
use regex::Regex;

use crate::postcss_tree::PostcssTree;
use crate::stylelint_version::installed_at_least;

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

/// The message for an empty line Stylelint wants gone: `Expected no empty
/// line before <what>` since Stylelint 17.7 reworded its messages, and
/// `Unexpected empty line before <what>` in the versions before it.
pub fn rejected_message(what: &str, reworded: bool) -> String {
  if reworded {
    format!("Expected no empty line before {what}")
  } else {
    format!("Unexpected empty line before {what}")
  }
}

/// The report an `*-empty-line-before` rule makes when `node` does not
/// match the expectation: `Expected empty line before <what>`, or the
/// [`rejected_message`] the installed Stylelint words, at the node, with
/// the fix that adds or removes the empty line.
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
      rejected_message(what, installed_at_least(17, 7)),
      EmptyLineAction::Remove,
    )
  };
  let n = &tree.nodes[node];
  Diagnostic::new(rule_name, message)
    .severity(severity)
    .span(Span::from_range(n.start, n.end.max(n.start)))
    .fix(fix_empty_lines_before(tree, node, action))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejected_wording_follows_the_stylelint_version() {
    assert_eq!(
      rejected_message("custom property", true),
      "Expected no empty line before custom property"
    );
    assert_eq!(
      rejected_message("custom property", false),
      "Unexpected empty line before custom property"
    );
  }

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
