//! The statements of a stylesheet as PostCSS sees them, recovered from the
//! source text.
//!
//! Stylelint rules read PostCSS's tree: every rule, at-rule, declaration
//! and comment, in order, with its parent, its siblings, its text exactly
//! as written and the whitespace before it (`raws.before`).  This is that
//! tree, and the one Gale's rules are meant to read: [`views`] gives each
//! node a typed view ([`DeclRef`], [`RuleRef`], ...) and the tree the
//! PostCSS walks (`walk_decls`, `walk_rules`, ...).
//!
//! [`PostcssTree::parse`] ports the statement-level logic of PostCSS's
//! tokenizer and parser, with the changes `postcss-scss` and `postcss-less`
//! make for those syntaxes (`//` comments, `#{}` interpolation, nested
//! properties, Less mixins and variables).  It never fails: where PostCSS
//! would throw, it carries on the way `postcss-safe-parser` does, folding
//! the stray text into the next node's `raws.before`.
//!
//! The linter parses each file's tree once and shares it between every
//! rule and the disable comments, rather than parsing it again.
//!
//! Offsets are byte offsets into the source and always fall on character
//! boundaries: every token ends at an ASCII delimiter or at the end of the
//! input.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::ops::Range;

use gale_diagnostics::SourceLineIndex;

use crate::Syntax;

use parser::Parser;
use tokenizer::{Flavor, Tok, Tokenizer};

mod parser;
mod tokenizer;
pub mod views;

pub use views::{AtRuleRef, CommentRef, DeclRef, NodeRef, RuleRef};

/// The kind of a statement node, as PostCSS types it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
  /// A style rule: `a { ... }`.
  Rule,
  /// An at-rule, with or without a block: `@media x { ... }`, `@import x;`.
  AtRule,
  /// A declaration: `color: red`.  SCSS nested properties with a value
  /// (`margin: 0 { top: 1px }`) are declarations with a block.
  Decl,
  /// A comment: `/* ... */`, or `// ...` in SCSS and Less.
  Comment,
}

/// One statement in the tree.
#[derive(Debug, Clone)]
pub struct Node {
  /// What the statement is.
  pub kind: NodeKind,
  /// Index of the enclosing node, `None` at the top level.
  pub parent: Option<usize>,
  /// Position among the parent's children (or the top-level nodes).
  pub index: usize,
  /// Indexes of the nodes inside the block, `None` when there is no block
  /// (PostCSS's `nodes === undefined`).
  pub children: Option<Vec<usize>>,
  /// Offset of the first character (`source.start`).
  pub start: usize,
  /// Offset just past the last character (`source.end`), including a
  /// declaration's or blockless at-rule's `;` and a block's `}`.
  pub end: usize,
  /// The span of `raws.before`: the whitespace (and stray semicolons)
  /// between the previous node, or the opening brace, and this one.
  pub before: Range<usize>,
  /// Offset of the `{` that opens the block, if there is one.
  pub block_open: Option<usize>,
  /// At-rule name without `@`, declaration property, rule selector (comments
  /// between tokens removed, as PostCSS cleans it), or comment text.
  pub name: String,
  /// At-rule params, or a declaration's value.
  pub params: String,
  /// The at-rule's `raws.afterName`: what separates the name from the params.
  pub after_name: String,
  /// A `//` comment (PostCSS's `raws.inline` / Less `inline`).
  pub inline: bool,
  /// A Less mixin call parsed as an at-rule (`.mixin();`).
  pub mixin: bool,
  /// A Less variable parsed as an at-rule (`@var: value;`).
  pub variable: bool,
  /// A Less `:extend` rule or `&:extend(...)` declaration.
  pub extend: bool,
  /// Where [`Self::name`] is written: a declaration's property (after any
  /// `*` or `_` hack), a rule's selector without the whitespace and
  /// comments before its `{`, or an at-rule's name without the `@`.
  pub name_span: Range<usize>,
  /// A declaration's raw value as Stylelint's `getDeclarationValue` reads
  /// it (comments kept, `!important` left out), or an at-rule's raw params
  /// (`getAtRuleParams`).  Empty for other nodes.
  pub value_span: Range<usize>,
  /// A declaration's `decl.value`: the raw value with the comments PostCSS
  /// drops (those next to whitespace) removed.
  pub value: String,
  /// Whether a declaration is `!important`.
  pub important: bool,
  /// For a node with a block, PostCSS's `raws.semicolon`: whether its last
  /// non-comment child ended with `;`.
  pub semicolon: bool,
}

/// The parsed statements of one stylesheet.
#[derive(Debug, Clone)]
pub struct PostcssTree<'a> {
  /// The text the offsets point into.
  source: &'a str,
  /// Every node, in document (pre-)order: a parent precedes its children.
  pub nodes: Vec<Node>,
  /// The top-level nodes.
  pub root: Vec<usize>,
  /// The root's `raws.semicolon`: whether its last non-comment node ended
  /// with `;`.
  pub root_semicolon: bool,
  /// The line each offset is on.
  lines: SourceLineIndex,
  /// The parser the tree imitates.
  flavor: Flavor,
  /// Which nodes are standard-syntax rules, worked out on first use: every
  /// selector rule asks it of every rule.
  standard_rules: OnceCell<Vec<bool>>,
}

impl<'a> PostcssTree<'a> {
  /// Parse `source` the way Stylelint's parser for `syntax` would.  Sass is
  /// treated as SCSS, since Gale lints the SCSS it converts Sass into.
  pub fn parse(source: &'a str, syntax: Syntax) -> Self {
    let flavor = match syntax {
      Syntax::Css => Flavor::Css,
      Syntax::Scss | Syntax::Sass => Flavor::Scss,
      Syntax::Less => Flavor::Less,
    };
    let mut parser = Parser::new(source, flavor);
    parser.parse();
    Self {
      source,
      nodes: parser.nodes,
      root: parser.root,
      root_semicolon: parser.root_semicolon,
      lines: SourceLineIndex::build(source),
      flavor,
      standard_rules: OnceCell::new(),
    }
  }

  /// The source text the tree was parsed from.
  pub fn source(&self) -> &'a str {
    self.source
  }

  /// The 1-based line holding `offset`.
  pub fn line_of(&self, offset: usize) -> usize {
    self.lines.line(offset)
  }

  /// The text of `range`, a rule's selector, a declaration's value or an
  /// at-rule's params, as the parser keeps it in `raws.*.raw`, which is
  /// what Stylelint's `getRuleSelector`, `getDeclarationValue` and
  /// `getAtRuleParams` read.  postcss-scss writes each `//` comment there
  /// as a `/* */` one; the text is otherwise the source's.
  ///
  /// Offsets into the result line up with the source up to the first
  /// `//` comment, and run two bytes ahead per comment after it, as they
  /// do in Stylelint.
  pub fn raw_text(&self, range: Range<usize>) -> Cow<'a, str> {
    let text = self.source.get(range.clone()).unwrap_or("");
    if self.flavor != Flavor::Scss || !text.contains("//") {
      return Cow::Borrowed(text);
    }
    let mut tokenizer = Tokenizer::new(self.source, true);
    tokenizer.pos = range.start;
    let mut out = String::with_capacity(text.len() + 4);
    let mut converted = false;
    while tokenizer.pos < range.end {
      let Some(token) = tokenizer.next_token() else {
        break;
      };
      let token_text = self
        .source
        .get(token.start..token.end.min(range.end))
        .unwrap_or("");
      if token.kind == Tok::InlineComment {
        // postcss-scss: `'/*' + text.slice(2).replace(/(\*\/|\/\*)/g, '*//*') + '*/'`.
        converted = true;
        out.push_str("/*");
        let mut rest = token_text.get(2..).unwrap_or("");
        while let Some(at) = rest.find(['*', '/']) {
          out.push_str(&rest[..at]);
          let pair = rest.get(at..at + 2);
          if pair == Some("*/") || pair == Some("/*") {
            out.push_str("*//*");
            rest = &rest[at + 2..];
          } else {
            out.push_str(&rest[at..at + 1]);
            rest = &rest[at + 1..];
          }
        }
        out.push_str(rest);
        out.push_str("*/");
      } else {
        out.push_str(token_text);
      }
    }
    if converted {
      Cow::Owned(out)
    } else {
      Cow::Borrowed(text)
    }
  }

  /// The line the node starts on (Stylelint's `getNodeLine`).
  pub fn start_line(&self, i: usize) -> usize {
    self.line_of(self.nodes[i].start)
  }

  /// The line of the node's last character (`source.end.line`).
  pub fn end_line(&self, i: usize) -> usize {
    let node = &self.nodes[i];
    self.line_of(node.end.saturating_sub(1).max(node.start))
  }

  /// The node's `raws.before`.
  pub fn before(&self, i: usize) -> &'a str {
    let range = &self.nodes[i].before;
    self.source.get(range.clone()).unwrap_or("")
  }

  /// The node's own text, what PostCSS's `toString()` gives for a node that
  /// has not been changed (no `raws.before`).
  pub fn text(&self, i: usize) -> &'a str {
    let node = &self.nodes[i];
    self.source.get(node.start..node.end).unwrap_or("")
  }

  /// stylelint-scss's `declarationValueIndex`: where the declaration's value
  /// starts, counted from the start of the declaration as the position of
  /// the first `:` in `decl.toString()` plus what follows it in
  /// `raws.between`.
  pub fn declaration_value_index(&self, decl: &Node) -> Option<usize> {
    let prop_and_between = self
      .source
      .get(decl.name_span.start..decl.value_span.start)?;
    let between = self.source.get(decl.name_span.end..decl.value_span.start)?;
    let before_colon = prop_and_between.find(':')?;
    let after_colon = between.len() - between.find(':')?;
    Some(before_colon + after_colon)
  }

  /// stylelint-scss's `atRuleParamIndex`: where the at-rule's params start,
  /// counted from its `@`.
  pub fn at_rule_param_index(&self, at_rule: &Node) -> usize {
    1 + at_rule.name.len() + at_rule.after_name.len()
  }

  /// The children of node `parent`, or the top-level nodes for `None`.
  pub fn children_of(&self, parent: Option<usize>) -> &[usize] {
    match parent {
      Some(i) => self.nodes[i].children.as_deref().unwrap_or(&[]),
      None => &self.root,
    }
  }

  /// The nodes sharing the node's parent, the node included.
  pub fn siblings(&self, i: usize) -> &[usize] {
    match self.nodes[i].parent {
      Some(parent) => self.nodes[parent].children.as_deref().unwrap_or(&[]),
      None => &self.root,
    }
  }

  /// The node's position among its siblings.
  fn index_in_parent(&self, i: usize) -> usize {
    self.nodes[i].index
  }

  /// The sibling before the node (PostCSS's `prev()`).
  pub fn prev(&self, i: usize) -> Option<usize> {
    let index = self.index_in_parent(i);
    index.checked_sub(1).map(|prev| self.siblings(i)[prev])
  }

  /// The sibling after the node (PostCSS's `next()`).
  pub fn next(&self, i: usize) -> Option<usize> {
    self.siblings(i).get(self.index_in_parent(i) + 1).copied()
  }

  /// Whether the node is a comment.
  fn is_comment(&self, i: Option<usize>) -> bool {
    i.is_some_and(|i| self.nodes[i].kind == NodeKind::Comment)
  }

  /// Whether the node has a block (`nodes !== undefined`).
  pub fn has_block(&self, i: usize) -> bool {
    self.nodes[i].children.is_some()
  }

  /// Stylelint's `isFirstNodeOfRoot`: the first node of the stylesheet.
  pub fn is_first_node_of_root(&self, i: usize) -> bool {
    self.nodes[i].parent.is_none() && self.root.first() == Some(&i)
  }

  /// Stylelint's `isFirstNested`: the first node in its block, or preceded
  /// only by comments on the line of the opening brace.  Never true at the
  /// top level.
  pub fn is_first_nested(&self, i: usize) -> bool {
    let Some(parent) = self.nodes[i].parent else {
      return false;
    };
    let siblings = self.nodes[parent].children.as_deref().unwrap_or(&[]);
    let Some(&first) = siblings.first() else {
      return false;
    };
    if first == i {
      return true;
    }
    if self.nodes[first].kind != NodeKind::Comment || self.before(first).contains('\n') {
      return false;
    }
    let brace_line = self.start_line(first);
    if self.end_line(first) != brace_line {
      return false;
    }
    for &node in &siblings[1..] {
      if node == i {
        return true;
      }
      if self.nodes[node].kind != NodeKind::Comment || self.end_line(node) != brace_line {
        return false;
      }
    }
    false
  }

  /// Stylelint's `getPreviousNonSharedLineCommentNode`: the previous sibling,
  /// skipping comments that share a line with the node or with the node
  /// before them.
  pub fn previous_non_shared_line_comment(&self, i: usize) -> Option<usize> {
    let prev = self.prev(i)?;
    if self.nodes[prev].kind != NodeKind::Comment {
      return Some(prev);
    }
    if self.start_line(i) == self.start_line(prev) {
      return self.previous_non_shared_line_comment(prev);
    }
    if let Some(prev2) = self.prev(prev)
      && self.start_line(prev) == self.start_line(prev2)
    {
      return self.previous_non_shared_line_comment(prev);
    }
    Some(prev)
  }

  /// Stylelint's `getNextNonSharedLineCommentNode`, the mirror of
  /// [`Self::previous_non_shared_line_comment`].
  pub fn next_non_shared_line_comment(&self, i: usize) -> Option<usize> {
    let next = self.next(i)?;
    if self.nodes[next].kind != NodeKind::Comment {
      return Some(next);
    }
    let after = self.next(next).map(|n| self.start_line(n));
    if self.start_line(i) == self.start_line(next) || after == Some(self.start_line(next)) {
      return self.next_non_shared_line_comment(next);
    }
    Some(next)
  }

  /// Stylelint's `isSharedLineComment`: a comment on the same line as the
  /// code before or after it (other than `ignored`), or right after the
  /// opening brace.
  pub fn is_shared_line_comment(&self, i: usize, ignored: Option<usize>) -> bool {
    if self.nodes[i].kind != NodeKind::Comment {
      return false;
    }
    if let Some(prev) = self.previous_non_shared_line_comment(i)
      && self.end_line(prev) == self.start_line(i)
    {
      return true;
    }
    if let Some(next) = self.next_non_shared_line_comment(i)
      && Some(next) != ignored
      && self.end_line(i) == self.start_line(next)
    {
      return true;
    }
    self.nodes[i].parent.is_some() && self.index_in_parent(i) == 0 && !self.before(i).contains('\n')
  }

  /// Stylelint's `isAfterComment`: the previous sibling is a comment of its
  /// own, not one trailing other code.
  pub fn is_after_comment(&self, i: usize) -> bool {
    match self.prev(i) {
      Some(prev) if self.nodes[prev].kind == NodeKind::Comment => {
        !self.is_shared_line_comment(prev, None)
      }
      _ => false,
    }
  }

  /// Stylelint's `isAfterSingleLineComment`.
  pub fn is_after_single_line_comment(&self, i: usize) -> bool {
    let Some(prev) = self.prev(i) else {
      return false;
    };
    self.is_comment(Some(prev))
      && !self.is_shared_line_comment(prev, Some(i))
      && self.start_line(prev) == self.end_line(prev)
  }

  /// The source ranges to delete so the text reads as PostCSS prints the
  /// tree after `node.remove()`.
  ///
  /// That is the node with its `raws.before`.  PostCSS prints a `;` after
  /// every declaration and blockless at-rule but the last non-comment node
  /// of a block, which gets one only when the block's last statement had one
  /// (`raws.semicolon`).  So when the node removed is that last statement
  /// and has no `;`, the one ending the statement that takes its place goes
  /// too.  Ranges come in source order and never overlap.
  pub fn removal_ranges(&self, i: usize) -> Vec<Range<usize>> {
    let node = &self.nodes[i];
    let mut ranges = Vec::with_capacity(2);
    ranges.push(node.before.start.min(node.start)..node.end);
    let siblings = self.siblings(i);
    let is_statement = |s: usize| self.nodes[s].kind != NodeKind::Comment;
    let last = siblings.iter().rposition(|&s| is_statement(s));
    if last.map(|l| siblings[l]) != Some(i) || self.text(i).ends_with(';') {
      return ranges;
    }
    let previous = siblings[..node.index.min(siblings.len())]
      .iter()
      .rev()
      .find(|&&s| is_statement(s));
    if let Some(&prev) = previous {
      let end = self.nodes[prev].end;
      if self.takes_semicolon(prev) && self.text(prev).ends_with(';') {
        ranges.insert(0, end - 1..end);
      }
    }
    ranges
  }

  /// Whether PostCSS prints a `;` after the node when it is followed by
  /// another statement: declarations and blockless at-rules.  (SCSS nested
  /// properties print their block instead, and a rule keeps the `;` of its
  /// own, `raws.ownSemicolon`, inside its text.)
  fn takes_semicolon(&self, i: usize) -> bool {
    let node = &self.nodes[i];
    matches!(node.kind, NodeKind::Decl | NodeKind::AtRule) && node.children.is_none()
  }

  /// What PostCSS prints for the children of a block after they are taken
  /// out and appended again in `order` (`removeAll()` then `append()`):
  /// each node with its own `raws.before`, and a `;` after every
  /// declaration and blockless at-rule except the last statement, which
  /// gets one only when `semicolon` (the block's `raws.semicolon`,
  /// [`Node::semicolon`]) is set.
  ///
  /// The text runs from the first child's `raws.before` to the end of the
  /// last child, the span [`Self::children_span`] gives for the block.
  pub fn print_children(&self, order: &[usize], semicolon: bool) -> String {
    let last_statement = order
      .iter()
      .rposition(|&c| self.nodes[c].kind != NodeKind::Comment);
    let mut out = String::new();
    for (position, &child) in order.iter().enumerate() {
      let node = &self.nodes[child];
      // A `*` or `_` hack sits in both `raws.before` and the node's span.
      let before_end = node.before.end.min(node.start);
      out.push_str(self.source.get(node.before.start..before_end).unwrap_or(""));
      let text = self.text(child);
      if self.takes_semicolon(child) {
        out.push_str(text.strip_suffix(';').unwrap_or(text));
        if Some(position) != last_statement || semicolon {
          out.push(';');
        }
      } else {
        out.push_str(text);
      }
    }
    out
  }

  /// The source span of a block's children: from the first child's
  /// `raws.before` to the end of the last child.
  pub fn children_span(&self, children: &[usize]) -> Option<Range<usize>> {
    let first = &self.nodes[*children.first()?];
    let last = &self.nodes[*children.last()?];
    Some(first.before.start.min(first.start)..last.end)
  }

  /// Stylelint's `isAfterBlock`: the previous sibling is a rule or at-rule.
  pub fn is_after_block(&self, i: usize) -> bool {
    self
      .prev(i)
      .is_some_and(|prev| matches!(self.nodes[prev].kind, NodeKind::Rule | NodeKind::AtRule))
  }

  /// Stylelint's `blockString` of a node's parent: the block from its `{`
  /// on, or the whole stylesheet for top-level nodes.
  pub fn parent_block_string(&self, i: usize) -> &'a str {
    match self.nodes[i].parent {
      Some(parent) => {
        let node = &self.nodes[parent];
        node
          .block_open
          .and_then(|open| self.source.get(open..node.end))
          .unwrap_or("")
      }
      None => self.source,
    }
  }

  /// Stylelint's `isStandardSyntaxAtRule`.
  pub fn is_standard_syntax_at_rule(&self, i: usize) -> bool {
    let node = &self.nodes[i];
    if node.name.eq_ignore_ascii_case("charset") {
      return false;
    }
    let blockless = node.children.is_none();
    if blockless && node.params.is_empty() {
      return false;
    }
    if node.mixin || node.variable {
      return false;
    }
    !(blockless && node.after_name.is_empty() && node.params.starts_with('('))
  }

  /// Stylelint's `isStandardSyntaxRule`.
  pub fn is_standard_syntax_rule(&self, i: usize) -> bool {
    let standard = self.standard_rules.get_or_init(|| {
      self
        .nodes
        .iter()
        .map(|node| {
          node.kind == NodeKind::Rule && !node.extend && is_standard_syntax_selector(&node.name)
        })
        .collect()
    });
    standard[i]
  }

  /// Stylelint's `isStandardSyntaxDeclaration`.
  pub fn is_standard_syntax_declaration(&self, i: usize) -> bool {
    let node = &self.nodes[i];
    let prop = node.name.as_str();
    if prop.starts_with('$') || prop.contains(".$") {
      return false;
    }
    if prop.starts_with('@') && !prop[1..].starts_with('{') {
      return false;
    }
    if let Some(parent) = node.parent.map(|p| &self.nodes[p]) {
      if parent.kind == NodeKind::AtRule && parent.after_name == ":" {
        return false;
      }
      if parent.kind == NodeKind::Rule {
        let selector = parent.name.as_str();
        if selector.starts_with('#') && selector.ends_with("()") {
          return false;
        }
        if selector.ends_with(':') && !selector.starts_with("--") {
          return false;
        }
      }
    }
    !node.extend
  }
}

/// Stylelint's `isStandardSyntaxSelector`: no interpolation, placeholder,
/// nested property, Less mixin or extend, template tag or comment.
pub fn is_standard_syntax_selector(selector: &str) -> bool {
  if has_interpolation(selector)
    || selector.starts_with('%')
    || selector.ends_with(':')
    || selector.contains(":extend")
    || selector.contains("<%")
    || selector.contains("%>")
    || selector.contains("//")
  {
    return false;
  }
  // Less mixins: `.foo().bar`, `.foo(@a)[x]`, `.mixin() {}`, `.mixin(@a: 1)`.
  if is_less_mixin_with_suffix(selector) {
    return false;
  }
  if selector.ends_with(')') && !selector.contains(':') {
    return false;
  }
  // `/\(@.*\)$/`: a Less parametric mixin.
  if selector.ends_with(')')
    && let Some(open) = selector.rfind("(@")
    && !selector[open..].contains(['\n', '\r'])
  {
    return false;
  }
  true
}

/// `/\.[\w-]+\(.*\).+/`: a Less mixin call followed by more selector.
fn is_less_mixin_with_suffix(selector: &str) -> bool {
  let bytes = selector.as_bytes();
  for (dot, _) in selector.match_indices('.') {
    let mut i = dot + 1;
    while i < bytes.len()
      && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
    {
      i += 1;
    }
    if i == dot + 1 || bytes.get(i) != Some(&b'(') {
      continue;
    }
    // `.*` stops at line breaks: some `)` on the line must have
    // something after it, and the first one is the best candidate.
    let line_end = selector[i..]
      .find(['\n', '\r'])
      .map_or(selector.len(), |n| i + n);
    if let Some(close) = selector[i + 1..line_end].find(')')
      && i + 1 + close + 1 < line_end
    {
      return true;
    }
  }
  false
}

/// Stylelint's `hasInterpolation`: SCSS `#{}`, Less `@{}`, template `{}` or
/// PostCSS simple vars `$()` with something inside.
pub fn has_interpolation(text: &str) -> bool {
  has_delimited(text, "#{", '}', true)
    || has_delimited(text, "@{", '}', false)
    || has_delimited(text, "{", '}', true)
    || has_delimited(text, "$(", ')', false)
}

/// Whether `open`, then at least one character, then `close` appear in
/// `text`.  Without `dot_all` the characters may not include line breaks
/// (JavaScript's `.`).
pub fn has_delimited(text: &str, open: &str, close: char, dot_all: bool) -> bool {
  text.match_indices(open).any(|(at, _)| {
    let rest = &text[at + open.len()..];
    let mut chars = rest.char_indices();
    match chars.next() {
      Some((_, c)) if dot_all || !is_js_line_break(c) => {}
      _ => return false,
    }
    for (_, c) in chars {
      if c == close {
        return true;
      }
      if !dot_all && is_js_line_break(c) {
        return false;
      }
    }
    false
  })
}

/// The characters JavaScript's `.` does not match.
fn is_js_line_break(c: char) -> bool {
  matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

#[cfg(test)]
mod tests;
