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
//! [`scan`] finds the comments the collector reads, and [`apply()`] then
//! drops the problems those ranges cover and adds Stylelint's
//! reports about the comments themselves.

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::registry::canonical_name;

mod apply;
mod policy;
mod scan;

use apply::apply;
pub use policy::DisablePolicy;
use scan::{directives, last_char};

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

#[cfg(test)]
mod tests;
