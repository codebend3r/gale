//! The statements of a stylesheet as PostCSS sees them, recovered from the
//! source text.
//!
//! Rules that reason about layout (the `*-empty-line-before` family, comment
//! spacing) need what Stylelint's PostCSS tree gives them: every rule,
//! at-rule, declaration and comment, in order, with its parent, its siblings
//! and the exact whitespace before it (`raws.before`).  Gale's own AST cannot
//! answer that: lightningcss drops comments and the at-rules it does not
//! know, and the SCSS/Less AST hoists comments to the top level.
//!
//! [`PostcssTree::parse`] ports the statement-level logic of PostCSS's
//! tokenizer and parser, with the changes `postcss-scss` and `postcss-less`
//! make for those syntaxes (`//` comments, `#{}` interpolation, nested
//! properties, Less mixins and variables).  It never fails: where PostCSS
//! would throw, it carries on the way `postcss-safe-parser` does, folding
//! the stray text into the next node's `raws.before`.
//!
//! Offsets are byte offsets into the source and always fall on character
//! boundaries: every token ends at an ASCII delimiter or at the end of the
//! input.

use std::ops::Range;

use gale_css_parser::Syntax;

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
  /// Offset where each line starts; line 1 starts at 0.
  line_starts: Vec<usize>,
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
    let mut line_starts = vec![0];
    line_starts.extend(
      source
        .bytes()
        .enumerate()
        .filter(|&(_, b)| b == b'\n')
        .map(|(i, _)| i + 1),
    );
    Self {
      source,
      nodes: parser.nodes,
      root: parser.root,
      root_semicolon: parser.root_semicolon,
      line_starts,
    }
  }

  /// The source text the tree was parsed from.
  pub fn source(&self) -> &'a str {
    self.source
  }

  /// The 1-based line holding `offset`.
  pub fn line_of(&self, offset: usize) -> usize {
    match self.line_starts.binary_search(&offset) {
      Ok(line) => line + 1,
      Err(next) => next,
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

  /// Every declaration, in document order.
  pub fn decls(&self) -> impl Iterator<Item = &Node> {
    self.nodes.iter().filter(|n| n.kind == NodeKind::Decl)
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
    let mut ranges = vec![node.before.start.min(node.start)..node.end];
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
      let p = &self.nodes[prev];
      let prints_semicolon =
        p.kind == NodeKind::Decl || (p.kind == NodeKind::AtRule && p.children.is_none());
      if prints_semicolon && self.text(prev).ends_with(';') {
        ranges.insert(0, p.end - 1..p.end);
      }
    }
    ranges
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
    let node = &self.nodes[i];
    node.kind == NodeKind::Rule && !node.extend && is_standard_syntax_selector(&node.name)
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
fn has_delimited(text: &str, open: &str, close: char, dot_all: bool) -> bool {
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

// ---------------------------------------------------------------------------
// Tokenizer: postcss/lib/tokenize.js and postcss-scss/lib/scss-tokenize.js
// ---------------------------------------------------------------------------

/// Which PostCSS parser to imitate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flavor {
  /// `postcss`.
  Css,
  /// `postcss-scss`.
  Scss,
  /// `postcss-less`, which keeps PostCSS's tokenizer.
  Less,
}

/// A token type, named after PostCSS's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
  Space,
  Word,
  Str,
  AtWord,
  Brackets,
  OpenParen,
  CloseParen,
  OpenSquare,
  CloseSquare,
  OpenCurly,
  CloseCurly,
  Semicolon,
  Colon,
  Comment,
  InlineComment,
}

/// A token: its type and the source range it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Token {
  kind: Tok,
  start: usize,
  end: usize,
}

impl Token {
  /// Whether PostCSS gives the token type `comment`.
  fn is_comment(&self) -> bool {
    matches!(self.kind, Tok::Comment | Tok::InlineComment)
  }

  /// Whether the token is whitespace or a comment.
  fn is_space_or_comment(&self) -> bool {
    self.kind == Tok::Space || self.is_comment()
  }
}

/// Whether `b` is whitespace to the PostCSS tokenizer.
fn is_space(b: u8) -> bool {
  matches!(b, b' ' | b'\n' | b'\t' | b'\r' | 0x0c)
}

/// Splits the source into PostCSS tokens, with `back` to push tokens back.
struct Tokenizer<'a> {
  css: &'a str,
  bytes: &'a [u8],
  scss: bool,
  pos: usize,
  /// Words seen so far, for the `url(` check (PostCSS's `buffer`).
  buffer: Vec<Token>,
  /// Tokens pushed back, read again before new ones.
  returned: Vec<Token>,
  /// The end of the last `(` that could not be read as one token.
  last_bad_paren: Option<usize>,
}

impl<'a> Tokenizer<'a> {
  /// A tokenizer over `css`; `scss` switches on postcss-scss's changes.
  fn new(css: &'a str, scss: bool) -> Self {
    Self {
      css,
      bytes: css.as_bytes(),
      scss,
      pos: 0,
      buffer: Vec::new(),
      returned: Vec::new(),
      last_bad_paren: None,
    }
  }

  /// Whether every token has been read.
  fn end_of_file(&self) -> bool {
    self.returned.is_empty() && self.pos >= self.bytes.len()
  }

  /// Push a token back to be read again.
  fn back(&mut self, token: Token) {
    self.returned.push(token);
  }

  /// The byte at `i`, or 0 past the end.
  fn at(&self, i: usize) -> u8 {
    self.bytes.get(i).copied().unwrap_or(0)
  }

  /// The text of a token.
  fn text(&self, token: Token) -> &'a str {
    self.css.get(token.start..token.end).unwrap_or("")
  }

  /// The offset just past the character starting at `i`.
  fn char_end(&self, i: usize) -> usize {
    let mut end = i + 1;
    while end < self.bytes.len() && !self.css.is_char_boundary(end) {
      end += 1;
    }
    end.min(self.bytes.len())
  }

  /// The first offset at or after `from` where `pred` holds, if any.
  fn find(&self, from: usize, pred: impl Fn(u8, usize) -> bool) -> Option<usize> {
    (from..self.bytes.len()).find(|&i| pred(self.bytes[i], i))
  }

  /// Read the next token.
  fn next_token(&mut self) -> Option<Token> {
    if let Some(token) = self.returned.pop() {
      return Some(token);
    }
    let len = self.bytes.len();
    let pos = self.pos;
    if pos >= len {
      return None;
    }
    let code = self.bytes[pos];
    let (kind, end) = match code {
      b if is_space(b) => {
        let end = self.find(pos + 1, |b, _| !is_space(b)).unwrap_or(len);
        (Tok::Space, end)
      }
      b'[' => (Tok::OpenSquare, pos + 1),
      b']' => (Tok::CloseSquare, pos + 1),
      b'{' => (Tok::OpenCurly, pos + 1),
      b'}' => (Tok::CloseCurly, pos + 1),
      b':' => (Tok::Colon, pos + 1),
      b';' => (Tok::Semicolon, pos + 1),
      b')' => (Tok::CloseParen, pos + 1),
      b',' if self.scss => (Tok::Word, pos + 1),
      b'(' => self.paren(pos),
      b'\'' | b'"' => (Tok::Str, self.string_end(pos)),
      b'@' => {
        let end = self
          .find(pos + 1, |b, _| {
            matches!(
              b,
              b'\t'
                | b'\n'
                | 0x0c
                | b'\r'
                | b' '
                | b'"'
                | b'#'
                | b'\''
                | b'('
                | b')'
                | b'/'
                | b';'
                | b'['
                | b'\\'
                | b']'
                | b'{'
                | b'}'
            )
          })
          .unwrap_or(len);
        (Tok::AtWord, end)
      }
      b'\\' => (Tok::Word, self.escape_end(pos)),
      _ => {
        let next = self.at(pos + 1);
        if self.scss && code == b'#' && next == b'{' {
          (Tok::Word, self.interpolation_end(pos))
        } else if code == b'/' && next == b'*' {
          let end = self.css[pos + 2..]
            .find("*/")
            .map_or(len, |at| pos + 2 + at + 2);
          (Tok::Comment, end)
        } else if self.scss && code == b'/' && next == b'/' {
          let end = self
            .find(pos + 1, |b, _| matches!(b, b'\n' | 0x0c | b'\r'))
            .unwrap_or(len);
          (Tok::InlineComment, end)
        } else {
          let scss = self.scss;
          let bytes = self.bytes;
          let end = self
            .find(pos + 1, |b, i| {
              matches!(
                b,
                b'\t'
                  | b'\n'
                  | 0x0c
                  | b'\r'
                  | b' '
                  | b'!'
                  | b'"'
                  | b'#'
                  | b'\''
                  | b'('
                  | b')'
                  | b':'
                  | b';'
                  | b'@'
                  | b'['
                  | b'\\'
                  | b']'
                  | b'{'
                  | b'}'
              ) || (scss && b == b',')
                || (b == b'/' && bytes.get(i + 1) == Some(&b'*'))
            })
            .unwrap_or(len);
          let token = Token {
            kind: Tok::Word,
            start: pos,
            end,
          };
          self.buffer.push(token);
          (Tok::Word, end)
        }
      }
    };
    let end = end.clamp(pos + 1, len);
    self.pos = end;
    Some(Token {
      kind,
      start: pos,
      end,
    })
  }

  /// A `(`: the whole `url(...)` or a simple `(...)` as one `brackets`
  /// token, or a lone `(`.
  fn paren(&mut self, pos: usize) -> (Tok, usize) {
    let len = self.bytes.len();
    let prev = self.buffer.pop().map(|t| self.text(t)).unwrap_or("");
    let n = self.at(pos + 1);
    if self.scss {
      if prev == "url" && n != b'\'' && n != b'"' {
        let mut depth = 1;
        let mut next = pos + 1;
        while next < len {
          match self.bytes[next] {
            b'(' => depth += 1,
            b')' => {
              depth -= 1;
              if depth == 0 {
                break;
              }
            }
            _ => {}
          }
          next += 1;
        }
        return (Tok::Brackets, next + 1);
      }
    } else if prev == "url" && !matches!(n, b'\'' | b'"') && !is_space(n) {
      let mut next = pos;
      loop {
        let Some(close) = self.find(next + 1, |b, _| b == b')') else {
          return (Tok::Brackets, pos + 1);
        };
        next = close;
        let backslashes = self.bytes[..close]
          .iter()
          .rev()
          .take_while(|&&b| b == b'\\')
          .count();
        if backslashes % 2 == 0 {
          return (Tok::Brackets, close + 1);
        }
      }
    } else if self.last_bad_paren.is_some_and(|bad| pos <= bad) {
      return (Tok::OpenParen, pos + 1);
    }
    let close = self.find(pos + 1, |b, _| b == b')');
    let bad = match close {
      None => true,
      Some(close) => self.bytes[pos + 1..=close]
        .iter()
        .any(|b| matches!(b, b'\r' | b'\n' | b'"' | b'\'' | b'(' | b'/' | b'\\')),
    };
    if bad {
      if !self.scss {
        self.last_bad_paren = Some(close.unwrap_or(len));
      }
      (Tok::OpenParen, pos + 1)
    } else {
      (Tok::Brackets, close.map_or(len, |close| close + 1))
    }
  }

  /// The end of the string starting at `pos`; an unclosed string runs to
  /// the end of the input.
  fn string_end(&self, pos: usize) -> usize {
    let len = self.bytes.len();
    let quote = self.bytes[pos];
    let mut next = pos;
    let mut escaped = false;
    loop {
      next += 1;
      if next >= len {
        return len;
      }
      let code = self.bytes[next];
      if !escaped && code == quote {
        return next + 1;
      } else if code == b'\\' {
        escaped = !escaped;
      } else if escaped {
        escaped = false;
      } else if self.scss && code == b'#' && self.at(next + 1) == b'{' {
        next = self.interpolation_end(next) - 1;
      }
    }
  }

  /// The end of the SCSS interpolation `#{...}` starting at `pos`, nested
  /// interpolations and strings inside included.
  fn interpolation_end(&self, pos: usize) -> usize {
    let len = self.bytes.len();
    let mut depth = 1;
    let mut next = pos + 1;
    let mut quote = 0u8;
    let mut escaped = false;
    while depth > 0 {
      next += 1;
      if next >= len {
        return len;
      }
      let code = self.bytes[next];
      if quote != 0 {
        if !escaped && code == quote {
          quote = 0;
        } else if code == b'\\' {
          escaped = !escaped;
        } else if escaped {
          escaped = false;
        }
      } else if code == b'\'' || code == b'"' {
        quote = code;
      } else if code == b'}' {
        depth -= 1;
      } else if code == b'#' && self.at(next + 1) == b'{' {
        depth += 1;
      }
    }
    next + 1
  }

  /// The end of the escape sequence starting at the backslash at `pos`.
  fn escape_end(&self, pos: usize) -> usize {
    let len = self.bytes.len();
    let mut next = pos;
    let mut escape = true;
    while self.at(next + 1) == b'\\' {
      next += 1;
      escape = !escape;
    }
    let code = self.at(next + 1);
    if escape && code != b'/' && !is_space(code) {
      if next + 1 >= len {
        return len;
      }
      next = self.char_end(next + 1) - 1;
      if self.bytes[next].is_ascii_hexdigit() {
        while self.at(next + 1).is_ascii_hexdigit() {
          next += 1;
        }
        if self.at(next + 1) == b' ' {
          next += 1;
        }
      }
    }
    (next + 1).min(len)
  }
}

// ---------------------------------------------------------------------------
// Parser: postcss/lib/parser.js with the postcss-scss and postcss-less changes
// ---------------------------------------------------------------------------

/// Builds the tree from the token stream.
struct Parser<'a> {
  css: &'a str,
  flavor: Flavor,
  tokenizer: Tokenizer<'a>,
  nodes: Vec<Node>,
  root: Vec<usize>,
  /// The node whose block is open, `None` for the top level.
  current: Option<usize>,
  /// Where the pending `raws.before` (PostCSS's `this.spaces`) starts.
  spaces: Option<usize>,
  /// Rules that already took a free semicolon (`raws.ownSemicolon`).
  own_semicolon: Vec<bool>,
  /// PostCSS's `this.semicolon`: whether the last non-comment node ended
  /// with `;`, recorded as the block's `raws.semicolon` when it closes.
  semicolon: bool,
  /// The root's `raws.semicolon`, set at the end of the input.
  root_semicolon: bool,
}

impl<'a> Parser<'a> {
  /// A parser over `css` for `flavor`.
  fn new(css: &'a str, flavor: Flavor) -> Self {
    Self {
      css,
      flavor,
      tokenizer: Tokenizer::new(css, flavor == Flavor::Scss),
      nodes: Vec::new(),
      root: Vec::new(),
      current: None,
      spaces: None,
      own_semicolon: Vec::new(),
      semicolon: false,
      root_semicolon: false,
    }
  }

  /// The text of a token.
  fn text(&self, token: Token) -> &'a str {
    self.css.get(token.start..token.end).unwrap_or("")
  }

  /// The concatenated text of `tokens`.
  fn join(&self, tokens: &[Token]) -> String {
    tokens.iter().map(|&t| self.text(t)).collect()
  }

  /// Add `token` to the pending `raws.before`.
  fn add_spaces(&mut self, token: Token) {
    self.spaces.get_or_insert(token.start);
  }

  /// PostCSS's `parse`: read statements until the input runs out.
  fn parse(&mut self) {
    while let Some(token) = self.tokenizer.next_token() {
      match token.kind {
        Tok::Space => self.add_spaces(token),
        Tok::Semicolon => self.free_semicolon(token),
        Tok::CloseCurly => self.end(token),
        Tok::Comment | Tok::InlineComment => self.comment(token),
        Tok::AtWord => self.atrule(token),
        Tok::OpenCurly => self.empty_rule(token),
        _ => self.other(token),
      }
    }
    // Close any block left open at the end of the input.
    while let Some(open) = self.current {
      self.nodes[open].end = self.css.len();
      self.record_semicolon(Some(open));
      self.current = self.nodes[open].parent;
    }
    self.record_semicolon(None);
  }

  /// PostCSS's `raws.semicolon` for the block of `node` (the root for
  /// `None`), taken when the block closes.
  fn record_semicolon(&mut self, node: Option<usize>) {
    let semicolon = std::mem::take(&mut self.semicolon);
    match node {
      Some(node) => {
        let has_children = self.nodes[node]
          .children
          .as_ref()
          .is_some_and(|c| !c.is_empty());
        self.nodes[node].semicolon = has_children && semicolon;
      }
      None => self.root_semicolon = !self.root.is_empty() && semicolon,
    }
  }

  /// PostCSS's `init`: add a node starting at `start` to the open block,
  /// taking the pending spaces as its `raws.before`.
  fn init(&mut self, kind: NodeKind, start: usize) -> usize {
    let index = self.nodes.len();
    let before_start = self.spaces.take().unwrap_or(start).min(start);
    let position = match self.current {
      Some(parent) => self.nodes[parent].children.as_ref().map_or(0, Vec::len),
      None => self.root.len(),
    };
    self.nodes.push(Node {
      kind,
      parent: self.current,
      index: position,
      children: None,
      start,
      end: start,
      before: before_start..start,
      block_open: None,
      name: String::new(),
      params: String::new(),
      after_name: String::new(),
      inline: false,
      mixin: false,
      variable: false,
      extend: false,
      name_span: start..start,
      value_span: start..start,
      value: String::new(),
      important: false,
      semicolon: false,
    });
    self.own_semicolon.push(false);
    if kind != NodeKind::Comment {
      self.semicolon = false;
    }
    match self.current {
      Some(parent) => self.nodes[parent]
        .children
        .get_or_insert_with(Vec::new)
        .push(index),
      None => self.root.push(index),
    }
    index
  }

  /// Open the block of `node` at the `{` at `open`.
  fn open_block(&mut self, node: usize, open: usize) {
    self.nodes[node].children.get_or_insert_with(Vec::new);
    self.nodes[node].block_open = Some(open);
    self.current = Some(node);
  }

  /// PostCSS's `freeSemicolon`: a `;` that ends no statement.  A rule takes
  /// the first one after it; otherwise it joins the next `raws.before`.
  fn free_semicolon(&mut self, token: Token) {
    self.add_spaces(token);
    let siblings = match self.current {
      Some(parent) => self.nodes[parent].children.as_deref().unwrap_or(&[]),
      None => &self.root,
    };
    if let Some(&prev) = siblings.last()
      && self.nodes[prev].kind == NodeKind::Rule
      && !self.own_semicolon[prev]
    {
      self.own_semicolon[prev] = true;
      self.spaces = None;
      self.nodes[prev].end = token.end;
    }
  }

  /// PostCSS's `end`: a `}` closes the open block.  A stray one at the top
  /// level is ignored, as the safe parser does.
  fn end(&mut self, token: Token) {
    self.spaces = None;
    if let Some(open) = self.current {
      self.nodes[open].end = token.end;
      self.record_semicolon(Some(open));
      self.current = self.nodes[open].parent;
    } else {
      self.semicolon = false;
    }
  }

  /// PostCSS's `comment`, with the SCSS and Less `//` forms.
  fn comment(&mut self, token: Token) {
    let node = self.init(NodeKind::Comment, token.start);
    self.nodes[node].end = token.end;
    self.nodes[node].inline = token.kind == Tok::InlineComment;
    let raw = self.text(token);
    let inner = if self.nodes[node].inline {
      raw.get(2..).unwrap_or("")
    } else {
      raw
        .get(2..)
        .map(|rest| rest.strip_suffix("*/").unwrap_or(rest))
        .unwrap_or("")
    };
    self.nodes[node].name = inner.trim().to_string();
  }

  /// PostCSS's `emptyRule`: a `{` with no selector.
  fn empty_rule(&mut self, token: Token) {
    let node = self.init(NodeKind::Rule, token.start);
    self.open_block(node, token.start);
  }

  /// PostCSS's `atrule`, with the SCSS name joining and the Less
  /// interpolation, import and variable handling.
  fn atrule(&mut self, token: Token) {
    let mut token = token;
    match self.flavor {
      Flavor::Less => {
        if self.less_interpolation(token) {
          return;
        }
      }
      Flavor::Scss => {
        // `@include#{...}`: words right after the at-word are part of the name.
        while let Some(next) = self.tokenizer.next_token() {
          if next.kind == Tok::Word && next.start == token.end {
            token.end = next.end;
          } else {
            self.tokenizer.back(next);
            break;
          }
        }
      }
      Flavor::Css => {}
    }
    self.atrule_from(token, self.text(token).get(1..).unwrap_or(""));
  }

  /// The body of [`Self::atrule`] for an at-rule named `name` starting at
  /// `token`.
  fn atrule_from(&mut self, token: Token, name: &str) {
    let node = self.init(NodeKind::AtRule, token.start);
    self.nodes[node].name = name.to_string();
    self.nodes[node].end = token.end;
    let name_start = token.end - name.len().min(token.end - token.start);
    self.nodes[node].name_span = name_start..token.end;
    self.nodes[node].value_span = token.end..token.end;
    let mut params: Vec<Token> = Vec::new();
    let mut brackets: Vec<Tok> = Vec::new();
    let mut open = None;
    let mut last = false;
    while !self.tokenizer.end_of_file() {
      let Some(token) = self.tokenizer.next_token() else {
        break;
      };
      match token.kind {
        Tok::OpenParen => brackets.push(Tok::CloseParen),
        Tok::OpenSquare => brackets.push(Tok::CloseSquare),
        Tok::OpenCurly if !brackets.is_empty() => brackets.push(Tok::CloseCurly),
        kind if Some(&kind) == brackets.last() => {
          brackets.pop();
        }
        _ => {}
      }
      if brackets.is_empty() {
        match token.kind {
          Tok::Semicolon => {
            self.nodes[node].end = token.end;
            self.semicolon = true;
            break;
          }
          Tok::OpenCurly => {
            open = Some(token.start);
            break;
          }
          Tok::CloseCurly => {
            if let Some(prev) = params.iter().rev().find(|t| t.kind != Tok::Space) {
              self.nodes[node].end = prev.end;
            }
            self.end(token);
            break;
          }
          _ => params.push(token),
        }
      } else {
        params.push(token);
      }
      if self.tokenizer.end_of_file() {
        last = true;
        break;
      }
    }

    let between = trailing_space_or_comments(&params);
    let between_start = params.get(params.len() - between).map(|t| t.start);
    params.truncate(params.len() - between);
    if params.is_empty() {
      self.nodes[node].after_name = String::new();
    } else {
      let after_name = leading_space_or_comments(&params);
      self.nodes[node].after_name = self.join(&params[..after_name]);
      let params = &params[after_name..];
      if let (Some(first), Some(last)) = (params.first(), params.last()) {
        self.nodes[node].value_span = first.start..last.end;
      }
      self.nodes[node].params = self.raw_value(params, false);
      if last {
        if let Some(prev) = params.last() {
          self.nodes[node].end = prev.end;
        }
        self.spaces = between_start;
      }
    }
    if self.flavor == Flavor::Less {
      self.less_variable(node);
    }
    if let Some(open) = open {
      self.open_block(node, open);
    }
  }

  /// postcss-less's `variable`: `@name: value` is a Less variable.
  fn less_variable(&mut self, node: usize) {
    let n = &mut self.nodes[node];
    if !n.name.ends_with(':') {
      return;
    }
    n.name.pop();
    n.name_span.end = n.name_span.end.saturating_sub(1).max(n.name_span.start);
    n.after_name.insert(0, ':');
    n.variable = true;
  }

  /// postcss-less's `interpolation`: `@{name}` is a word, not an at-rule.
  /// Merges the tokens into one word, pushes it back and returns `true`.
  fn less_interpolation(&mut self, token: Token) -> bool {
    let Some(next) = self.tokenizer.next_token() else {
      return false;
    };
    if token.end - token.start > 1 || next.kind != Tok::OpenCurly {
      self.tokenizer.back(next);
      return false;
    }
    let mut last = next;
    let mut stop = self.tokenizer.next_token();
    while let Some(t) = stop {
      if !matches!(t.kind, Tok::Word | Tok::CloseCurly) {
        break;
      }
      last = t;
      stop = self.tokenizer.next_token();
    }
    if let Some(stop) = stop {
      self.tokenizer.back(stop);
    }
    self.tokenizer.back(Token {
      kind: Tok::Word,
      start: token.start,
      end: last.end,
    });
    true
  }

  /// PostCSS's `other`: a declaration or a rule, told apart by what ends it.
  fn other(&mut self, start: Token) {
    if self.flavor == Flavor::Less && self.less_inline_comment(start) {
      return;
    }
    let custom_property = self.text(start).starts_with("--");
    let mut tokens: Vec<Token> = Vec::new();
    let mut brackets: Vec<Tok> = Vec::new();
    let mut colon = false;
    let mut end = false;
    let mut token = Some(start);
    while let Some(t) = token {
      tokens.push(t);
      if matches!(t.kind, Tok::OpenParen | Tok::OpenSquare) {
        brackets.push(if t.kind == Tok::OpenParen {
          Tok::CloseParen
        } else {
          Tok::CloseSquare
        });
      } else if custom_property && colon && t.kind == Tok::OpenCurly {
        brackets.push(Tok::CloseCurly);
      } else if brackets.is_empty() {
        match t.kind {
          Tok::Semicolon => {
            if colon {
              self.decl(tokens);
              return;
            }
            break;
          }
          Tok::OpenCurly => {
            self.rule(tokens);
            return;
          }
          Tok::CloseCurly => {
            tokens.pop();
            self.tokenizer.back(t);
            end = true;
            break;
          }
          Tok::Colon => colon = true,
          _ => {}
        }
      } else if Some(&t.kind) == brackets.last() {
        brackets.pop();
      }
      token = self.tokenizer.next_token();
    }
    if self.tokenizer.end_of_file() {
      end = true;
    }
    if end && colon {
      if !custom_property {
        while let Some(&last) = tokens.last() {
          if !last.is_space_or_comment() {
            break;
          }
          tokens.pop();
          self.tokenizer.back(last);
        }
      }
      self.decl(tokens);
    } else {
      self.unknown_word(tokens);
    }
  }

  /// postcss-less's `isInlineComment`: a statement starting with `//` is a
  /// comment to the end of the line.
  fn less_inline_comment(&mut self, token: Token) -> bool {
    let text = self.text(token);
    let starts_comment =
      text.starts_with("//") || (text == "/" && self.css.as_bytes().get(token.end) == Some(&b'/'));
    if !starts_comment {
      return false;
    }
    // Read tokens up to the line break; the one holding it goes back.
    let mut end = token.end;
    while let Some(next) = self.tokenizer.next_token() {
      let next_text = self.text(next);
      if let Some(newline) = next_text.find('\n') {
        if next.kind == Tok::Space {
          self.tokenizer.back(next);
        } else {
          // A token running past the line break (an unclosed string): the
          // comment stops at the break and the rest is read again.
          let cut = next.start + newline;
          end = end.max(cut);
          self.tokenizer.pos = cut;
          self.tokenizer.returned.clear();
        }
        break;
      }
      end = next.end;
    }
    let end = self.css[..end].trim_end_matches('\r').len().max(token.end);
    let node = self.init(NodeKind::Comment, token.start);
    self.nodes[node].end = end;
    self.nodes[node].inline = true;
    self.nodes[node].name = self
      .css
      .get(token.start + 2..end)
      .unwrap_or("")
      .trim()
      .to_string();
    true
  }

  /// PostCSS's `unknownWord`.  Less reads `.mixin();` as an at-rule here;
  /// anything else is folded into the next `raws.before`, as the safe
  /// parser does.
  fn unknown_word(&mut self, tokens: Vec<Token>) {
    let Some(&first) = tokens.first() else {
      return;
    };
    if self.flavor == Flavor::Less && is_less_mixin_token(self.text(first)) {
      for &t in tokens.iter().rev() {
        self.tokenizer.back(t);
      }
      let Some(first) = self.tokenizer.next_token() else {
        return;
      };
      let name = self.text(first).get(1..).unwrap_or("");
      self.atrule_from(first, name);
      if let Some(node) = self.nodes.len().checked_sub(1) {
        self.nodes[node].mixin = true;
      }
      return;
    }
    self.add_spaces(first);
  }

  /// PostCSS's `rule`, with postcss-scss's nested declarations
  /// (`margin: 0 { top: 1px }`) and postcss-less's interpolation and
  /// `:extend` handling.
  fn rule(&mut self, mut tokens: Vec<Token>) {
    if self.flavor == Flavor::Less && tokens.len() >= 2 {
      let last = tokens[tokens.len() - 1];
      let prev = tokens[tokens.len() - 2];
      if prev.kind == Tok::AtWord && last.kind == Tok::OpenCurly {
        self.tokenizer.back(last);
        if self.less_interpolation(prev) {
          let merged = self.tokenizer.next_token();
          tokens.truncate(tokens.len() - 2);
          tokens.extend(merged);
          for &t in tokens.iter().rev() {
            self.tokenizer.back(t);
          }
          return;
        }
        // postcss-less leaves the `{` pushed back here, so it opens a
        // nested empty rule as well; the tree keeps that quirk.
      }
    }
    if self.flavor == Flavor::Scss && self.is_scss_nested_declaration(&tokens) {
      self.nested_declaration(tokens);
      return;
    }
    let Some(open) = tokens.pop() else {
      return;
    };
    let Some(&first) = tokens.first() else {
      let node = self.init(NodeKind::Rule, open.start);
      self.open_block(node, open.start);
      return;
    };
    let node = self.init(NodeKind::Rule, first.start);
    let between = trailing_space_or_comments(&tokens);
    tokens.truncate(tokens.len() - between);
    if let Some(last) = tokens.last() {
      self.nodes[node].name_span = first.start..last.end;
    }
    let selector = self.raw_value(&tokens, false);
    if self.flavor == Flavor::Less && selector.to_ascii_lowercase().contains(":extend(") {
      self.nodes[node].extend = has_extend_call(&selector, true);
    }
    self.nodes[node].name = selector;
    self.open_block(node, open.start);
  }

  /// postcss-scss's test for `prop: value {`: a colon before the first line
  /// break, then a value that does not look like a selector.
  fn is_scss_nested_declaration(&self, tokens: &[Token]) -> bool {
    let mut with_colon = false;
    let mut depth = 0i32;
    let mut value = String::new();
    for &t in tokens {
      if with_colon {
        if !t.is_comment() && t.kind != Tok::OpenCurly {
          value.push_str(self.text(t));
        }
      } else if t.kind == Tok::Space && self.text(t).contains('\n') {
        break;
      } else if t.kind == Tok::OpenParen {
        depth += 1;
      } else if t.kind == Tok::CloseParen {
        depth -= 1;
      } else if depth == 0 && t.kind == Tok::Colon {
        with_colon = true;
      }
    }
    let starts_like_selector = value
      .chars()
      .next()
      .is_some_and(|c| c == '#' || c == ':' || c == '-' || c.is_ascii_alphabetic());
    with_colon && !value.trim().is_empty() && !starts_like_selector
  }

  /// postcss-scss's `NestedDeclaration`: a declaration with a block.
  fn nested_declaration(&mut self, mut tokens: Vec<Token>) {
    let Some(open) = tokens.pop() else {
      return;
    };
    let Some(&first) = tokens.first() else {
      return;
    };
    let node = self.init(NodeKind::Decl, first.start);
    let word = tokens.iter().position(|t| t.kind == Tok::Word).unwrap_or(0);
    self.start_declaration(node, &tokens[word..]);
    self.nodes[node].end = tokens
      .iter()
      .rev()
      .find(|t| t.kind != Tok::Space)
      .map_or(first.end, |t| t.end);
    self.open_block(node, open.start);
  }

  /// PostCSS's `decl`.
  fn decl(&mut self, mut tokens: Vec<Token>) {
    let Some(&first) = tokens.first() else {
      return;
    };
    let node = self.init(NodeKind::Decl, first.start);
    if tokens.last().is_some_and(|t| t.kind == Tok::Semicolon) {
      self.nodes[node].end = tokens.last().map_or(first.end, |t| t.end);
      tokens.pop();
      self.semicolon = true;
    } else {
      self.nodes[node].end = tokens
        .iter()
        .rev()
        .find(|t| t.kind != Tok::Space)
        .map_or(first.end, |t| t.end);
    }
    let Some(word) = tokens.iter().position(|t| t.kind == Tok::Word) else {
      return;
    };
    self.start_declaration(node, &tokens[word..]);
    let value = self.nodes[node].params.clone();
    if self.flavor == Flavor::Less && has_extend_call(&value, false) {
      self.nodes[node].extend = true;
    }
  }

  /// Set a declaration's start, property and value from `tokens`, which
  /// begin at its first word.
  fn start_declaration(&mut self, node: usize, tokens: &[Token]) {
    let Some(&word) = tokens.first() else {
      return;
    };
    self.nodes[node].start = word.start;
    self.nodes[node].before.end = word.start;
    let prop_end = tokens
      .iter()
      .position(|t| matches!(t.kind, Tok::Colon | Tok::Space) || t.is_comment())
      .unwrap_or(tokens.len());
    let mut prop = self.join(&tokens[..prop_end]);
    let mut prop_start = word.start;
    if prop.starts_with('_') || prop.starts_with('*') {
      // The hack character joins `raws.before`; the node still starts on it.
      self.nodes[node].before.end = word.start + 1;
      prop.remove(0);
      prop_start += 1;
    }
    let prop_stop = prop_end
      .checked_sub(1)
      .map_or(prop_start, |last| tokens[last].end.max(prop_start));
    self.nodes[node].name_span = prop_start..prop_stop;
    let custom_property = prop.starts_with("--");
    self.nodes[node].name = prop;
    let colon = tokens.iter().position(|t| t.kind == Tok::Colon);
    if let Some(colon) = colon {
      self.nodes[node].params = self.join(&tokens[colon + 1..]).trim().to_string();
      self.declaration_value(
        node,
        tokens[colon].end,
        &tokens[colon + 1..],
        custom_property,
      );
    }
  }

  /// PostCSS's handling of what follows a declaration's colon: leading
  /// whitespace and comments go to `raws.between` (unless nothing else
  /// follows), a trailing `!important` is split off, and the rest is the
  /// raw value, cleaned into `decl.value`.
  fn declaration_value(
    &mut self,
    node: usize,
    colon_end: usize,
    rest: &[Token],
    custom_property: bool,
  ) {
    let first_spaces = leading_space_or_comments(rest);
    let mut tokens: Vec<Token> = rest[first_spaces..].to_vec();
    let mut important = false;
    let mut i = tokens.len();
    while i > 0 {
      i -= 1;
      let token = tokens[i];
      let lower = self.text(token).to_ascii_lowercase();
      if lower == "!important" {
        important = true;
        tokens.truncate(i);
        while tokens.last().is_some_and(|t| t.kind == Tok::Space) {
          tokens.pop();
        }
        break;
      }
      if lower == "important" {
        // Collect tokens from the end until the text gathered starts with
        // `!` and the next one is not whitespace, as PostCSS does.
        let mut cache = tokens.clone();
        let mut gathered = String::new();
        let mut j = i;
        while j > 0 {
          if gathered.trim_start().starts_with('!') && cache[j].kind != Tok::Space {
            break;
          }
          let Some(popped) = cache.pop() else {
            break;
          };
          gathered.insert_str(0, self.text(popped));
          j -= 1;
        }
        if gathered.trim_start().starts_with('!') {
          important = true;
          tokens = cache;
        }
      }
      if !token.is_space_or_comment() {
        break;
      }
    }
    let has_word = tokens.iter().any(|t| !t.is_space_or_comment());
    // Without a word, the leading whitespace and comments stay in the value.
    let value: &[Token] = if has_word {
      &tokens
    } else {
      &rest[..first_spaces + tokens.len()]
    };
    self.nodes[node].important = important;
    self.nodes[node].value_span = match (value.first(), value.last()) {
      (Some(first), Some(last)) => first.start..last.end,
      _ => colon_end..colon_end,
    };
    self.nodes[node].value = self.raw_value(value, custom_property);
  }

  /// PostCSS's `raw`: the clean value of `tokens`, with comments between
  /// spaces dropped and a trailing space removed.
  fn raw_value(&self, tokens: &[Token], custom_property: bool) -> String {
    let mut value = String::new();
    for (i, &t) in tokens.iter().enumerate() {
      if t.kind == Tok::Space && i == tokens.len() - 1 && !custom_property {
        continue;
      }
      if t.is_comment() {
        let prev_safe = i == 0 || tokens[i - 1].kind == Tok::Space;
        let next_safe = tokens.get(i + 1).is_none_or(|n| n.kind == Tok::Space);
        if !prev_safe && !next_safe && !value.ends_with(',') {
          value.push_str(self.text(t));
        }
        continue;
      }
      value.push_str(self.text(t));
    }
    value
  }
}

/// How many tokens at the end of `tokens` are spaces or comments
/// (PostCSS's `spacesAndCommentsFromEnd`).
fn trailing_space_or_comments(tokens: &[Token]) -> usize {
  tokens
    .iter()
    .rev()
    .take_while(|t| t.is_space_or_comment())
    .count()
}

/// How many tokens at the start of `tokens` are spaces or comments
/// (PostCSS's `spacesAndCommentsFromStart`).
fn leading_space_or_comments(tokens: &[Token]) -> usize {
  tokens
    .iter()
    .take_while(|t| t.is_space_or_comment())
    .count()
}

/// postcss-less's `isMixinToken`: a word starting with `.` or `#` that is
/// neither a hex colour nor a fractional number.
fn is_less_mixin_token(text: &str) -> bool {
  let Some(first) = text.chars().next() else {
    return false;
  };
  if first != '.' && first != '#' {
    return false;
  }
  let hex = text
    .strip_prefix('#')
    .is_some_and(|h| (h.len() == 6 || h.len() == 3) && h.bytes().all(|b| b.is_ascii_hexdigit()));
  let fraction = text
    .as_bytes()
    .windows(2)
    .any(|w| w[0] == b'.' && w[1].is_ascii_digit());
  !hex && !fraction
}

/// postcss-less's `/extend\(.+\)/i` (declarations) and `/:extend\(.+\)/i`
/// (rules).
fn has_extend_call(text: &str, with_colon: bool) -> bool {
  let lower = text.to_ascii_lowercase();
  let needle = if with_colon { ":extend(" } else { "extend(" };
  lower.match_indices(needle).any(|(at, _)| {
    let rest = &lower[at + needle.len()..];
    let line = rest.split(['\n', '\r']).next().unwrap_or("");
    line.char_indices().any(|(i, c)| i > 0 && c == ')')
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The kinds and texts of the top-level nodes.
  fn top(tree: &PostcssTree) -> Vec<(NodeKind, String)> {
    tree
      .root
      .iter()
      .map(|&i| (tree.nodes[i].kind, tree.text(i).to_string()))
      .collect()
  }

  #[test]
  fn css_statements_keep_comments_and_unknown_at_rules() {
    let tree = PostcssTree::parse(
      "/* a */\n@foo bar;\na {\n  color: red; /* b */\n\n  @mixin x;\n}",
      Syntax::Css,
    );
    assert_eq!(
      top(&tree),
      vec![
        (NodeKind::Comment, "/* a */".to_string()),
        (NodeKind::AtRule, "@foo bar;".to_string()),
        (
          NodeKind::Rule,
          "a {\n  color: red; /* b */\n\n  @mixin x;\n}".to_string()
        ),
      ]
    );
    let rule = tree.root[2];
    let kids = tree.nodes[rule].children.clone().unwrap();
    assert_eq!(kids.len(), 3);
    assert_eq!(tree.text(kids[0]), "color: red;");
    assert_eq!(tree.before(kids[0]), "\n  ");
    assert_eq!(tree.before(kids[1]), " ");
    assert_eq!(tree.before(kids[2]), "\n\n  ");
    assert_eq!(tree.nodes[kids[2]].name, "mixin");
    assert_eq!(tree.nodes[kids[2]].params, "x");
  }

  #[test]
  fn free_semicolons_join_the_next_before_unless_a_rule_takes_one() {
    let tree = PostcssTree::parse("@media {};\n@import 'x';\na {};  b {}", Syntax::Css);
    let [media, import, a, b] = tree.root[..] else {
      panic!("{:?}", tree.root);
    };
    assert_eq!(tree.text(media), "@media {}");
    assert_eq!(tree.before(import), ";\n");
    assert_eq!(tree.text(a), "a {};");
    assert_eq!(tree.before(b), "  ");
  }

  #[test]
  fn scss_line_comments_and_interpolation() {
    let tree = PostcssTree::parse(
      "a {\n  // note\n  #{$p}-x: 1;\n  b: c; // trailing\n}",
      Syntax::Scss,
    );
    let rule = tree.root[0];
    let kids = tree.nodes[rule].children.clone().unwrap();
    let kinds: Vec<_> = kids.iter().map(|&k| tree.nodes[k].kind).collect();
    assert_eq!(
      kinds,
      vec![
        NodeKind::Comment,
        NodeKind::Decl,
        NodeKind::Decl,
        NodeKind::Comment
      ]
    );
    assert!(tree.nodes[kids[0]].inline);
    assert_eq!(tree.text(kids[0]), "// note");
    assert_eq!(tree.nodes[kids[1]].name, "#{$p}-x");
    assert_eq!(tree.end_line(kids[2]), tree.start_line(kids[3]));
  }

  #[test]
  fn less_mixins_variables_and_comments() {
    let tree = PostcssTree::parse(
      "@var: 1px;\n.a {\n  .mixin();\n  // c\n  color: red;\n}",
      Syntax::Less,
    );
    let var = tree.root[0];
    assert!(tree.nodes[var].variable);
    assert_eq!(tree.nodes[var].name, "var");
    let kids = tree.nodes[tree.root[1]].children.clone().unwrap();
    assert!(tree.nodes[kids[0]].mixin);
    assert_eq!(tree.nodes[kids[1]].kind, NodeKind::Comment);
    assert!(tree.nodes[kids[1]].inline);
    assert_eq!(tree.text(kids[1]), "// c");
    assert_eq!(tree.nodes[kids[2]].name, "color");
  }

  #[test]
  fn removal_ranges_keep_the_semicolons_postcss_prints() {
    let source = "a { color: red; top: 0; left: 1px }";
    let tree = PostcssTree::parse(source, Syntax::Css);
    let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
    let removed = |i: usize| {
      let mut text = source.to_string();
      for range in tree.removal_ranges(i).into_iter().rev() {
        text.replace_range(range, "");
      }
      text
    };
    assert_eq!(removed(kids[0]), "a { top: 0; left: 1px }");
    assert_eq!(removed(kids[2]), "a { color: red; top: 0 }");
    let source = "a { color: red; /* c */ top: 0; }";
    let tree = PostcssTree::parse(source, Syntax::Css);
    let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
    assert_eq!(tree.removal_ranges(kids[2]), vec![23..31]);
  }

  #[test]
  fn first_nested_skips_comments_on_the_brace_line() {
    let tree = PostcssTree::parse("a { /* x */\n  b: c;\n  d: e; }", Syntax::Css);
    let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
    assert!(tree.is_first_nested(kids[1]));
    assert!(!tree.is_first_nested(kids[2]));
    assert!(
      !tree.is_after_comment(kids[1]),
      "the comment shares the brace line"
    );
  }

  /// `(prop, raw value, important, clean value)` for every declaration.
  fn decls(source: &str, syntax: Syntax) -> Vec<(String, String, bool, String)> {
    let tree = PostcssTree::parse(source, syntax);
    tree
      .decls()
      .map(|d| {
        (
          source[d.name_span.clone()].to_string(),
          source[d.value_span.clone()].to_string(),
          d.important,
          d.value.clone(),
        )
      })
      .collect()
  }

  #[test]
  fn declarations_keep_their_raw_value_and_important() {
    assert_eq!(
      decls(
        "a { color: red ; margin : 0 /* c */ 1px !important; top: 1px ! important }",
        Syntax::Css
      ),
      vec![
        ("color".into(), "red ".into(), false, "red".into()),
        (
          "margin".into(),
          "0 /* c */ 1px".into(),
          true,
          "0  1px".into()
        ),
        ("top".into(), "1px".into(), true, "1px".into()),
      ]
    );
    assert_eq!(
      decls("a { *zoom: 1; color: /* x */; }", Syntax::Css),
      vec![
        ("zoom".into(), "1".into(), false, "1".into()),
        ("color".into(), " /* x */".into(), false, " ".into()),
      ]
    );
    assert_eq!(
      decls("$a: rgb(0 0 0 / 50%); .b { #{$p}: 1px; }", Syntax::Scss),
      vec![
        (
          "$a".into(),
          "rgb(0 0 0 / 50%)".into(),
          false,
          "rgb(0 0 0 / 50%)".into()
        ),
        ("#{$p}".into(), "1px".into(), false, "1px".into()),
      ]
    );
  }

  #[test]
  fn at_rule_params_and_selectors_are_located() {
    let source =
      "@import url(a;b.css) /* c */ screen;\n@media (x: 1) { a:hover /* c */ { top: 0; } }";
    let tree = PostcssTree::parse(source, Syntax::Css);
    let import = &tree.nodes[tree.root[0]];
    assert_eq!(&source[import.name_span.clone()], "import");
    assert_eq!(
      &source[import.value_span.clone()],
      "url(a;b.css) /* c */ screen"
    );
    assert_eq!(import.params, "url(a;b.css)  screen");
    let media = &tree.nodes[tree.root[1]];
    assert_eq!(&source[media.value_span.clone()], "(x: 1)");
    let rule = &tree.nodes[tree.children_of(Some(tree.root[1]))[0]];
    assert_eq!(&source[rule.name_span.clone()], "a:hover");
    assert!(rule.semicolon);
    assert!(!media.semicolon, "its last child is a rule");
  }

  #[test]
  fn raws_semicolon_follows_the_last_non_comment_node() {
    let tree = PostcssTree::parse(
      "a { color: red; /* c */ } b { color: red /* c */ } top: 0;",
      Syntax::Scss,
    );
    let a = &tree.nodes[tree.root[0]];
    let b = &tree.nodes[tree.root[1]];
    assert!(a.semicolon);
    assert!(!b.semicolon);
    assert!(tree.root_semicolon);
  }

  #[test]
  fn never_panics_on_broken_or_multibyte_input() {
    for source in [
      "a { color: red",
      "}}} a {",
      "@",
      "a { b: \"unclosed",
      "/* unclosed",
      "a\\",
      "a { b: url(é",
      "é { ü: ö; } /* ß */ @ä ü;",
      "\\é",
      "#{",
      "a { b: (c; }",
    ] {
      for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
        let tree = PostcssTree::parse(source, syntax);
        for i in 0..tree.nodes.len() {
          let _ = (tree.text(i), tree.before(i), tree.end_line(i));
        }
      }
    }
  }
}
