//! The braces of blocks, as the plugin's `block-opening-brace-*` and
//! `block-closing-brace-*` rules check them: every rule's and at-rule's
//! block (rules first, as the plugin walks them), through the block's
//! string, the string before it, and the node after it.

use gale_css_parser::postcss_tree::{NodeKind, NodeRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Edit};

use super::{
  before_block_string, block_string, closing_brace, from_end, index_in, node_string, statement_end,
  utf16_len, utf16_offset,
};
use crate::options::{Matchers, Options};
use crate::rule::{RuleContext, declare_rule};
use crate::whitespace::{
  At, Expectation, Messages, SPACE_ANY_LINE, Spacing, SpacingRule, Target, is_js_space,
  is_single_line, newline, replace,
};

/// Every rule, then every at-rule, with a block (`walkRules`, then
/// `walkAtRules`), and with only non-empty blocks when `non_empty`.  A
/// block the file ends inside, which PostCSS refuses to parse, is left
/// out.
fn blocks<'t>(
  tree: &'t PostcssTree<'_>,
  rules: bool,
  at_rules: bool,
  non_empty: bool,
) -> impl Iterator<Item = NodeRef<'t>> {
  let of = move |kind: NodeKind, wanted: bool| {
    tree.walk().filter(move |node| {
      wanted
        && node.kind == kind
        && closing_brace(*node).is_some()
        && !(non_empty && node.children().len() == 0)
    })
  };
  of(NodeKind::Rule, rules).chain(of(NodeKind::AtRule, at_rules))
}

/// `raws.between`: what separates a statement's selector or params from
/// its `{`.
fn between(node: NodeRef) -> std::ops::Range<usize> {
  let open = node.block_open.unwrap_or(node.end);
  let start = match node.kind {
    NodeKind::Rule => node.name_span.end,
    _ if node.params.is_empty() => node.name_span.end + node.after_name.len(),
    _ => node.value_span.end,
  };
  start.min(open)..open
}

/// `raws.after`: what follows a block's last node, up to the `}`.  An
/// at-rule the `}` ends without a `;` takes that text as its own
/// `raws.between`, which leaves `raws.after` empty.
fn after(node: NodeRef) -> std::ops::Range<usize> {
  let close = closing_brace(node).unwrap_or(node.end);
  let start = node
    .children()
    .next_back()
    .map_or(close, |last| statement_end(last));
  start.min(close)..close
}

/// Where the plugin reports a problem with the `}`: the character before
/// it in the statement's string, or the `\r` of a `\r\n` before it.
fn before_closing_brace(node: NodeRef) -> usize {
  let text = node_string(node);
  let back = if text.chars().rev().nth(2) == Some('\r') {
    3
  } else {
    2
  };
  from_end(node.tree().source(), node.start, &text, back)
}

/// The secondary options the opening-brace space rule reads.
pub struct IgnoreStatements {
  at_rules: Matchers,
  selectors: Matchers,
}

const OPENING_BRACE_SPACE_BEFORE: &str = "@stylistic/block-opening-brace-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the opening brace
  /// of blocks.
  ///
  /// Equivalent to `@stylistic/block-opening-brace-space-before`, with its
  /// `ignoreAtRules` and `ignoreSelectors`.
  pub struct StylisticBlockOpeningBraceSpaceBefore => Spacing<StylisticBlockOpeningBraceSpaceBefore> {
    name: OPENING_BRACE_SPACE_BEFORE,
    description: "Require or disallow a space before the opening brace of blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticBlockOpeningBraceSpaceBefore {
  const OPTIONS: &'static [&'static str] = SPACE_ANY_LINE;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \"{\""),
    rejected_before: Some("Unexpected whitespace before \"{\""),
    expected_before_single_line: Some("Expected single space before \"{\" of a single-line block"),
    rejected_before_single_line: Some("Unexpected whitespace before \"{\" of a single-line block"),
    expected_before_multi_line: Some("Expected single space before \"{\" of a multi-line block"),
    rejected_before_multi_line: Some("Unexpected whitespace before \"{\" of a multi-line block"),
    ..Messages::NONE
  };
  type Secondary = IgnoreStatements;

  fn secondary(options: &mut Options) -> IgnoreStatements {
    IgnoreStatements {
      at_rules: options.matchers("ignoreAtRules"),
      selectors: options.matchers("ignoreSelectors"),
    }
  }

  /// The plugin checks the end of `raws.before` + the string before the
  /// block, and reports at the end of the string before the block.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let ignore = &rule.secondary;
    let mut problems = Vec::new();
    for node in blocks(tree, true, true, true) {
      let ignored = match node.kind {
        NodeKind::AtRule => ignore.at_rules.matches(&node.name),
        _ => ignore.selectors.matches(&node.name),
      };
      if ignored {
        continue;
      }
      let head = before_block_string(node).unwrap_or_default();
      let text = format!("{}{head}", node.before());
      let block = block_string(node);
      let at = At::new(&text, text.len()).line_check(&block);
      let Some(message) = rule.checker.before(at) else {
        continue;
      };
      // Counted in the selector or params as PostCSS stores them, which
      // may be shorter than the source.
      let mut index = utf16_len(&head).saturating_sub(1);
      if head.ends_with("\r\n") {
        index = index.saturating_sub(1);
      }
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let edits = vec![replace(0, between(node), with)];
      let report = utf16_offset(tree.source(), node.start, index);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

const OPENING_BRACE_SPACE_AFTER: &str = "@stylistic/block-opening-brace-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the opening brace
  /// of blocks.
  ///
  /// Equivalent to `@stylistic/block-opening-brace-space-after`, with its
  /// `ignore: ["at-rules"]`.
  pub struct StylisticBlockOpeningBraceSpaceAfter => Spacing<StylisticBlockOpeningBraceSpaceAfter> {
    name: OPENING_BRACE_SPACE_AFTER,
    description: "Require or disallow a space after the opening brace of blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticBlockOpeningBraceSpaceAfter {
  const OPTIONS: &'static [&'static str] = SPACE_ANY_LINE;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \"{\""),
    rejected_after: Some("Unexpected whitespace after \"{\""),
    expected_after_single_line: Some("Expected single space after \"{\" of a single-line block"),
    rejected_after_single_line: Some("Unexpected whitespace after \"{\" of a single-line block"),
    expected_after_multi_line: Some("Expected single space after \"{\" of a multi-line block"),
    rejected_after_multi_line: Some("Unexpected whitespace after \"{\" of a multi-line block"),
    ..Messages::NONE
  };
  /// Whether at-rules are left out (`ignore: ["at-rules"]`).
  type Secondary = bool;

  fn secondary(options: &mut Options) -> bool {
    options
      .keywords("ignore", &["at-rules"])
      .contains(&"at-rules")
  }

  /// The plugin checks what follows the `{` in the block's string and
  /// reports just past the `{`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for node in blocks(tree, true, !rule.secondary, true) {
      let block = block_string(node);
      let Some(message) = rule.checker.after(At::new(&block, 0)) else {
        continue;
      };
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let edits = node
        .children()
        .next()
        .map(|first| vec![replace(0, first.before.clone(), with)])
        .unwrap_or_default();
      let index = before_block_string(node).map_or(0, |head| utf16_len(&head)) + 1;
      let report = utf16_offset(tree.source(), node.start, index);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

const OPENING_BRACE_NEWLINE_AFTER: &str = "@stylistic/block-opening-brace-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the opening brace of
  /// blocks.  A comment after the brace is allowed before the newline.
  ///
  /// Equivalent to `@stylistic/block-opening-brace-newline-after`, with its
  /// `ignore: ["rules"]`.
  pub struct StylisticBlockOpeningBraceNewlineAfter => Spacing<StylisticBlockOpeningBraceNewlineAfter> {
    name: OPENING_BRACE_NEWLINE_AFTER,
    description: "Require a newline after opening braces",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticBlockOpeningBraceNewlineAfter {
  const OPTIONS: &'static [&'static str] = &["always", "always-multi-line", "never-multi-line"];
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \"{\""),
    expected_after_multi_line: Some("Expected newline after \"{\" of a multi-line block"),
    rejected_after_multi_line: Some("Unexpected whitespace after \"{\" of a multi-line block"),
    ..Messages::NONE
  };
  /// Whether rules are left out (`ignore: ["rules"]`).
  type Secondary = bool;

  fn secondary(options: &mut Options) -> bool {
    options.keywords("ignore", &["rules"]).contains(&"rules")
  }

  /// The plugin skips comments after the `{`, and when one starts a line
  /// and the node after it does not, it reads that node as starting the
  /// comment's line.  It checks what starts the node and reports just past
  /// the `{`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for node in blocks(tree, !rule.secondary, true, true) {
      let Some((target, before)) = first_after_comments(node) else {
        continue;
      };
      let text = format!("{before}{}", node_string(target));
      let block = block_string(node);
      let at = At::start_of(&text).line_check(&block);
      let Some(message) = rule.checker.after_one_only(at) else {
        continue;
      };
      let edits = match rule.expectation() {
        e if e.is_always() => {
          let fixed = match line_break(before) {
            Some(at) => before[at..].to_string(),
            None => format!("{}{before}", newline(tree)),
          };
          vec![replace(0, target.before.clone(), fixed)]
        }
        Expectation::NeverMultiLine => {
          // Line breaks go from the comments' `raws.before`, and the node
          // starts right after the last of them.
          let mut edits: Vec<Edit> = node
            .children()
            .take_while(|child| child.kind == NodeKind::Comment)
            .map(|comment| {
              let before = comment.before().replace("\r\n", "").replace('\n', "");
              replace(0, comment.before.clone(), before)
            })
            .collect();
          edits.push(replace(0, target.before.clone(), ""));
          edits
        }
        _ => vec![],
      };
      let index = before_block_string(node).map_or(0, |head| utf16_len(&head)) + 1;
      let report = utf16_offset(tree.source(), node.start, index);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

/// The plugin's `nextNode` in `block-opening-brace-newline-after`: the
/// first node of the block that is not a comment, with the `raws.before`
/// it reads for it, which is the comment's before it when the comment
/// starts a line and the node does not.
fn first_after_comments(node: NodeRef<'_>) -> Option<(NodeRef<'_>, &str)> {
  let mut current = node.children().next()?;
  let mut before = current.before();
  loop {
    if current.kind != NodeKind::Comment {
      return Some((current, before));
    }
    let next = current.next()?;
    before = if before.contains('\n') && !next.before().contains('\n') {
      before
    } else {
      next.before()
    };
    current = next;
  }
}

/// Where `/\r?\n/` matches in `text`.
fn line_break(text: &str) -> Option<usize> {
  let at = text.find('\n')?;
  Some(if at > 0 && text.as_bytes()[at - 1] == b'\r' {
    at - 1
  } else {
    at
  })
}

const CLOSING_BRACE_SPACE_BEFORE: &str = "@stylistic/block-closing-brace-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the closing brace
  /// of blocks.
  ///
  /// Equivalent to `@stylistic/block-closing-brace-space-before`.
  pub struct StylisticBlockClosingBraceSpaceBefore => Spacing<StylisticBlockClosingBraceSpaceBefore> {
    name: CLOSING_BRACE_SPACE_BEFORE,
    description: "Require or disallow a space before the closing brace of blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticBlockClosingBraceSpaceBefore {
  const OPTIONS: &'static [&'static str] = SPACE_ANY_LINE;
  const DEFAULT: &'static str = "always-single-line";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \"}\""),
    rejected_before: Some("Unexpected whitespace before \"}\""),
    expected_before_single_line: Some("Expected single space before \"}\" of a single-line block"),
    rejected_before_single_line: Some("Unexpected whitespace before \"}\" of a single-line block"),
    expected_before_multi_line: Some("Expected single space before \"}\" of a multi-line block"),
    rejected_before_multi_line: Some("Unexpected whitespace before \"}\" of a multi-line block"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks what precedes the `}` in the block's string.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for node in blocks(tree, true, true, true) {
      let block = block_string(node);
      let at = At {
        index: block.len() as isize - 1,
        ..At::new(&block, 0)
      };
      let Some(message) = rule.checker.before(at) else {
        continue;
      };
      // `raws.after.replace(/\s*$/, ...)`
      let after = after(node);
      let text = &tree.source()[after.clone()];
      let kept = text.trim_end_matches(is_js_space).len();
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let edits = vec![replace(after.start, kept..text.len(), with)];
      let report = before_closing_brace(node);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

const CLOSING_BRACE_NEWLINE_AFTER: &str = "@stylistic/block-closing-brace-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the closing brace of
  /// blocks.  A comment on the brace's line is allowed before the newline.
  ///
  /// Equivalent to `@stylistic/block-closing-brace-newline-after`, with its
  /// `ignoreAtRules`; gale also takes `"never"`, no line break after the
  /// brace.
  pub struct StylisticBlockClosingBraceNewlineAfter => Spacing<StylisticBlockClosingBraceNewlineAfter> {
    name: CLOSING_BRACE_NEWLINE_AFTER,
    description: "Require or disallow a newline after the closing brace of blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticBlockClosingBraceNewlineAfter {
  const OPTIONS: &'static [&'static str] = &[
    "always",
    "always-single-line",
    "never-single-line",
    "always-multi-line",
    "never-multi-line",
    "never",
  ];
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \"}\""),
    rejected_after: Some("Unexpected newline after \"}\""),
    expected_after_single_line: Some("Expected newline after \"}\" of a single-line block"),
    rejected_after_single_line: Some("Unexpected whitespace after \"}\" of a single-line block"),
    expected_after_multi_line: Some("Expected newline after \"}\" of a multi-line block"),
    rejected_after_multi_line: Some("Unexpected whitespace after \"}\" of a multi-line block"),
    ..Messages::NONE
  };
  type Secondary = Matchers;

  fn secondary(options: &mut Options) -> Matchers {
    options.matchers("ignoreAtRules")
  }

  /// The plugin checks what starts the next node, past a comment on the
  /// brace's line and a stray `;`, and reports just past the `}`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    // Where the fixes for rules put a line break: Stylelint fixes the rules
    // before it checks the at-rules, and a single-line at-rule block that
    // gains a line break is no longer single-line by then.
    let mut broken: Vec<usize> = Vec::new();
    for node in blocks(tree, true, true, false) {
      if node.kind == NodeKind::AtRule && rule.secondary.matches(&node.name) {
        continue;
      }
      let Some(next) = node.next() else {
        continue;
      };
      let comment_on_line = next.kind == NodeKind::Comment
        && next.before().bytes().all(|b| b == b' ')
        && !node_string(next).contains('\n');
      let Some(target) = (if comment_on_line {
        next.next()
      } else {
        Some(next)
      }) else {
        continue;
      };
      let string = node_string(node);
      let mut report = string.len();
      let text = format!("{}{}", target.before(), node_string(target));
      let text = match text.strip_prefix(';') {
        Some(rest) => {
          report += 1;
          rest.to_string()
        }
        None => text,
      };
      let block = block_string(node);
      let at = At::start_of(&text).line_check(&block);
      let Some(message) = rule.checker.after_one_only(at) else {
        continue;
      };
      let before = target.before();
      let edits = match rule.expectation() {
        Expectation::AlwaysSingleLine
          if node.kind == NodeKind::AtRule
            && broken.iter().any(|at| (node.start..node.end).contains(at)) =>
        {
          vec![]
        }
        e if e.is_always() => {
          if node.kind == NodeKind::Rule && line_break(before).is_none() {
            broken.push(target.before.start);
          }
          let fixed = match line_break(before) {
            Some(at) => before[at..].to_string(),
            None => format!("{}{before}", newline(tree)),
          };
          vec![replace(0, target.before.clone(), fixed)]
        }
        Expectation::NeverSingleLine | Expectation::NeverMultiLine => {
          vec![replace(0, target.before.clone(), "")]
        }
        _ => vec![],
      };
      let report = index_in(tree.source(), node.start, &string, report);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

const CLOSING_BRACE_NEWLINE_BEFORE: &str = "@stylistic/block-closing-brace-newline-before";

declare_rule! {
  /// Require a newline or disallow whitespace before the closing brace of
  /// blocks.
  ///
  /// Equivalent to `@stylistic/block-closing-brace-newline-before`; gale
  /// also takes `"never"`, no line break before the brace.
  pub struct StylisticBlockClosingBraceNewlineBefore => Spacing<StylisticBlockClosingBraceNewlineBefore> {
    name: CLOSING_BRACE_NEWLINE_BEFORE,
    description: "Require or disallow a newline before the closing brace of blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticBlockClosingBraceNewlineBefore {
  const OPTIONS: &'static [&'static str] =
    &["always", "always-multi-line", "never-multi-line", "never"];
  const DEFAULT: &'static str = "always-multi-line";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected newline before \"}\""),
    rejected_before: Some("Unexpected newline before \"}\""),
    expected_before_multi_line: Some("Expected newline before \"}\" of a multi-line block"),
    rejected_before_multi_line: Some("Unexpected whitespace before \"}\" of a multi-line block"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin reads `raws.after`, its first run of semicolons left out,
  /// itself rather than through the checker: whether a line break starts
  /// it, and whether it is empty.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let messages = Self::MESSAGES;
    let mut problems = Vec::new();
    for node in blocks(tree, true, true, true) {
      let span = after(node);
      let raw_after = &tree.source()[span.clone()];
      let after = without_first_semicolons(raw_after);
      let multi_line = !is_single_line(&block_string(node));
      let starts_line = after.starts_with('\n') || after.starts_with("\r\n");
      let expectation = rule.expectation();
      let mut complain = |message: Option<&'static str>| {
        let Some(message) = message else {
          return;
        };
        let edits = closing_newline_fix(expectation, tree, span.clone(), raw_after);
        let report = before_closing_brace(node);
        problems.push(rule.problem_fixed_by(tree.source(), message.to_string(), report, edits));
      };
      if !starts_line {
        match expectation {
          Expectation::Always => complain(messages.expected_before),
          Expectation::AlwaysMultiLine if multi_line => {
            complain(messages.expected_before_multi_line)
          }
          _ => {}
        }
      }
      if !after.is_empty() && multi_line && expectation == Expectation::NeverMultiLine {
        complain(messages.rejected_before_multi_line);
      }
      // Gale's own `"never"`: no line break before the `}`, spaces and tabs
      // aside.
      if expectation == Expectation::Never
        && raw_after
          .trim_end_matches([' ', '\t'])
          .ends_with(['\n', '\r'])
      {
        complain(messages.rejected_before);
      }
    }
    problems
  }
}

/// `after.replace(/;+/, '')`: `after` without its first run of semicolons.
fn without_first_semicolons(after: &str) -> String {
  match after.find(';') {
    Some(start) => {
      let end = start + after[start..].bytes().take_while(|&b| b == b';').count();
      format!("{}{}", &after[..start], &after[end..])
    }
    None => after.to_string(),
  }
}

/// The fix of `block-closing-brace-newline-before`, on `raws.after`: for an
/// `always` expectation, a line break before the whitespace after its
/// first non-space characters, or the line break that whitespace already
/// holds; for `never-multi-line`, no whitespace at all.
fn closing_newline_fix(
  expectation: Expectation,
  tree: &PostcssTree,
  span: std::ops::Range<usize>,
  after: &str,
) -> Vec<Edit> {
  let fixed = match expectation {
    // A custom property's value can end the block, taking the whitespace
    // before the `}` in: a line break the fix adds goes there, and the
    // problem stays.  Stylelint adds it once; gale adds it unless one is
    // there.
    e if e.is_always()
      && after.is_empty()
      && tree.source()[..span.start]
        .trim_end_matches([' ', '\t'])
        .ends_with('\n') =>
    {
      return vec![];
    }
    e if e.is_always() => {
      let first_space = after.find(is_js_space).unwrap_or(after.len());
      let (head, tail) = after.split_at(first_space);
      match line_break(tail) {
        Some(at) => format!("{head}{}", &tail[at..]),
        None => format!("{head}{}{tail}", newline(tree)),
      }
    }
    Expectation::NeverMultiLine => after.chars().filter(|&c| !is_js_space(c)).collect(),
    _ => return vec![],
  };
  vec![replace(0, span, fixed)]
}

#[cfg(test)]
#[path = "cases/brace.rs"]
mod cases;

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  /// `(line, column, message)` of each problem.
  fn problems(rule: &str, option: serde_json::Value, source: &str) -> Vec<(usize, usize, String)> {
    let index = gale_diagnostics::SourceLineIndex::build(source);
    let mut found: Vec<_> = lint(rule, option, source, Syntax::Css)
      .into_iter()
      .map(|d| {
        let (line, column) = index.offset_to_location(d.span.offset);
        (line, column, d.message)
      })
      .collect();
    found.sort();
    found
  }

  #[test]
  fn opening_brace_space_before() {
    let rule = "@stylistic/block-opening-brace-space-before";
    let source = "a{ color: red; }\n@media print  { b { top: 0; } }\nc {}";
    assert_eq!(
      problems(rule, json!("always"), source),
      [
        (1, 1, "Expected single space before \"{\"".into()),
        (2, 14, "Expected single space before \"{\"".into()),
      ]
    );
    assert_eq!(
      fix(rule, json!("always"), source, Syntax::Css),
      "a { color: red; }\n@media print { b { top: 0; } }\nc {}"
    );
    assert_eq!(
      problems(
        rule,
        json!(["always", { "ignoreAtRules": ["media"] }]),
        source
      ),
      [(1, 1, "Expected single space before \"{\"".into())]
    );
  }

  #[test]
  fn opening_brace_after() {
    let rule = "@stylistic/block-opening-brace-space-after";
    assert_eq!(
      fix(
        rule,
        json!("always"),
        "a {color: red; }\nb {  top: 0; }",
        Syntax::Css
      ),
      "a { color: red; }\nb { top: 0; }"
    );
    let newline = "@stylistic/block-opening-brace-newline-after";
    let source = "a { /* c */\n  color: red; }\nb {\n  /* c */ top: 0; }\nc { left: 0; }";
    assert_eq!(
      problems(newline, json!("always"), source),
      [(5, 4, "Expected newline after \"{\"".into())]
    );
    assert_eq!(
      fix(newline, json!("always"), "a { color: red; }", Syntax::Css),
      "a {\n color: red; }"
    );
  }

  #[test]
  fn closing_brace() {
    let before = "@stylistic/block-closing-brace-space-before";
    assert_eq!(
      fix(
        before,
        json!("always"),
        "a { color: red;}\nb { top: 0;  }",
        Syntax::Css
      ),
      "a { color: red; }\nb { top: 0; }"
    );
    let after = "@stylistic/block-closing-brace-newline-after";
    let source = "a { color: red; } b {} /* c */\nc {}";
    assert_eq!(
      problems(after, json!("always"), source),
      [(1, 18, "Expected newline after \"}\"".into())]
    );
    assert_eq!(
      fix(after, json!("always"), source, Syntax::Css),
      "a { color: red; }\n b {} /* c */\nc {}"
    );
    let newline_before = "@stylistic/block-closing-brace-newline-before";
    assert_eq!(
      problems(
        newline_before,
        json!("always-multi-line"),
        "a {\n  color: red; }\nb { top: 0; }"
      ),
      [(
        2,
        14,
        "Expected newline before \"}\" of a multi-line block".into()
      )]
    );
    assert_eq!(
      fix(
        newline_before,
        json!("always"),
        "a { color: red; }",
        Syntax::Css
      ),
      "a { color: red;\n }"
    );
  }

  /// A custom property that ends the block takes the whitespace before the
  /// `}` into its value, so the line break the plugin adds stays a problem:
  /// gale adds it once, as Stylelint's one run does.
  #[test]
  fn the_custom_property_fix_settles() {
    use crate::rules::spacing::harness::{bad, check};
    check(
      "@stylistic/block-closing-brace-newline-before",
      json!("always"),
      Syntax::Css,
      &[bad("a { --x: 1 }")
        .at(1, 11, "Expected newline before \"}\"")
        .fixed("a { --x: 1 \n}")],
    );
  }
}
