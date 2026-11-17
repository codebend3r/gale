//! Stylelint's configuration comments: `stylelint-disable`,
//! `stylelint-enable`, `stylelint-disable-line` and
//! `stylelint-disable-next-line` (each also spelt `gale-…`).
//!
//! [`Collector`] ports Stylelint's `assignDisabledRanges`.  Like Stylelint it
//! reads the comments PostCSS sees, in document order: comment nodes, and the
//! `/* */` comments inside a rule's selector, an at-rule's params and a
//! declaration's value (plus `//` ones there in SCSS, which `postcss-scss`
//! turns into block comments).  A `//` command followed by `//` comments on
//! the next lines that carry on its description is read as one comment.
//!
//! Ranges are whole lines, as in Stylelint: a `disable` covers its own line
//! (code before the comment included) and every line to the `enable` that
//! closes it, that line included; `disable-line` covers the comment's first
//! line and `disable-next-line` the line after its last.  Every rule keeps
//! its own list of ranges, seeded from the blanket ones when it is first
//! named, and a problem is checked against its rule's list when there is
//! one and the blanket list otherwise.  A plain `enable` ends the open range
//! of every rule, named ones included.
//!
//! [`apply`] then drops the problems those ranges cover and adds Stylelint's
//! reports about the comments themselves.

use std::collections::{HashMap, HashSet};

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::{NodeKind, PostcssTree};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::registry::canonical_name;

/// Stylelint's `RULE_NAME_ALL`: the rule name of the blanket ranges.
pub(crate) const ALL: &str = "all";

/// The prefixes a configuration comment may start with: Stylelint's own
/// and gale's.
const PREFIXES: [&str; 2] = ["stylelint", "gale"];

/// Whether `text` could hold a configuration comment at all, so files
/// without one skip the comment scan.
pub(crate) fn may_have_directives(text: &str) -> bool {
  PREFIXES
    .iter()
    .any(|prefix| text.contains(&format!("{prefix}-")[..]))
}

/// What a configuration comment asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
  /// `-disable`: from this line on.
  Disable,
  /// `-disable-line`: this line only.
  DisableLine,
  /// `-disable-next-line`: the line after the comment.
  DisableNextLine,
  /// `-enable`: back on after this line.
  Enable,
}

/// The command a comment's text starts with, and the length of the word
/// naming it (`stylelint-disable`), or `None` for any other comment.
///
/// Stylelint's `extractConfigurationComment`: the first whitespace-separated
/// word, without its prefix, must be one of the four commands.
fn command(text: &str) -> Option<(Command, usize)> {
  let word = text.split(char::is_whitespace).next().unwrap_or("");
  let rest = PREFIXES
    .iter()
    .find_map(|prefix| word.strip_prefix(prefix))?;
  let command = match rest {
    "-disable" => Command::Disable,
    "-disable-line" => Command::DisableLine,
    "-disable-next-line" => Command::DisableNextLine,
    "-enable" => Command::Enable,
    _ => return None,
  };
  Some((command, word.len()))
}

/// The rules a command names (Stylelint's `getCommandRules`): the text after
/// the command word, up to a ` -- ` description separator, split on commas.
/// No rule at all means every rule, [`ALL`].
fn command_rules(text: &str, word_len: usize) -> Vec<String> {
  let rest = &text[word_len..];
  let rules: Vec<String> = before_description(rest)
    .trim()
    .split(',')
    .filter(|part| !part.is_empty())
    .map(|part| part.trim().to_string())
    .collect();
  if rules.is_empty() {
    vec![ALL.to_string()]
  } else {
    rules
  }
}

/// `text` up to the first `/\s-{2,}\s/`: whitespace, two or more dashes,
/// whitespace.
fn before_description(text: &str) -> &str {
  for (at, c) in text.char_indices() {
    if !c.is_whitespace() {
      continue;
    }
    let after = &text[at + c.len_utf8()..];
    let dashes = after.bytes().take_while(|&b| b == b'-').count();
    if dashes >= 2 && after[dashes..].starts_with(char::is_whitespace) {
      return &text[..at];
    }
  }
  text
}

/// Whether the comment explains itself (Stylelint's `getDescription`): any
/// text after the first `--`.
fn has_description(text: &str) -> bool {
  text
    .find("--")
    .is_some_and(|at| !text[at + 2..].trim().is_empty())
}

/// Whether two rule names name the same rule, one perhaps by an alias.
fn same_rule(a: &str, b: &str) -> bool {
  a == b || canonical_name(a) == canonical_name(b)
}

// ---------------------------------------------------------------------------
// Ranges
// ---------------------------------------------------------------------------

/// The PostCSS node a range comes from (Stylelint's `range.node`), which
/// reports about the comment point at: the comment itself (with the `//`
/// comments merged into it), or the rule, at-rule or declaration whose
/// selector, params or value holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DirectiveNode {
  /// Offset of the node's first character.
  pub start: usize,
  /// Offset of its last character (PostCSS's `source.end`).
  pub last: usize,
  /// Offset just past its last character.
  pub end: usize,
}

impl DirectiveNode {
  /// The node spanning `start..end` of `text`, at offset `base` of the
  /// reported coordinates.
  fn new(text: &str, start: usize, end: usize, base: usize) -> Self {
    Self {
      start: base + start,
      last: base + last_char(text, start, end),
      end: base + end,
    }
  }

  /// The span reports about the comment point at: from the node's first
  /// character to its last, which Stylelint gives as the warning's end.
  fn span(&self) -> Span {
    Span::from_range(self.start, self.last.max(self.start))
  }
}

/// One of Stylelint's `DisabledRange`s.
#[derive(Debug, Clone)]
struct DisabledRange {
  /// Index of the comment in [`DisabledRanges::nodes`].
  node: usize,
  /// The first line it covers.
  start: usize,
  /// The last line it covers, or `None` to the end of the file.
  end: Option<usize>,
  /// Whether the comment named this rule (rather than all of them).
  strict_start: bool,
  /// Whether the comment that ended it named this rule, once ended.
  strict_end: Option<bool>,
  /// Whether the comment carried a description.
  described: bool,
}

impl DisabledRange {
  /// Whether the range covers `line`.
  fn covers(&self, line: usize) -> bool {
    self.start <= line && self.end.is_none_or(|end| end >= line)
  }
}

/// Stylelint's `disabledRanges`: the ranges of every rule a configuration
/// comment named, and the blanket ones under [`ALL`], in the order the
/// rules were first seen.
#[derive(Debug, Clone)]
pub(crate) struct DisabledRanges {
  /// The comments the ranges come from.
  nodes: Vec<DirectiveNode>,
  /// Each rule's ranges, [`ALL`] first.
  rules: Vec<(String, Vec<DisabledRange>)>,
}

impl Default for DisabledRanges {
  /// No ranges, with the blanket list in place as Stylelint starts.
  fn default() -> Self {
    Self {
      nodes: Vec::new(),
      rules: vec![(ALL.to_string(), Vec::new())],
    }
  }
}

impl DisabledRanges {
  /// Whether no comment disabled anything.
  pub(crate) fn is_empty(&self) -> bool {
    self.rules.iter().all(|(_, ranges)| ranges.is_empty())
  }

  /// The ranges a problem from `rule` is checked against (Stylelint's
  /// `disabledRanges[ruleName] ?? disabledRanges.all`).
  fn ranges_for<'r>(&'r self, rule: &'r str) -> impl Iterator<Item = &'r DisabledRange> + 'r {
    let named = self
      .rules
      .iter()
      .skip(1)
      .any(|(name, _)| same_rule(name, rule));
    self
      .rules
      .iter()
      .enumerate()
      .filter(move |(i, (name, _))| {
        if named {
          *i > 0 && same_rule(name, rule)
        } else {
          *i == 0
        }
      })
      .flat_map(|(_, (_, ranges))| ranges)
  }

  /// Whether a problem from `rule` reported on `line` is disabled.
  pub(crate) fn covers(&self, rule: &str, line: usize) -> bool {
    self.ranges_for(rule).any(|range| range.covers(line))
  }

  /// The blanket ranges.
  fn all(&self) -> &[DisabledRange] {
    &self.rules[0].1
  }
}

/// A configuration comment Stylelint would reject, such as an `enable`
/// with nothing disabled.  Stylelint stops linting the file there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectiveError {
  /// Stylelint's message, e.g. `No rules have been disabled`.
  pub message: String,
  /// The node of the rejected comment.
  pub node: DirectiveNode,
}

impl DirectiveError {
  /// The problem Stylelint reports in place of every other one in the
  /// file: a `CssSyntaxError` spanning the comment's node.
  pub(crate) fn to_diagnostic(&self, file_path: &str) -> Diagnostic {
    Diagnostic::new(SYNTAX_ERROR, self.message.clone())
      .severity(Severity::Error)
      .span(Span::from_range(self.node.start, self.node.end))
      .file_path(file_path)
  }
}

/// The rule name Stylelint gives a `CssSyntaxError`.
pub(crate) const SYNTAX_ERROR: &str = "CssSyntaxError";

/// One configuration comment, ready to apply.
struct Directive {
  /// The comment's text without its delimiters, trimmed (merged `//`
  /// comments joined by newlines).
  text: String,
  /// The node reports point at.
  node: DirectiveNode,
  /// The line the comment starts on.
  start_line: usize,
  /// The line the comment ends on.
  end_line: usize,
}

/// Builds [`DisabledRanges`] from one or more style sheets, in order.
///
/// Stylelint reads every root of a document (each `<style>` block of an
/// HTML file) as one sequence, so a range opened in one sheet can be
/// closed in a later one.
pub(crate) struct Collector<'l> {
  ranges: DisabledRanges,
  /// The 1-based line of an offset, in whatever coordinates the caller
  /// reports (the host file, or the Sass a converted sheet came from).
  line_of: &'l dyn Fn(usize) -> usize,
  /// The first comment Stylelint would have rejected.
  error: Option<DirectiveError>,
}

impl<'l> Collector<'l> {
  /// Start with no ranges; `line_of` turns an offset passed to
  /// [`Self::scan`] (plus its `base`) into a line number.
  pub(crate) fn new(line_of: &'l dyn Fn(usize) -> usize) -> Self {
    Self {
      ranges: DisabledRanges::default(),
      line_of,
      error: None,
    }
  }

  /// Read the configuration comments in `text`, a style sheet parsed as
  /// `syntax` that starts at offset `base` of the reported coordinates.
  pub(crate) fn scan(&mut self, text: &str, syntax: Syntax, base: usize) {
    if !may_have_directives(text) {
      return;
    }
    self.scan_tree(&PostcssTree::parse(text, syntax), syntax, base);
  }

  /// [`Self::scan`] over a style sheet already parsed as `syntax` into
  /// `tree`, such as the one the rules shared.
  pub(crate) fn scan_tree(&mut self, tree: &PostcssTree, syntax: Syntax, base: usize) {
    if !may_have_directives(tree.source()) {
      return;
    }
    for directive in directives(tree, syntax, base, self.line_of) {
      self.apply(&directive);
    }
  }

  /// The ranges, and the first comment Stylelint would have rejected.
  pub(crate) fn finish(self) -> (DisabledRanges, Option<DirectiveError>) {
    (self.ranges, self.error)
  }

  /// Apply one configuration comment (Stylelint's `checkComment`).
  fn apply(&mut self, directive: &Directive) {
    let Some((command, word_len)) = command(&directive.text) else {
      return;
    };
    let rules = command_rules(&directive.text, word_len);
    let described = has_description(&directive.text);
    let node = self.node_index(directive.node);
    match command {
      Command::DisableLine => {
        for rule in &rules {
          self.disable_line(node, directive.start_line, rule, described);
        }
      }
      Command::DisableNextLine => {
        for rule in &rules {
          self.disable_line(node, directive.end_line + 1, rule, described);
        }
      }
      Command::Disable => {
        for rule in &rules {
          self.disable(node, directive.start_line, rule, described);
        }
      }
      Command::Enable => {
        for rule in &rules {
          self.enable(node, directive.end_line, rule);
        }
      }
    }
  }

  /// The index of `node` in the node list, adding it when new.
  fn node_index(&mut self, node: DirectiveNode) -> usize {
    match self.ranges.nodes.iter().position(|n| *n == node) {
      Some(i) => i,
      None => {
        self.ranges.nodes.push(node);
        self.ranges.nodes.len() - 1
      }
    }
  }

  /// Record the first rejected comment; the rest of the file is still read
  /// so that callers that ignore the error get the other ranges.
  fn fail(&mut self, node: usize, message: String) {
    if self.error.is_none() {
      self.error = Some(DirectiveError {
        message,
        node: self.ranges.nodes[node],
      });
    }
  }

  /// The position of `rule`'s list among the rules.
  fn rule_index(&self, rule: &str) -> Option<usize> {
    if rule == ALL {
      return Some(0);
    }
    self
      .ranges
      .rules
      .iter()
      .skip(1)
      .position(|(name, _)| same_rule(name, rule))
      .map(|i| i + 1)
  }

  /// Stylelint's `ruleIsDisabled`: the rule's last range is still open.
  fn is_disabled(&self, rule: &str) -> bool {
    self
      .rule_index(rule)
      .and_then(|i| self.ranges.rules[i].1.last())
      .is_some_and(|range| range.end.is_none())
  }

  /// Stylelint's `ensureRuleRanges`: give `rule` a list of its own, starting
  /// with copies of the blanket ranges.  Returns its position.
  fn ensure(&mut self, rule: &str) -> usize {
    if let Some(i) = self.rule_index(rule) {
      return i;
    }
    let copies = self
      .ranges
      .all()
      .iter()
      .map(|range| DisabledRange {
        strict_start: false,
        strict_end: Some(false),
        ..range.clone()
      })
      .collect();
    self.ranges.rules.push((rule.to_string(), copies));
    self.ranges.rules.len() - 1
  }

  /// Stylelint's `startDisabledRange`.
  fn start(&mut self, node: usize, line: usize, rule: &str, strict: bool, described: bool) {
    let i = self.ensure(rule);
    self.ranges.rules[i].1.push(DisabledRange {
      node,
      start: line,
      end: None,
      strict_start: strict,
      strict_end: None,
      described,
    });
  }

  /// Stylelint's `endDisabledRange`: close the rule's last range at `line`.
  fn end(&mut self, line: usize, rule: &str, strict: bool) {
    let Some(i) = self.rule_index(rule) else {
      return;
    };
    if let Some(last) = self.ranges.rules[i].1.last_mut() {
      last.end = Some(line);
      last.strict_end = Some(strict);
    }
  }

  /// The names of every rule with a list, [`ALL`] first.
  fn rule_names(&self) -> Vec<String> {
    self
      .ranges
      .rules
      .iter()
      .map(|(name, _)| name.clone())
      .collect()
  }

  /// Stylelint's `disableLine`: `rule` (or every rule) off on `line`.
  fn disable_line(&mut self, node: usize, line: usize, rule: &str, described: bool) {
    if self.is_disabled(ALL) {
      self.fail(node, "All rules have already been disabled".to_string());
      return;
    }
    if rule == ALL {
      for name in self.rule_names() {
        if self.is_disabled(&name) {
          continue;
        }
        let strict = name == ALL;
        self.start(node, line, &name, strict, described);
        self.end(line, &name, strict);
      }
    } else {
      if self.is_disabled(rule) {
        self.fail(node, format!("\"{rule}\" has already been disabled"));
        return;
      }
      self.start(node, line, rule, true, described);
      self.end(line, rule, true);
    }
  }

  /// Stylelint's `processDisableCommand` for one rule: off from `line`.
  fn disable(&mut self, node: usize, line: usize, rule: &str, described: bool) {
    if self.is_disabled(rule) {
      let message = if rule == ALL {
        "All rules have already been disabled".to_string()
      } else {
        format!("\"{rule}\" has already been disabled")
      };
      self.fail(node, message);
      return;
    }
    if rule == ALL {
      for name in self.rule_names() {
        let strict = name == ALL;
        self.start(node, line, &name, strict, described);
      }
    } else {
      self.start(node, line, rule, true, described);
    }
  }

  /// Stylelint's `processEnableCommand` for one rule (or [`ALL`]): back on
  /// after `line`.
  ///
  /// A plain `enable` ends the open range of every rule, the ones disabled
  /// by name included.  Enabling one rule while every rule is disabled
  /// gives that rule a list of its own, copied from the blanket ranges, and
  /// ends it, so the other rules stay off.
  fn enable(&mut self, node: usize, line: usize, rule: &str) {
    if rule == ALL {
      let any_open = self
        .ranges
        .rules
        .iter()
        .any(|(_, list)| list.last().is_some_and(|range| range.end.is_none()));
      if !any_open {
        self.fail(node, "No rules have been disabled".to_string());
        return;
      }
      for (i, name) in self.rule_names().into_iter().enumerate() {
        let open = self.ranges.rules[i]
          .1
          .last()
          .is_some_and(|range| range.end.is_none());
        if open {
          self.end(line, &name, name == ALL);
        }
      }
      return;
    }
    if self.is_disabled(ALL) && self.rule_index(rule).is_none() {
      // Stylelint points these copies at the enable comment.
      let copies = self
        .ranges
        .all()
        .iter()
        .map(|range| DisabledRange {
          node,
          strict_start: false,
          strict_end: Some(false),
          ..range.clone()
        })
        .collect();
      self.ranges.rules.push((rule.to_string(), copies));
      self.end(line, rule, true);
      return;
    }
    if self.is_disabled(rule) {
      self.end(line, rule, true);
      return;
    }
    self.fail(node, format!("\"{rule}\" has not been disabled"));
  }
}

// ---------------------------------------------------------------------------
// Finding the comments
// ---------------------------------------------------------------------------

/// The configuration comments of the style sheet `tree` was parsed from,
/// as `syntax`, in document order, the way Stylelint's walk meets them.
fn directives(
  tree: &PostcssTree,
  syntax: Syntax,
  base: usize,
  line_of: &dyn Fn(usize) -> usize,
) -> Vec<Directive> {
  let text = tree.source();
  let line = |offset: usize| line_of(base + offset);
  // The last end of a run of `//` comments that has been merged into the
  // command before it (Stylelint's `inlineEnd`).
  let mut merged_until: Option<usize> = None;
  let mut found = Vec::new();
  for i in 0..tree.nodes.len() {
    let node = &tree.nodes[i];
    match node.kind {
      NodeKind::Comment => {
        if let Some(last) = merged_until {
          if last == i {
            merged_until = None;
          }
          continue;
        }
        let (directive, merged) = comment_directive(tree, i, base, &line);
        merged_until = merged;
        found.push(directive);
      }
      NodeKind::Rule | NodeKind::AtRule | NodeKind::Decl => {
        let header_end = node.block_open.unwrap_or(node.end);
        let header_start = match node.kind {
          NodeKind::AtRule => node.start + 1 + node.name.len(),
          NodeKind::Decl if text[node.start..].starts_with(node.name.as_str()) => {
            node.start + node.name.len()
          }
          _ => node.start,
        };
        let owner = DirectiveNode::new(text, node.start, node.end, base);
        let header = header_start.min(header_end)..header_end;
        comments_in_node(tree, header, owner, syntax, &line, &mut found);
      }
    }
  }
  found
}

/// The comment node `i` as a directive, merged with the `//` comments on
/// the lines after it that carry on its description, and the last of
/// those merged comments, if any.
///
/// Stylelint works around postcss-scss#109 this way: a `//` command whose
/// text holds `--`, or whose next comment starts with `--`, takes in every
/// following `//` comment on the very next line that is not a command.
fn comment_directive(
  tree: &PostcssTree,
  i: usize,
  base: usize,
  line: &dyn Fn(usize) -> usize,
) -> (Directive, Option<usize>) {
  let node = &tree.nodes[i];
  let next_comment = tree
    .next(i)
    .filter(|&n| tree.nodes[n].kind == NodeKind::Comment);
  let merges = node.inline
    && command(&node.name).is_some()
    && next_comment
      .is_some_and(|n| node.name.contains("--") || tree.nodes[n].name.starts_with("--"));

  let mut text = node.name.clone();
  let mut end = node.end;
  let mut merged = None;
  if let Some(mut current) = next_comment.filter(|_| merges) {
    let mut last_line = line(last_char(tree.source(), node.start, end));
    while tree.nodes[current].inline && command(&tree.nodes[current].name).is_none() {
      let current_node = &tree.nodes[current];
      let current_line = line(last_char(
        tree.source(),
        current_node.start,
        current_node.end,
      ));
      if last_line + 1 != current_line {
        break;
      }
      text.push('\n');
      text.push_str(&current_node.name);
      end = current_node.end;
      merged = Some(current);
      match tree
        .next(current)
        .filter(|&n| tree.nodes[n].kind == NodeKind::Comment)
      {
        Some(next) => {
          current = next;
          last_line = current_line;
        }
        None => break,
      }
    }
  }
  let directive = Directive {
    text,
    node: DirectiveNode::new(tree.source(), node.start, end, base),
    start_line: line(node.start),
    end_line: line(last_char(tree.source(), node.start, end)),
  };
  (directive, merged)
}

/// The offset of the last character of `start..end` (PostCSS's
/// `source.end`), or `start` when it is empty.
fn last_char(text: &str, start: usize, end: usize) -> usize {
  text
    .get(start..end)
    .and_then(|s| s.char_indices().next_back())
    .map_or(start, |(at, _)| start + at)
}

/// Stylelint's `checkCommentsInNode`: the comments in `header` of the
/// tree's source, a node's selector, params or value, each belonging to
/// `owner`, the node that holds them.  Block comments count everywhere,
/// `//` ones in SCSS only; strings and `url()` are skipped.
fn comments_in_node(
  tree: &PostcssTree,
  header: std::ops::Range<usize>,
  owner: DirectiveNode,
  syntax: Syntax,
  line: &dyn Fn(usize) -> usize,
  found: &mut Vec<Directive>,
) {
  let source = tree.source();
  let start = header.start;
  let Some(part) = source.get(header) else {
    return;
  };
  let inline_comments = matches!(syntax, Syntax::Scss | Syntax::Sass);
  if !may_have_directives(part) {
    return;
  }
  let bytes = part.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'"' | b'\'' => i = string_end(bytes, i),
      b'\\' => i += 2,
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        let Some(close) = part[i + 2..].find("*/") else {
          return;
        };
        let comment_end = i + 2 + close + 2;
        found.push(Directive {
          text: part[i + 2..comment_end - 2].trim().to_string(),
          node: owner,
          start_line: line(start + i),
          end_line: line(start + comment_end - 1),
        });
        i = comment_end;
      }
      b'/' if inline_comments && bytes.get(i + 1) == Some(&b'/') => {
        let comment_end = part[i..]
          .find(['\n', '\r', '\x0c'])
          .map_or(part.len(), |n| i + n);
        found.push(Directive {
          text: part[i + 2..comment_end].trim().to_string(),
          node: owner,
          start_line: line(start + i),
          end_line: line(last_char(part, i, comment_end) + start),
        });
        i = comment_end;
      }
      b'u' | b'U'
        if part[i..]
          .get(..4)
          .is_some_and(|s| s.eq_ignore_ascii_case("url(")) =>
      {
        let open = i + 4;
        let first = part[open..].trim_start();
        if first.starts_with(['"', '\'']) {
          i = open;
        } else {
          i = part[open..].find(')').map_or(part.len(), |n| open + n + 1);
        }
      }
      _ => i += 1,
    }
  }
}

/// The offset just past the string that opens at `start` (its closing
/// quote, or the line break or end of text that cuts it short).
fn string_end(bytes: &[u8], start: usize) -> usize {
  let quote = bytes[start];
  let mut i = start + 1;
  while i < bytes.len() {
    match bytes[i] {
      b'\\' => i += 2,
      b'\n' | b'\r' | b'\x0c' => return i,
      b if b == quote => return i + 1,
      _ => i += 1,
    }
  }
  bytes.len()
}

// ---------------------------------------------------------------------------
// Applying the ranges
// ---------------------------------------------------------------------------

mod policy;
pub use policy::DisablePolicy;

/// Drop the problems in `diagnostics` that `ranges` disable (unless
/// `policy` ignores disables), and add the reports about the comments that
/// `policy` asks for, filed under `file_path`.
///
/// `line_of` gives the line of a problem's offset, in the coordinates the
/// ranges were collected in.
pub(crate) fn apply(
  diagnostics: &mut Vec<Diagnostic>,
  ranges: &DisabledRanges,
  line_of: &dyn Fn(usize) -> usize,
  policy: &DisablePolicy,
  file_path: &str,
) {
  if ranges.is_empty() {
    return;
  }
  // Stylelint records the problems a range disabled even under
  // `ignoreDisables`; only dropping them is switched off.
  let mut disabled_warnings: Vec<(String, usize)> = Vec::new();
  diagnostics.retain(|d| {
    if d.is_comment_problem()
      || d.is_unknown_rule()
      || d.is_invalid_option()
      || d.is_internal_error()
    {
      return true;
    }
    let line = line_of(d.span.offset);
    if !ranges.covers(&d.rule_name, line) {
      return true;
    }
    disabled_warnings.push((d.rule_name.clone(), line));
    // Kept only under `ignoreDisables`.
    policy.options.ignore
  });

  let options = policy.options;
  let severity = policy.report_severity();
  let mut report = |rule: &str, message: String, severity: Severity, node: usize| {
    diagnostics.push(
      Diagnostic::new(rule, message)
        .severity(severity)
        .span(ranges.nodes[node].span())
        .file_path(file_path),
    );
  };
  if options.report_needless {
    needless_disables(ranges, &disabled_warnings, |rule, node| {
      report(
        "--report-needless-disables",
        format!("Needless disable for \"{rule}\""),
        severity,
        node,
      );
    });
  }
  if options.report_invalid_scope {
    for (rule, list) in &ranges.rules[1..] {
      if policy.is_configured(rule) {
        continue;
      }
      for range in list {
        if range.strict_start || range.strict_end == Some(true) {
          report(
            "--report-invalid-scope-disables",
            format!("Rule \"{rule}\" isn't enabled"),
            severity,
            range.node,
          );
        }
      }
    }
  }
  if options.report_descriptionless {
    let mut reported: HashSet<usize> = HashSet::new();
    for (rule, list) in &ranges.rules {
      for range in list {
        if range.described || !reported.insert(range.node) {
          continue;
        }
        report(
          "--report-descriptionless-disables",
          format!("Disable for \"{rule}\" is missing a description"),
          severity,
          range.node,
        );
      }
    }
  }
  if options.report_unscoped {
    for range in ranges.all() {
      report(
        "--report-unscoped-disables",
        "Configuration comment must be scoped".to_string(),
        severity,
        range.node,
      );
    }
  }
  for (rule, list) in &ranges.rules[1..] {
    if !policy.forbids_disable(rule) {
      continue;
    }
    for range in list {
      report(
        "reportDisables",
        format!("Rule \"{rule}\" may not be disabled"),
        Severity::Error,
        range.node,
      );
    }
  }
}

/// Stylelint's `reportNeedlessDisables`, narrowed to what gale can prove:
/// `report(rule, node)` for each range of a rule that never reports
/// anything ([`is_noop_stub_rule`]) whose comment disabled no problem of
/// that rule.
///
/// Blanket disables and disables of the rules gale runs are never called
/// needless: gale may not report every problem Stylelint (or a plugin it
/// lacks) would, so the comment may well be needed there.
fn needless_disables(
  ranges: &DisabledRanges,
  disabled_warnings: &[(String, usize)],
  mut report: impl FnMut(&str, usize),
) {
  // The rules each comment usefully disabled.
  let mut useful: HashMap<usize, Vec<&str>> = HashMap::new();
  for (rule, line) in disabled_warnings {
    let named = ranges.rules[1..]
      .iter()
      .filter(|(name, _)| name == rule)
      .flat_map(|(_, list)| list);
    for range in named.chain(ranges.all()) {
      if range.covers(*line) {
        useful.entry(range.node).or_default().push(rule);
      }
    }
  }
  let blanket_nodes: HashSet<usize> = ranges.all().iter().map(|range| range.node).collect();
  for (rule, list) in &ranges.rules[1..] {
    if !is_noop_stub_rule(rule) {
      continue;
    }
    for range in list {
      if blanket_nodes.contains(&range.node) {
        continue;
      }
      let used = useful
        .get(&range.node)
        .is_some_and(|rules| rules.iter().any(|r| same_rule(r, rule)));
      if !used {
        report(rule, range.node);
      }
    }
  }
}

/// Rules that are registered as **no-op stubs** — they never produce any
/// diagnostics.  These exist for third-party plugin rules that Gale can't
/// execute natively.  Because they never fire, any inline disable comment
/// referencing them is genuinely needless.
///
/// All other registered rules *might* produce different diagnostics than
/// Stylelint, so we can't safely call their disables "needless".
fn is_noop_stub_rule(name: &str) -> bool {
  // Only include rules where BOTH Gale AND Stylelint produce zero warnings
  // in practice.  Plugin rules that Gale stubs as no-ops BUT Stylelint
  // actually fires must NOT be listed here, because their disables
  // suppress real Stylelint warnings and are therefore not needless.
  matches!(name, "material/no-prefixes")
}

#[cfg(test)]
mod tests;
