//! The commas of lists: selector lists, value lists, function arguments and
//! media query lists, found as the plugin's
//! `selectorListCommaWhitespaceChecker`, `valueListCommaWhitespaceChecker`,
//! `functionCommaSpaceChecker` and `mediaQueryListCommaWhitespaceChecker`
//! find them.

use std::borrow::Cow;
use std::ops::Range;

use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_css_parser::value_parser::{self, NodeKind, ValueNode};
use gale_diagnostics::Diagnostic;

use super::{
  Text, declaration_string, declaration_value_index, index_in, space_after, space_before,
};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::standard_syntax::{is_standard_syntax_function, is_standard_syntax_property};
use crate::whitespace::{
  At, Expectation, Match, Messages, NEWLINE_MULTI_LINE, SPACE_SINGLE_LINE, SearchOptions, Spacing,
  SpacingRule, Target, is_horizontal_space, is_js_space, newline, replace, space_run_after,
  space_run_before, style_search,
};

/// The newline rules' options, with gale's own `"never"`: no line break
/// next to the comma.
const NEWLINE_OR_NEVER: &[&str] = &["always", "always-multi-line", "never-multi-line", "never"];

/// The plugin's `selectorListCommaWhitespaceChecker`: the commas of every
/// standard rule's selector as written, outside function arguments.
fn selector_list_commas(tree: &PostcssTree, mut each: impl FnMut(&Text, &Match)) {
  for rule in tree.walk_rules() {
    if !rule.is_standard_syntax() {
      continue;
    }
    let selector = rule.raw_selector();
    let text = Text {
      text: &selector,
      report_base: rule.start,
      fix_base: rule.selector_fixable().then(|| rule.selector_start()),
    };
    let options = SearchOptions::new(",").skip_function_arguments();
    style_search(&selector, &options, |m| each(&text, m));
  }
}

/// The plugin's `valueListCommaWhitespaceChecker`: the commas of every
/// standard declaration's string, outside function arguments.
fn value_list_commas(tree: &PostcssTree, mut each: impl FnMut(&Text, &Match, DeclRef)) {
  for decl in tree.walk_decls() {
    if !decl.is_standard_syntax() || !is_standard_syntax_property(decl.prop()) {
      continue;
    }
    let string = declaration_string(decl);
    let text = Text {
      text: &string,
      report_base: decl.start,
      fix_base: matches!(string, Cow::Borrowed(_)).then(|| decl.prop_start()),
    };
    let options = SearchOptions::new(",").skip_function_arguments();
    style_search(&string, &options, |m| each(&text, m, decl));
  }
}

/// A comma between the arguments of a function, as the plugin's
/// `functionCommaSpaceChecker` checks it.
struct FunctionComma<'v, 'a> {
  /// The function's arguments between its parentheses, with the first
  /// comment taken out: the text the checker reads.
  arguments: &'a str,
  /// Where the comma is in `arguments`.
  index: usize,
  /// The source offset the problem is reported at.
  report: usize,
  /// The function's arguments, and which of them is the comma.
  nodes: &'a [ValueNode<'v>],
  node_index: usize,
  /// The whitespace before the function's `)`.
  closing_space: &'v str,
  /// Where the declaration's value starts in the source, when it is the
  /// value as written.
  fix_base: Option<usize>,
}

/// Remove the first match of the plugin's `COMMENT_PATTERN`,
/// `/ *\/(?:\*.*?\*\/(?!\S)|\/.*)/u`: a comment and the spaces before it,
/// where a `/* */` comment is followed by whitespace or the end, and a
/// `//` one runs to the end of the line.
fn without_first_comment(text: &str) -> Cow<'_, str> {
  let bytes = text.as_bytes();
  let line_end = |from: usize| {
    text[from..]
      .find(['\n', '\r', '\u{2028}', '\u{2029}'])
      .map_or(text.len(), |at| from + at)
  };
  for slash in (0..bytes.len()).filter(|&i| bytes[i] == b'/') {
    let end = match bytes.get(slash + 1) {
      Some(b'/') => Some(line_end(slash + 2)),
      Some(b'*') => {
        // `.*?` stops at the first `*/` followed by whitespace or the end,
        // on the same line.
        let limit = line_end(slash + 2);
        let mut from = slash + 2;
        let mut end = None;
        while let Some(at) = text.get(from..limit).and_then(|rest| rest.find("*/")) {
          let close = from + at + 2;
          if text[close..].chars().next().is_none_or(is_js_space) {
            end = Some(close);
            break;
          }
          from += at + 1;
        }
        end
      }
      _ => None,
    };
    if let Some(end) = end {
      let start = slash - text[..slash].len() + text[..slash].trim_end_matches(' ').len();
      return Cow::Owned(format!("{}{}", &text[..start], &text[end..]));
    }
  }
  Cow::Borrowed(text)
}

/// The plugin's `functionCommaSpaceChecker`: the commas between the
/// arguments of every standard function in every declaration's value,
/// except `url()`.
fn function_commas(tree: &PostcssTree, mut each: impl FnMut(&FunctionComma)) {
  for decl in tree.walk_decls() {
    let source = tree.source();
    let value = decl.raw_value();
    let fix_base = matches!(value, Cow::Borrowed(_)).then(|| decl.value_start());
    let value_base = decl.start + declaration_value_index(decl);
    let nodes = value_parser::parse(&value);
    value_parser::walk(&nodes, &mut |function| {
      if function.kind != NodeKind::Function
        || !is_standard_syntax_function(function)
        || function.value.eq_ignore_ascii_case("url")
      {
        return true;
      }
      let strings: Vec<String> = function.nodes.iter().map(ValueNode::to_css).collect();
      let arguments = format!("{}{}{}", function.before, strings.concat(), function.after);
      let arguments = without_first_comment(&arguments);
      for (node_index, comma) in function.nodes.iter().enumerate() {
        if !comma.is_comma() {
          continue;
        }
        let before_comma = format!(
          "{}{}{}",
          function.before,
          strings[..node_index].concat(),
          comma.before
        );
        each(&FunctionComma {
          arguments: &arguments,
          index: without_first_comment(&before_comma).len(),
          report: index_in(
            source,
            value_base,
            &value,
            comma.source_index + comma.before.len(),
          ),
          nodes: &function.nodes,
          node_index,
          closing_space: function.after,
          fix_base,
        });
      }
      true
    });
  }
}

/// Which side of a function's comma a fix rewrites.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommaSide {
  Before,
  After,
}

/// The plugin's `functionCommaSpaceFix`: the whitespace on `side` of the
/// comma becomes `with` for an `always` expectation; for a `never` one it
/// goes, along with the space nodes after the comma (past any comments).
fn function_comma_fix<R: SpacingRule>(
  rule: &Spacing<R>,
  comma: &FunctionComma,
  side: CommaSide,
  with: &str,
) -> Vec<gale_diagnostics::Edit> {
  let Some(base) = comma.fix_base else {
    return vec![];
  };
  let div = &comma.nodes[comma.node_index];
  let before = div.source_index..div.source_index + div.before.len();
  let after_start = before.end + div.value.len();
  let span: Range<usize> = match side {
    CommaSide::Before => before,
    CommaSide::After => after_start..after_start + div.after.len(),
  };
  if rule.expectation().is_always() {
    // After a trailing comma the whitespace is the closing parenthesis's,
    // so the plugin's fix adds `with` in front of it and leaves the problem
    // in place.  Stylelint fixes once; gale fixes until nothing changes, so
    // it adds `with` only where it is not already there.
    let trailing = side == CommaSide::After && comma.node_index + 1 == comma.nodes.len();
    if trailing && comma.closing_space.starts_with(with) {
      return vec![];
    }
    return vec![replace(base, span, with)];
  }
  if !rule.expectation().is_never() {
    return vec![];
  }
  let mut edits = vec![replace(base, span, "")];
  for node in &comma.nodes[comma.node_index + 1..] {
    match node.kind {
      NodeKind::Comment => continue,
      NodeKind::Space => edits.push(replace(base, node.source_index..node.source_end_index, "")),
      _ => break,
    }
  }
  edits
}

/// The plugin's `mediaQueryListCommaWhitespaceChecker`: the commas of every
/// `@media` at-rule's params as written.  With `allow_trailing_comments`,
/// the index moves past comments on the comma's line, so that the check
/// looks at what follows them.
fn media_query_list_commas(
  tree: &PostcssTree,
  allow_trailing_comments: bool,
  mut each: impl FnMut(&Text, usize),
) {
  for at in tree.walk_at_rules() {
    if !at.name().eq_ignore_ascii_case("media") {
      continue;
    }
    let params = at.raw_params();
    let text = Text {
      text: &params,
      report_base: at.start + at.param_index(),
      fix_base: at.params_fixable().then(|| at.params_start()),
    };
    style_search(&params, &SearchOptions::new(","), |m| {
      let mut index = m.start;
      if allow_trailing_comments {
        while let Some(len) = block_comment_on_line(&params[index + 1..]) {
          index += len;
        }
        if let Some(len) = line_comment(&params[index + 1..]) {
          index += len;
        }
      }
      each(&text, index);
    });
  }
}

/// The length of `/^[^\S\r\n]*\/\*([\s\S]*?)\*\//`'s match in `text`.
fn block_comment_on_line(text: &str) -> Option<usize> {
  let comment = text.trim_start_matches(is_horizontal_space);
  let body = comment.strip_prefix("/*")?;
  let close = body.find("*/")?;
  Some(text.len() - comment.len() + 2 + close + 2)
}

/// The length of the first group of `/^([^\S\r\n]*\/\/[\s\S]*?)\r?\n/` in
/// `text`: a `//` comment up to the line break.
fn line_comment(text: &str) -> Option<usize> {
  let comment = text.trim_start_matches(is_horizontal_space);
  if !comment.starts_with("//") {
    return None;
  }
  let newline = text.find('\n')?;
  let end = if newline > 0 && text.as_bytes()[newline - 1] == b'\r' {
    newline - 1
  } else {
    newline
  };
  (end >= text.len() - comment.len() + 2).then_some(end)
}

/// Whether `text` starts with whitespace and then `marker`
/// (`/^\s+\/\//`, `/^\s+\/\*/`).
fn spaced_comment(text: &str, marker: &str) -> bool {
  let rest = text.trim_start_matches(is_js_space);
  rest.len() < text.len() && rest.starts_with(marker)
}

/// Whether `text` starts with spaces or tabs and then `marker`
/// (`/^[ \t]*\/\//`, `/^[ \t]*\/\*/`).
fn tabbed_comment(text: &str, marker: &str) -> bool {
  text.trim_start_matches([' ', '\t']).starts_with(marker)
}

/// The index of the `/` that ends the first `*/` after `from`, as
/// `text.indexOf('*/', from) + 1` gives it (0 when there is none).
fn comment_end(text: &str, from: usize) -> usize {
  text
    .get(from..)
    .and_then(|rest| rest.find("*/"))
    .map_or(0, |at| from + at + 1)
}

/// The fix of the `*-newline-after` list rules: a line break after
/// `text[index]` for an `always` expectation, no whitespace there for
/// `never-multi-line`.
fn newline_after_fix<R: SpacingRule>(
  rule: &Spacing<R>,
  tree: &PostcssTree,
  text: &str,
  base: usize,
  index: usize,
) -> Vec<gale_diagnostics::Edit> {
  match rule.expectation() {
    e if e.is_always() => vec![replace(base, index + 1..index + 1, newline(tree))],
    Expectation::NeverMultiLine => vec![replace(base, space_run_after(text, index + 1), "")],
    _ => vec![],
  }
}

/// The fix of the `*-newline-before` list rules: a line break before the
/// whitespace before `text[index]` for an `always` expectation, no
/// whitespace there for `never-multi-line`.
fn newline_before_fix<R: SpacingRule>(
  rule: &Spacing<R>,
  tree: &PostcssTree,
  text: &str,
  base: usize,
  index: usize,
) -> Vec<gale_diagnostics::Edit> {
  let run = space_run_before(text, index);
  match rule.expectation() {
    e if e.is_always() => vec![replace(base, run.start..run.start, newline(tree))],
    Expectation::NeverMultiLine => vec![replace(base, run, "")],
    _ => vec![],
  }
}

const SELECTOR_LIST_COMMA_SPACE_AFTER: &str = "@stylistic/selector-list-comma-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the commas of
  /// selector lists.
  ///
  /// Equivalent to `@stylistic/selector-list-comma-space-after`.
  pub struct StylisticSelectorListCommaSpaceAfter => Spacing<StylisticSelectorListCommaSpaceAfter> {
    name: SELECTOR_LIST_COMMA_SPACE_AFTER,
    description: "Require or disallow a space after the commas of selector lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorListCommaSpaceAfter {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "always-single-line";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \",\""),
    rejected_after: Some("Unexpected whitespace after \",\""),
    expected_after_single_line: Some("Expected single space after \",\" in a single-line list"),
    rejected_after_single_line: Some("Unexpected whitespace after \",\" in a single-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    selector_list_commas(tree, |text, m| {
      problems.extend(space_after(rule, tree.source(), text, m.start));
    });
    problems
  }
}

const SELECTOR_LIST_COMMA_SPACE_BEFORE: &str = "@stylistic/selector-list-comma-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the commas of
  /// selector lists.
  ///
  /// Equivalent to `@stylistic/selector-list-comma-space-before`.
  pub struct StylisticSelectorListCommaSpaceBefore => Spacing<StylisticSelectorListCommaSpaceBefore> {
    name: SELECTOR_LIST_COMMA_SPACE_BEFORE,
    description: "Require or disallow a space before the commas of selector lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorListCommaSpaceBefore {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \",\""),
    rejected_before: Some("Unexpected whitespace before \",\""),
    expected_before_single_line: Some("Expected single space before \",\" in a single-line list"),
    rejected_before_single_line: Some("Unexpected whitespace before \",\" in a single-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    selector_list_commas(tree, |text, m| {
      problems.extend(space_before(rule, tree.source(), text, m.start));
    });
    problems
  }
}

const SELECTOR_LIST_COMMA_NEWLINE_AFTER: &str = "@stylistic/selector-list-comma-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the commas of selector
  /// lists.  A comment after the comma is allowed before the newline.
  ///
  /// Equivalent to `@stylistic/selector-list-comma-newline-after`; gale also
  /// takes `"never"`, no line break after the comma.
  pub struct StylisticSelectorListCommaNewlineAfter => Spacing<StylisticSelectorListCommaNewlineAfter> {
    name: SELECTOR_LIST_COMMA_NEWLINE_AFTER,
    description: "Require a newline or disallow whitespace after the commas of selector lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorListCommaNewlineAfter {
  const OPTIONS: &'static [&'static str] = NEWLINE_OR_NEVER;
  const DEFAULT: &'static str = "always-multi-line";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \",\""),
    rejected_after: Some("Unexpected newline after \",\""),
    expected_after_multi_line: Some("Expected newline after \",\" in a multi-line list"),
    rejected_after_multi_line: Some("Unexpected whitespace after \",\" in a multi-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    selector_list_commas(tree, |text, m| {
      let next = &text.text[m.end..];
      // A `//` comment ends with a newline.
      if spaced_comment(next, "//") {
        return;
      }
      let index = if spaced_comment(next, "/*") {
        comment_end(text.text, m.end)
      } else {
        m.start
      };
      let Some(message) = rule.checker.after_one_only(At::new(text.text, index)) else {
        return;
      };
      let edits = text.edits(|base| newline_after_fix(rule, tree, text.text, base, index));
      problems.push(rule.problem_fixed_by(
        tree.source(),
        message,
        text.report(tree.source(), m.start),
        edits,
      ));
    });
    problems
  }
}

const SELECTOR_LIST_COMMA_NEWLINE_BEFORE: &str = "@stylistic/selector-list-comma-newline-before";

declare_rule! {
  /// Require a newline or disallow whitespace before the commas of selector
  /// lists.
  ///
  /// Equivalent to `@stylistic/selector-list-comma-newline-before`; gale
  /// also takes `"never"`, no line break before the comma.
  pub struct StylisticSelectorListCommaNewlineBefore => Spacing<StylisticSelectorListCommaNewlineBefore> {
    name: SELECTOR_LIST_COMMA_NEWLINE_BEFORE,
    description: "Require a newline or disallow whitespace before the commas of selector lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorListCommaNewlineBefore {
  const OPTIONS: &'static [&'static str] = NEWLINE_OR_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected newline before \",\""),
    rejected_before: Some("Unexpected whitespace before \",\""),
    expected_before_multi_line: Some("Expected newline before \",\" in a multi-line list"),
    rejected_before_multi_line: Some("Unexpected whitespace before \",\" in a multi-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    selector_list_commas(tree, |text, m| {
      let at = At::new(text.text, m.start);
      let Some(message) = rule.checker.before_allowing_indentation(at) else {
        return;
      };
      let edits = text.edits(|base| newline_before_fix(rule, tree, text.text, base, m.start));
      problems.push(rule.problem_fixed_by(
        tree.source(),
        message,
        text.report(tree.source(), m.start),
        edits,
      ));
    });
    problems
  }
}

const VALUE_LIST_COMMA_SPACE_AFTER: &str = "@stylistic/value-list-comma-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the commas of
  /// value lists.
  ///
  /// Equivalent to `@stylistic/value-list-comma-space-after`.
  pub struct StylisticValueListCommaSpaceAfter => Spacing<StylisticValueListCommaSpaceAfter> {
    name: VALUE_LIST_COMMA_SPACE_AFTER,
    description: "Require or disallow a space after the commas of value lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticValueListCommaSpaceAfter {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \",\""),
    rejected_after: Some("Unexpected whitespace after \",\""),
    expected_after_single_line: Some("Expected single space after \",\" in a single-line list"),
    rejected_after_single_line: Some("Unexpected whitespace after \",\" in a single-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    value_list_commas(tree, |text, m, decl| {
      // The plugin fixes commas in the value only.
      let text = Text {
        fix_base: text
          .fix_base
          .filter(|_| m.start > declaration_value_index(decl)),
        ..*text
      };
      problems.extend(space_after(rule, tree.source(), &text, m.start));
    });
    problems
  }
}

const VALUE_LIST_COMMA_SPACE_BEFORE: &str = "@stylistic/value-list-comma-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the commas of
  /// value lists.
  ///
  /// Equivalent to `@stylistic/value-list-comma-space-before`.
  pub struct StylisticValueListCommaSpaceBefore => Spacing<StylisticValueListCommaSpaceBefore> {
    name: VALUE_LIST_COMMA_SPACE_BEFORE,
    description: "Require or disallow a space before the commas of value lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticValueListCommaSpaceBefore {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \",\""),
    rejected_before: Some("Unexpected whitespace before \",\""),
    // Sic: the plugin words both single-line problems the same.
    expected_before_single_line: Some("Unexpected whitespace before \",\" in a single-line list"),
    rejected_before_single_line: Some("Unexpected whitespace before \",\" in a single-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    value_list_commas(tree, |text, m, decl| {
      let text = Text {
        fix_base: text
          .fix_base
          .filter(|_| m.start > declaration_value_index(decl)),
        ..*text
      };
      problems.extend(space_before(rule, tree.source(), &text, m.start));
    });
    problems
  }
}

const VALUE_LIST_COMMA_NEWLINE_AFTER: &str = "@stylistic/value-list-comma-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the commas of value
  /// lists.  A comment after the comma is allowed before the newline.
  ///
  /// Equivalent to `@stylistic/value-list-comma-newline-after`; gale also
  /// takes `"never"`, no line break after the comma.
  pub struct StylisticValueListCommaNewlineAfter => Spacing<StylisticValueListCommaNewlineAfter> {
    name: VALUE_LIST_COMMA_NEWLINE_AFTER,
    description: "Require a newline or disallow whitespace after the commas of value lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticValueListCommaNewlineAfter {
  const OPTIONS: &'static [&'static str] = NEWLINE_OR_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \",\""),
    rejected_after: Some("Unexpected whitespace after \",\""),
    expected_after_multi_line: Some("Expected newline after \",\" in a multi-line list"),
    rejected_after_multi_line: Some("Unexpected whitespace after \",\" in a multi-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    value_list_commas(tree, |text, m, decl| {
      let next = &text.text[m.end..];
      // A `//` comment ends with a newline.
      if tabbed_comment(next, "//") {
        return;
      }
      let index = if tabbed_comment(next, "/*") {
        comment_end(text.text, m.end)
      } else {
        m.start
      };
      let Some(message) = rule.checker.after_one_only(At::new(text.text, index)) else {
        return;
      };
      let edits = match text.fix_base {
        Some(base) if index > declaration_value_index(decl) => {
          newline_after_fix(rule, tree, text.text, base, index)
        }
        _ => vec![],
      };
      problems.push(rule.problem_fixed_by(
        tree.source(),
        message,
        text.report(tree.source(), index),
        edits,
      ));
    });
    problems
  }
}

const VALUE_LIST_COMMA_NEWLINE_BEFORE: &str = "@stylistic/value-list-comma-newline-before";

declare_rule! {
  /// Require a newline or disallow whitespace before the commas of value
  /// lists.
  ///
  /// Equivalent to `@stylistic/value-list-comma-newline-before`, which does
  /// not fix; gale also takes `"never"`, no line break before the comma.
  pub struct StylisticValueListCommaNewlineBefore => Spacing<StylisticValueListCommaNewlineBefore> {
    name: VALUE_LIST_COMMA_NEWLINE_BEFORE,
    description: "Require a newline or disallow whitespace before the commas of value lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticValueListCommaNewlineBefore {
  const OPTIONS: &'static [&'static str] = NEWLINE_OR_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected newline before \",\""),
    rejected_before: Some("Unexpected whitespace before \",\""),
    expected_before_multi_line: Some("Expected newline before \",\" in a multi-line list"),
    rejected_before_multi_line: Some("Unexpected whitespace before \",\" in a multi-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    value_list_commas(tree, |text, m, _| {
      let at = At::new(text.text, m.start);
      if let Some(message) = rule.checker.before_allowing_indentation(at) {
        problems.push(rule.problem(tree.source(), message, text.report(tree.source(), m.start)));
      }
    });
    problems
  }
}

/// `ignoreFunctions`, which newer releases of the plugin take: accepted,
/// not yet applied.
fn ignore_functions(options: &mut Options) {
  options.strings("ignoreFunctions");
}

const FUNCTION_COMMA_SPACE_AFTER: &str = "@stylistic/function-comma-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the commas of
  /// functions.
  ///
  /// Equivalent to `@stylistic/function-comma-space-after`.
  /// `ignoreFunctions` is accepted but not yet applied.
  pub struct StylisticFunctionCommaSpaceAfter => Spacing<StylisticFunctionCommaSpaceAfter> {
    name: FUNCTION_COMMA_SPACE_AFTER,
    description: "Require or disallow a space after the commas of functions",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticFunctionCommaSpaceAfter {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \",\""),
    rejected_after: Some("Unexpected whitespace after \",\""),
    expected_after_single_line: Some("Expected single space after \",\" in a single-line function"),
    rejected_after_single_line: Some("Unexpected whitespace after \",\" in a single-line function"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(options: &mut Options) {
    ignore_functions(options);
  }

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    function_commas(tree, |comma| {
      if let Some(message) = rule.checker.after(At::new(comma.arguments, comma.index)) {
        let edits = function_comma_fix(rule, comma, CommaSide::After, " ");
        problems.push(rule.problem_fixed_by(tree.source(), message, comma.report, edits));
      }
    });
    problems
  }
}

const FUNCTION_COMMA_SPACE_BEFORE: &str = "@stylistic/function-comma-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the commas of
  /// functions.
  ///
  /// Equivalent to `@stylistic/function-comma-space-before`.
  /// `ignoreFunctions` is accepted but not yet applied.
  pub struct StylisticFunctionCommaSpaceBefore => Spacing<StylisticFunctionCommaSpaceBefore> {
    name: FUNCTION_COMMA_SPACE_BEFORE,
    description: "Require or disallow a space before the commas of functions",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticFunctionCommaSpaceBefore {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \",\""),
    rejected_before: Some("Unexpected whitespace before \",\""),
    expected_before_single_line: Some(
      "Expected single space before \",\" in a single-line function",
    ),
    rejected_before_single_line: Some(
      "Unexpected whitespace before \",\" in a single-line function",
    ),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(options: &mut Options) {
    ignore_functions(options);
  }

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    function_commas(tree, |comma| {
      if let Some(message) = rule.checker.before(At::new(comma.arguments, comma.index)) {
        let edits = function_comma_fix(rule, comma, CommaSide::Before, " ");
        problems.push(rule.problem_fixed_by(tree.source(), message, comma.report, edits));
      }
    });
    problems
  }
}

const FUNCTION_COMMA_NEWLINE_AFTER: &str = "@stylistic/function-comma-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the commas of functions.
  ///
  /// Equivalent to `@stylistic/function-comma-newline-after`.
  /// `ignoreFunctions` is accepted but not yet applied.
  pub struct StylisticFunctionCommaNewlineAfter => Spacing<StylisticFunctionCommaNewlineAfter> {
    name: FUNCTION_COMMA_NEWLINE_AFTER,
    description: "Require or disallow a newline after the commas of functions",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticFunctionCommaNewlineAfter {
  const OPTIONS: &'static [&'static str] = NEWLINE_MULTI_LINE;
  const DEFAULT: &'static str = "always-multi-line";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \",\""),
    expected_after_multi_line: Some("Expected newline after \",\" in a multi-line function"),
    rejected_after_multi_line: Some("Unexpected whitespace after \",\" in a multi-line function"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(options: &mut Options) {
    ignore_functions(options);
  }

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    function_commas(tree, |comma| {
      let at = At::new(comma.arguments, comma.index);
      if let Some(message) = rule.checker.after_one_only(at) {
        let edits = function_comma_fix(rule, comma, CommaSide::After, newline(tree));
        problems.push(rule.problem_fixed_by(tree.source(), message, comma.report, edits));
      }
    });
    problems
  }
}

const MEDIA_QUERY_LIST_COMMA_SPACE_AFTER: &str = "@stylistic/media-query-list-comma-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the commas of
  /// media query lists.
  ///
  /// Equivalent to `@stylistic/media-query-list-comma-space-after`.
  pub struct StylisticMediaQueryListCommaSpaceAfter => Spacing<StylisticMediaQueryListCommaSpaceAfter> {
    name: MEDIA_QUERY_LIST_COMMA_SPACE_AFTER,
    description: "Require or disallow a space after the commas of media query lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaQueryListCommaSpaceAfter {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \",\""),
    rejected_after: Some("Unexpected whitespace after \",\""),
    expected_after_single_line: Some("Expected single space after \",\" in a single-line list"),
    rejected_after_single_line: Some("Unexpected whitespace after \",\" in a single-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    media_query_list_commas(tree, false, |text, index| {
      problems.extend(space_after(rule, tree.source(), text, index));
    });
    problems
  }
}

const MEDIA_QUERY_LIST_COMMA_SPACE_BEFORE: &str = "@stylistic/media-query-list-comma-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the commas of
  /// media query lists.
  ///
  /// Equivalent to `@stylistic/media-query-list-comma-space-before`.
  pub struct StylisticMediaQueryListCommaSpaceBefore => Spacing<StylisticMediaQueryListCommaSpaceBefore> {
    name: MEDIA_QUERY_LIST_COMMA_SPACE_BEFORE,
    description: "Require or disallow a space before the commas of media query lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaQueryListCommaSpaceBefore {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \",\""),
    rejected_before: Some("Unexpected whitespace before \",\""),
    expected_before_single_line: Some("Expected single space before \",\" in a single-line list"),
    rejected_before_single_line: Some("Unexpected whitespace before \",\" in a single-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    media_query_list_commas(tree, false, |text, index| {
      problems.extend(space_before(rule, tree.source(), text, index));
    });
    problems
  }
}

const MEDIA_QUERY_LIST_COMMA_NEWLINE_AFTER: &str =
  "@stylistic/media-query-list-comma-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the commas of media
  /// query lists.  With `always`, comments on the comma's line are allowed
  /// before the newline.
  ///
  /// Equivalent to `@stylistic/media-query-list-comma-newline-after`; gale
  /// also takes `"never"`, no line break after the comma.
  pub struct StylisticMediaQueryListCommaNewlineAfter => Spacing<StylisticMediaQueryListCommaNewlineAfter> {
    name: MEDIA_QUERY_LIST_COMMA_NEWLINE_AFTER,
    description: "Require a newline or disallow whitespace after the commas of media query lists",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaQueryListCommaNewlineAfter {
  const OPTIONS: &'static [&'static str] = NEWLINE_OR_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \",\""),
    rejected_after: Some("Unexpected whitespace after \",\""),
    expected_after_multi_line: Some("Expected newline after \",\" in a multi-line list"),
    rejected_after_multi_line: Some("Unexpected whitespace after \",\" in a multi-line list"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    let allow_comments = rule.expectation().is_always();
    media_query_list_commas(tree, allow_comments, |text, index| {
      let Some(message) = rule.checker.after_one_only(At::new(text.text, index)) else {
        return;
      };
      let edits = text.edits(|base| {
        let after = &text.text[index + 1..];
        if rule.expectation().is_always() && after[space_run_after(after, 0)].contains('\n') {
          // A line break follows already: drop the spaces before it.
          let horizontal = after.len() - after.trim_start_matches(is_horizontal_space).len();
          vec![replace(base, index + 1..index + 1 + horizontal, "")]
        } else {
          newline_after_fix(rule, tree, text.text, base, index)
        }
      });
      problems.push(rule.problem_fixed_by(
        tree.source(),
        message,
        text.report(tree.source(), index),
        edits,
      ));
    });
    problems
  }
}

#[cfg(test)]
#[path = "cases/comma.rs"]
mod cases;

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, invalid_options, lint};

  /// `(line, column, message)` of each problem `rule` reports with
  /// `option`.
  fn problems(rule: &str, option: &str, source: &str) -> Vec<(usize, usize, String)> {
    problems_in(rule, option, source, Syntax::Css)
  }

  fn problems_in(
    rule: &str,
    option: &str,
    source: &str,
    syntax: Syntax,
  ) -> Vec<(usize, usize, String)> {
    let index = gale_diagnostics::SourceLineIndex::build(source);
    let mut found: Vec<_> = lint(rule, json!(option), source, syntax)
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
  fn selector_list_commas_are_checked_outside_functions() {
    let rule = "@stylistic/selector-list-comma-space-after";
    assert_eq!(
      problems(rule, "always", "a,b, c:is(d,e),  f {}"),
      [
        (1, 2, "Expected single space after \",\"".into()),
        (1, 15, "Expected single space after \",\"".into()),
      ]
    );
    assert_eq!(
      fix(rule, json!("always"), "a,b,  c {}", Syntax::Css),
      "a, b, c {}"
    );
    assert_eq!(
      problems(rule, "always-single-line", "a,\nb {}"),
      [],
      "a multi-line selector is not single-line"
    );
    let before = "@stylistic/selector-list-comma-space-before";
    assert_eq!(
      fix(before, json!("never"), "a , b\t,c {}", Syntax::Css),
      "a, b,c {}"
    );
  }

  #[test]
  fn selector_list_newlines_allow_comments_and_indentation() {
    let after = "@stylistic/selector-list-comma-newline-after";
    assert_eq!(
      problems(after, "always", "a, /* x */\nb, c {}"),
      [(2, 2, "Expected newline after \",\"".into())]
    );
    assert_eq!(
      fix(after, json!("always"), "a, b,\nc {}", Syntax::Css),
      "a,\n b,\nc {}"
    );
    let before = "@stylistic/selector-list-comma-newline-before";
    assert_eq!(problems(before, "always", "a\n  , b {}"), []);
    assert_eq!(
      fix(before, json!("always"), "a , b {}", Syntax::Css),
      "a\n , b {}"
    );
    // Gale's own `"never"`: no line break before the comma.
    assert_eq!(
      problems(before, "never", "a\n, b , c {}"),
      [(2, 1, "Unexpected whitespace before \",\"".into())]
    );
  }

  #[test]
  fn value_list_commas_report_and_fix_in_the_value() {
    let rule = "@stylistic/value-list-comma-space-after";
    assert_eq!(
      problems(
        rule,
        "always",
        "a { margin: 1px,2px; font: a, rgb(0,0,0),b; }"
      ),
      [
        (1, 16, "Expected single space after \",\"".into()),
        (1, 41, "Expected single space after \",\"".into()),
      ]
    );
    assert_eq!(
      fix(rule, json!("never"), "a { font: a,  b , c; }", Syntax::Css),
      "a { font: a,b ,c; }"
    );
    let newline = "@stylistic/value-list-comma-newline-after";
    assert_eq!(
      problems(
        newline,
        "always",
        "a { font: a, /* x */\n b, // y\n c, d; }"
      ),
      [(3, 3, "Expected newline after \",\"".into())]
    );
    // postcss-scss writes the `//` comment as `/* y*/`, two characters
    // longer, and Stylelint reports that many columns further on.
    assert_eq!(
      problems_in(
        newline,
        "always",
        "a { font: a, /* x */\n b, // y\n c, d; }",
        Syntax::Scss
      ),
      [(3, 5, "Expected newline after \",\"".into())]
    );
  }

  #[test]
  fn function_commas_skip_url_and_the_first_comment() {
    let rule = "@stylistic/function-comma-space-after";
    assert_eq!(
      problems(
        rule,
        "always",
        "a { b: rgb(0,0, 0); c: url(a,b); d: f(a /* , */,b); }"
      ),
      [
        (1, 13, "Expected single space after \",\"".into()),
        (1, 48, "Expected single space after \",\"".into()),
      ]
    );
    assert_eq!(
      fix(rule, json!("always"), "a { b: rgb(0,0,  0); }", Syntax::Css),
      "a { b: rgb(0, 0, 0); }"
    );
    let before = "@stylistic/function-comma-space-before";
    assert_eq!(
      fix(
        before,
        json!("never"),
        "a { b: rgb(0 ,0 , 0); }",
        Syntax::Css
      ),
      // The plugin's fix keeps the space after the comma.
      "a { b: rgb(0,0, 0); }"
    );
    let newline = "@stylistic/function-comma-newline-after";
    assert_eq!(
      fix(
        newline,
        json!("always-multi-line"),
        "a { b: f(\n0, 1); }",
        Syntax::Css
      ),
      "a { b: f(\n0,\n1); }"
    );
  }

  #[test]
  fn media_query_list_commas_are_in_media_params_only() {
    let rule = "@stylistic/media-query-list-comma-space-after";
    assert_eq!(
      problems(rule, "always", "@media a,b {}\n@supports (a,b) {}"),
      [(1, 9, "Expected single space after \",\"".into())]
    );
    let newline = "@stylistic/media-query-list-comma-newline-after";
    assert_eq!(problems(newline, "always", "@media a, /* x */\nb {}"), []);
    assert_eq!(
      problems(newline, "always", "@media a, /* x */ b {}"),
      [(1, 17, "Expected newline after \",\"".into())]
    );
    assert_eq!(
      fix(
        newline,
        json!("always"),
        "@media a,  \nb, c {}",
        Syntax::Css
      ),
      "@media a,\nb,\n c {}"
    );
  }

  #[test]
  fn the_first_comment_of_function_arguments_is_left_out() {
    use super::without_first_comment;
    assert_eq!(without_first_comment("a /* b */ c /* d */"), "a c /* d */");
    assert_eq!(without_first_comment("a /* b */c"), "a /* b */c");
    assert_eq!(without_first_comment("a  // b\nc"), "a\nc");
    assert_eq!(without_first_comment("a / b"), "a / b");
  }

  #[test]
  fn options_are_validated_like_the_plugin() {
    for (rule, valid) in [
      (
        "@stylistic/selector-list-comma-space-after",
        "never-single-line",
      ),
      ("@stylistic/value-list-comma-newline-before", "never"),
      (
        "@stylistic/function-comma-newline-after",
        "never-multi-line",
      ),
    ] {
      assert!(invalid_options(rule, json!(valid)).is_empty(), "{rule}");
    }
    assert_eq!(
      invalid_options("@stylistic/function-comma-newline-after", json!("never")),
      ["Invalid option value \"never\" for rule \"@stylistic/function-comma-newline-after\""]
    );
    assert!(
      invalid_options(
        "@stylistic/function-comma-space-after",
        json!(["always", { "ignoreFunctions": ["a"] }])
      )
      .is_empty()
    );
  }

  /// Fixes that leave a problem for the same rule: Stylelint fixes once and
  /// gale fixes until nothing changes, which is where a second Stylelint
  /// run ends too.
  #[test]
  fn fixes_settle_where_a_second_stylelint_run_would() {
    use crate::rules::spacing::harness::{bad, check};
    // Joining the lines puts the comment next to the comma; the next pass
    // takes the space after it (Stylelint's first run stops at
    // `,/* x */ inset`).
    check(
      "@stylistic/value-list-comma-newline-after",
      json!("never-multi-line"),
      Syntax::Css,
      &[
        bad("a {\n  box-shadow:\n    0 0 red,\n    /* x */ inset 0 0 blue;\n}")
          .at(
            3,
            12,
            "Unexpected whitespace after \",\" in a multi-line list",
          )
          .fixed("a {\n  box-shadow:\n    0 0 red,/* x */inset 0 0 blue;\n}"),
      ],
    );
    // After a trailing comma the whitespace is the `)`'s: the space the
    // plugin adds stays a problem, so gale adds it once, as Stylelint does.
    check(
      "@stylistic/function-comma-space-after",
      json!("always"),
      Syntax::Css,
      &[bad("a { b: f(a,\n); }")
        .at(1, 11, "Expected single space after \",\"")
        .fixed("a { b: f(a, \n); }")],
    );
    // Without a line break in the file the fixes use `\n`, as Stylelint's
    // `context.newline` does.
    check(
      "@stylistic/selector-list-comma-newline-after",
      json!("always"),
      Syntax::Css,
      &[bad("a,b {}")
        .at(1, 2, "Expected newline after \",\"")
        .fixed("a,\nb {}")],
    );
  }

  /// Gale's own `"never"` on `selector-list-comma-newline-after`, which the
  /// plugin does not take: no line break after the comma, spaces aside.
  #[test]
  fn selector_list_comma_newline_after_never() {
    use crate::rules::spacing::harness::{bad, check, ok};
    check(
      "@stylistic/selector-list-comma-newline-after",
      json!("never"),
      Syntax::Css,
      &[
        ok("a, b {}"),
        ok("a,b {}"),
        bad("a,\nb {}").at(1, 2, "Unexpected newline after \",\""),
        bad("a,  \n  b {}").at(1, 2, "Unexpected newline after \",\""),
      ],
    );
  }
}
