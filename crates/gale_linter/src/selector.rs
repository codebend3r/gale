//! A small CSS selector parser shared by the selector rules.
//!
//! Gale's selector rules historically scanned raw selector strings. Rules that
//! need to reason about selector *structure*, which compound selector a
//! pseudo-class belongs to, what sits inside `:is()`, whether a combinator
//! follows a pseudo-element, need an AST instead. This module provides one.
//!
//! The parser is deliberately permissive about things CSS allows but Gale does
//! not care about, and deliberately strict about the error classes that
//! `selector-no-invalid` reports, so that its diagnostics can carry the same
//! reasons and offsets Stylelint produces.

/// A combinator between two compound selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
  /// Whitespace.
  Descendant,
  /// `>`
  Child,
  /// `+`
  NextSibling,
  /// `~`
  SubsequentSibling,
  /// `||`
  Column,
}

/// A single component of a compound selector.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectorNode {
  /// A type selector such as `a` or `svg|path`.
  Tag { name: String, offset: usize },
  /// `*`
  Universal { offset: usize },
  /// `.foo`
  Class { name: String, offset: usize },
  /// `#foo`
  Id { name: String, offset: usize },
  /// `[foo="bar"]`, stored with its brackets.
  Attribute { raw: String, offset: usize },
  /// `:hover`, `:is(a, b)`.
  PseudoClass {
    name: String,
    /// The parsed argument, when the pseudo-class was written with `()`.
    args: Option<SelectorList>,
    /// The raw text between the parentheses, when present.
    raw_args: Option<String>,
    offset: usize,
    length: usize,
  },
  /// `::before`, `::slotted(a)`.
  PseudoElement {
    name: String,
    args: Option<SelectorList>,
    raw_args: Option<String>,
    offset: usize,
    length: usize,
  },
  /// `&`
  Nesting { offset: usize },
  /// A combinator separating compound selectors.
  Combinator {
    kind: Combinator,
    offset: usize,
    length: usize,
  },
  /// `/* ... */` inside a selector.
  Comment { raw: String, offset: usize },
}

impl SelectorNode {
  /// The byte offset of this node within the selector source.
  pub fn offset(&self) -> usize {
    match self {
      SelectorNode::Tag { offset, .. }
      | SelectorNode::Universal { offset }
      | SelectorNode::Class { offset, .. }
      | SelectorNode::Id { offset, .. }
      | SelectorNode::Attribute { offset, .. }
      | SelectorNode::PseudoClass { offset, .. }
      | SelectorNode::PseudoElement { offset, .. }
      | SelectorNode::Nesting { offset }
      | SelectorNode::Combinator { offset, .. }
      | SelectorNode::Comment { offset, .. } => *offset,
    }
  }
}

/// One selector in a selector list, e.g. the `a > b` of `a > b, .c`.
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
  pub nodes: Vec<SelectorNode>,
  /// Byte offset of the first non-whitespace character.
  pub offset: usize,
  /// Exclusive byte offset of the last non-whitespace character.
  pub end: usize,
}

/// A comma-separated list of selectors.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorList {
  pub selectors: Vec<Selector>,
}

/// Why a selector failed to parse, phrased the way `css-tree` phrases it so
/// that `selector-no-invalid` can report the same reason Stylelint does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
  /// The lower-cased reason, e.g. `"identifier is expected"`.
  pub reason: String,
  /// Byte offset of the offending character within the selector source.
  pub offset: usize,
}

/// Pseudo-classes and pseudo-elements whose argument is a selector list.
///
/// For the `nth-*` pseudo-classes this refers to the `of S` clause; the An+B
/// part is validated separately.
const SELECTOR_ARGUMENT_PSEUDOS: &[&str] = &[
  "any",
  "cue",
  "cue-region",
  "current",
  "future",
  "has",
  "host",
  "host-context",
  "is",
  "matches",
  "not",
  "nth-child",
  "nth-last-child",
  "past",
  "slotted",
  "where",
  "-moz-any",
  "-webkit-any",
];

/// Pseudo-classes whose argument uses An+B notation.
const ANB_PSEUDOS: &[&str] = &[
  "nth-child",
  "nth-col",
  "nth-last-child",
  "nth-last-col",
  "nth-last-of-type",
  "nth-of-type",
];

/// Whether `c` can begin a CSS identifier.
fn is_ident_start(c: char) -> bool {
  c.is_alphabetic() || c == '_' || c == '-' || c == '\\' || !c.is_ascii()
}

/// Whether `c` can continue a CSS identifier.
fn is_ident_char(c: char) -> bool {
  c.is_alphanumeric() || c == '_' || c == '-' || c == '\\' || !c.is_ascii()
}

/// A recursive-descent selector parser over the characters of one selector
/// list. `base` is added to every recorded offset so that arguments parsed out
/// of a nested `:is(...)` still carry offsets into the original source.
struct Parser<'a> {
  chars: Vec<(usize, char)>,
  source: &'a str,
  pos: usize,
  base: usize,
}

impl<'a> Parser<'a> {
  fn new(source: &'a str, base: usize) -> Self {
    Self {
      chars: source.char_indices().collect(),
      source,
      pos: 0,
      base,
    }
  }

  /// The character at the cursor.
  fn peek(&self) -> Option<char> {
    self.chars.get(self.pos).map(|(_, c)| *c)
  }

  /// The character `n` positions past the cursor.
  fn peek_at(&self, n: usize) -> Option<char> {
    self.chars.get(self.pos + n).map(|(_, c)| *c)
  }

  /// The absolute byte offset of the cursor.
  fn offset(&self) -> usize {
    self.base
      + self
        .chars
        .get(self.pos)
        .map(|(i, _)| *i)
        .unwrap_or(self.source.len())
  }

  /// The byte offset of the cursor relative to this parser's source.
  fn local_offset(&self) -> usize {
    self
      .chars
      .get(self.pos)
      .map(|(i, _)| *i)
      .unwrap_or(self.source.len())
  }

  /// Advance the cursor one character.
  fn bump(&mut self) {
    self.pos += 1;
  }

  /// Build an error at the cursor.
  fn error(&self, reason: &str) -> ParseError {
    ParseError {
      reason: reason.to_string(),
      offset: self.offset(),
    }
  }

  /// Consume an identifier, returning its text.
  fn take_ident(&mut self) -> Result<String, ParseError> {
    match self.peek() {
      Some(c) if is_ident_start(c) => {}
      _ => return Err(self.error("identifier is expected")),
    }
    let start = self.local_offset();
    while let Some(c) = self.peek() {
      if is_ident_char(c) {
        self.bump();
      } else {
        break;
      }
    }
    Ok(self.source[start..self.local_offset()].to_string())
  }

  /// Skip whitespace, returning whether any was consumed.
  fn skip_whitespace(&mut self) -> bool {
    let mut seen = false;
    while let Some(c) = self.peek() {
      if c.is_whitespace() {
        seen = true;
        self.bump();
      } else {
        break;
      }
    }
    seen
  }

  /// Consume a balanced parenthesised argument, returning its inner text and
  /// the absolute offset at which that text starts.
  fn take_parenthesised(&mut self) -> Result<(String, usize), ParseError> {
    // Cursor sits on '('.
    self.bump();
    let inner_start = self.local_offset();
    let inner_abs = self.offset();
    let mut depth = 1usize;
    while let Some(c) = self.peek() {
      match c {
        '(' => {
          depth += 1;
          self.bump();
        }
        ')' => {
          depth -= 1;
          if depth == 0 {
            let inner = self.source[inner_start..self.local_offset()].to_string();
            self.bump();
            return Ok((inner, inner_abs));
          }
          self.bump();
        }
        '\'' | '"' => self.skip_string(c),
        _ => self.bump(),
      }
    }
    Err(self.error("\")\" is expected"))
  }

  /// Skip over a quoted string, cursor sitting on the opening quote.
  fn skip_string(&mut self, quote: char) {
    self.bump();
    while let Some(c) = self.peek() {
      self.bump();
      if c == '\\' {
        self.bump();
      } else if c == quote {
        return;
      }
    }
  }

  /// Consume an attribute selector, validating its shape.
  fn take_attribute(&mut self) -> Result<SelectorNode, ParseError> {
    let start_abs = self.offset();
    let start = self.local_offset();
    self.bump(); // '['
    self.skip_whitespace();
    // Namespace prefix, e.g. `[*|foo]`.
    if self.peek() == Some('*') && self.peek_at(1) == Some('|') {
      self.bump();
      self.bump();
    }
    self.take_ident()?;
    // A namespaced attribute name, e.g. `[xlink|href]`.
    if self.peek() == Some('|') && self.peek_at(1) != Some('=') {
      self.bump();
      self.take_ident()?;
    }
    self.skip_whitespace();
    if let Some(c) = self.peek() {
      if c == ']' {
        self.bump();
        return Ok(SelectorNode::Attribute {
          raw: self.source[start..self.local_offset()].to_string(),
          offset: start_abs,
        });
      }
      // An operator: `=`, `~=`, `|=`, `^=`, `$=`, `*=`.
      if matches!(c, '~' | '|' | '^' | '$' | '*') {
        self.bump();
      }
      if self.peek() != Some('=') {
        return Err(self.error("\"=\" is expected"));
      }
      self.bump();
      self.skip_whitespace();
      match self.peek() {
        Some('"') => self.skip_string('"'),
        Some('\'') => self.skip_string('\''),
        Some(c) if is_ident_start(c) || c.is_ascii_digit() => {
          while let Some(c) = self.peek() {
            if is_ident_char(c) || c.is_ascii_digit() {
              self.bump();
            } else {
              break;
            }
          }
        }
        _ => return Err(self.error("identifier is expected")),
      }
      self.skip_whitespace();
      // An optional case-sensitivity flag, e.g. `[foo="bar" i]`.
      if matches!(self.peek(), Some(c) if is_ident_start(c)) {
        self.take_ident()?;
        self.skip_whitespace();
      }
    }
    if self.peek() != Some(']') {
      return Err(self.error("\"]\" is expected"));
    }
    self.bump();
    Ok(SelectorNode::Attribute {
      raw: self.source[start..self.local_offset()].to_string(),
      offset: start_abs,
    })
  }

  /// Consume a `/* ... */` comment.
  fn take_comment(&mut self) -> SelectorNode {
    let start_abs = self.offset();
    let start = self.local_offset();
    self.bump();
    self.bump();
    while self.peek().is_some() {
      if self.peek() == Some('*') && self.peek_at(1) == Some('/') {
        self.bump();
        self.bump();
        break;
      }
      self.bump();
    }
    SelectorNode::Comment {
      raw: self.source[start..self.local_offset()].to_string(),
      offset: start_abs,
    }
  }

  /// Consume a pseudo-class or pseudo-element.
  fn take_pseudo(&mut self) -> Result<SelectorNode, ParseError> {
    let start_abs = self.offset();
    self.bump(); // first ':'
    let is_element = if self.peek() == Some(':') {
      self.bump();
      true
    } else {
      false
    };
    let name = self.take_ident()?;
    let lower = name.to_ascii_lowercase();
    let (args, raw_args) = if self.peek() == Some('(') {
      let (raw, inner_abs) = self.take_parenthesised()?;
      let parsed = parse_pseudo_argument(&lower, &raw, inner_abs)?;
      (parsed, Some(raw))
    } else {
      (None, None)
    };
    let length = self.offset() - start_abs;
    if is_element {
      Ok(SelectorNode::PseudoElement {
        name,
        args,
        raw_args,
        offset: start_abs,
        length,
      })
    } else {
      Ok(SelectorNode::PseudoClass {
        name,
        args,
        raw_args,
        offset: start_abs,
        length,
      })
    }
  }

  /// Parse the whole selector list.
  fn parse(&mut self) -> Result<SelectorList, ParseError> {
    let mut selectors = Vec::new();
    loop {
      let selector = self.parse_selector()?;
      selectors.push(selector);
      self.skip_whitespace();
      match self.peek() {
        Some(',') => {
          self.bump();
        }
        Some(_) => return Err(self.error("unexpected input")),
        None => break,
      }
    }
    Ok(SelectorList { selectors })
  }

  /// Parse one selector up to a comma or the end of the source.
  fn parse_selector(&mut self) -> Result<Selector, ParseError> {
    let mut nodes: Vec<SelectorNode> = Vec::new();
    let mut pending_space = false;
    let start_abs;
    self.skip_whitespace();
    start_abs = self.offset();

    loop {
      let had_space = self.skip_whitespace();
      pending_space |= had_space;
      let Some(c) = self.peek() else { break };
      if c == ',' {
        break;
      }
      if c == ')' {
        return Err(self.error("unexpected input"));
      }

      // Combinators.
      if matches!(c, '>' | '+' | '~') || (c == '|' && self.peek_at(1) == Some('|')) {
        let offset = self.offset();
        let kind = match c {
          '>' => Combinator::Child,
          '+' => Combinator::NextSibling,
          '~' => Combinator::SubsequentSibling,
          _ => Combinator::Column,
        };
        self.bump();
        if kind == Combinator::Column {
          self.bump();
        }
        let length = self.offset() - offset;
        nodes.push(SelectorNode::Combinator {
          kind,
          offset,
          length,
        });
        pending_space = false;
        continue;
      }

      // A descendant combinator, only between two compound selectors. A
      // comment sitting inside the whitespace belongs to the one combinator
      // around it rather than splitting it in two.
      let after_comment_in_combinator = matches!(nodes.last(), Some(SelectorNode::Comment { .. }))
        && matches!(
          nodes.len().checked_sub(2).map(|i| &nodes[i]),
          Some(SelectorNode::Combinator { .. })
        );
      if pending_space
        && !nodes.is_empty()
        && !matches!(nodes.last(), Some(SelectorNode::Combinator { .. }))
        && !after_comment_in_combinator
      {
        nodes.push(SelectorNode::Combinator {
          kind: Combinator::Descendant,
          offset: self.offset(),
          length: 0,
        });
      }
      pending_space = false;

      let node = match c {
        '/' if self.peek_at(1) == Some('*') => self.take_comment(),
        '.' => {
          let offset = self.offset();
          self.bump();
          let name = self.take_ident()?;
          SelectorNode::Class { name, offset }
        }
        '#' => {
          let offset = self.offset();
          self.bump();
          let name = self.take_ident()?;
          SelectorNode::Id { name, offset }
        }
        '[' => self.take_attribute()?,
        ':' => self.take_pseudo()?,
        '&' => {
          let offset = self.offset();
          self.bump();
          SelectorNode::Nesting { offset }
        }
        '*' => {
          let offset = self.offset();
          self.bump();
          // A universal selector with a namespace, e.g. `*|a`.
          if self.peek() == Some('|') && self.peek_at(1) != Some('=') {
            self.bump();
            if self.peek() == Some('*') {
              self.bump();
              SelectorNode::Universal { offset }
            } else {
              let name = self.take_ident()?;
              SelectorNode::Tag {
                name: format!("*|{name}"),
                offset,
              }
            }
          } else {
            SelectorNode::Universal { offset }
          }
        }
        c if is_ident_start(c) => {
          let offset = self.offset();
          let mut name = self.take_ident()?;
          // A namespaced type selector, e.g. `svg|path`.
          if self.peek() == Some('|') && self.peek_at(1) != Some('=') {
            self.bump();
            if self.peek() == Some('*') {
              self.bump();
              name.push_str("|*");
            } else {
              let rest = self.take_ident()?;
              name.push('|');
              name.push_str(&rest);
            }
          }
          SelectorNode::Tag { name, offset }
        }
        _ => return Err(self.error("unexpected input")),
      };
      nodes.push(node);
    }

    if nodes.is_empty() {
      return Err(ParseError {
        reason: "selector is expected".to_string(),
        offset: self.offset(),
      });
    }

    // Trailing combinators are not part of the selector's reported extent.
    let end = nodes
      .iter()
      .rev()
      .find(|n| !matches!(n, SelectorNode::Combinator { .. }))
      .map(node_end)
      .unwrap_or(start_abs);

    Ok(Selector {
      nodes,
      offset: start_abs,
      end,
    })
  }
}

/// The exclusive end offset of a node.
fn node_end(node: &SelectorNode) -> usize {
  match node {
    SelectorNode::Tag { name, offset } => offset + name.len(),
    SelectorNode::Universal { offset } => offset + 1,
    SelectorNode::Class { name, offset } => offset + 1 + name.len(),
    SelectorNode::Id { name, offset } => offset + 1 + name.len(),
    SelectorNode::Attribute { raw, offset } => offset + raw.len(),
    SelectorNode::PseudoClass { offset, length, .. }
    | SelectorNode::PseudoElement { offset, length, .. } => offset + length,
    SelectorNode::Nesting { offset } => offset + 1,
    SelectorNode::Combinator { offset, length, .. } => offset + length,
    SelectorNode::Comment { raw, offset } => offset + raw.len(),
  }
}

/// Parse a pseudo-selector's argument into a selector list, when that
/// pseudo-selector takes one.
fn parse_pseudo_argument(
  lower_name: &str,
  raw: &str,
  inner_abs: usize,
) -> Result<Option<SelectorList>, ParseError> {
  if ANB_PSEUDOS.contains(&lower_name) {
    return parse_anb_argument(lower_name, raw, inner_abs);
  }
  if !SELECTOR_ARGUMENT_PSEUDOS.contains(&lower_name) {
    return Ok(None);
  }
  if raw.trim().is_empty() {
    return Ok(None);
  }
  let mut parser = Parser::new(raw, inner_abs);
  Ok(Some(parser.parse()?))
}

/// Validate the An+B part of an `nth-*` argument and parse any `of S` clause.
fn parse_anb_argument(
  lower_name: &str,
  raw: &str,
  inner_abs: usize,
) -> Result<Option<SelectorList>, ParseError> {
  let lower = raw.to_ascii_lowercase();
  // Split off an `of S` clause; only `nth-child` and `nth-last-child` take one.
  let of_index = find_of_clause(&lower);
  let (anb, selector_part) = match of_index {
    Some(i) => (&raw[..i], Some((&raw[i + 2..], inner_abs + i + 2))),
    None => (raw, None),
  };

  validate_anb(anb, inner_abs)?;

  if !SELECTOR_ARGUMENT_PSEUDOS.contains(&lower_name) {
    return Ok(None);
  }
  match selector_part {
    Some((text, offset)) if !text.trim().is_empty() => {
      let mut parser = Parser::new(text, offset);
      Ok(Some(parser.parse()?))
    }
    _ => Ok(None),
  }
}

/// The byte index of the `of` keyword in an An+B argument, if present.
fn find_of_clause(lower: &str) -> Option<usize> {
  let bytes = lower.as_bytes();
  let mut i = 0;
  while i + 1 < bytes.len() {
    if bytes[i] == b'o' && bytes[i + 1] == b'f' {
      let before_ok = i == 0 || bytes[i - 1].is_ascii_whitespace();
      let after_ok = i + 2 >= bytes.len() || !lower.as_bytes()[i + 2].is_ascii_alphanumeric();
      if before_ok && after_ok {
        return Some(i);
      }
    }
    i += 1;
  }
  None
}

/// Validate An+B notation, reporting the offset at which it goes wrong.
fn validate_anb(raw: &str, base: usize) -> Result<(), ParseError> {
  let chars: Vec<(usize, char)> = raw.char_indices().collect();
  let mut i = 0usize;

  let skip_ws = |i: &mut usize| {
    while *i < chars.len() && chars[*i].1.is_whitespace() {
      *i += 1;
    }
  };
  let at_end = |i: usize| i >= chars.len();
  let err = |i: usize, reason: &str| ParseError {
    reason: reason.to_string(),
    offset: base + chars.get(i).map(|(o, _)| *o).unwrap_or_else(|| raw.len()),
  };

  skip_ws(&mut i);
  if at_end(i) {
    return Err(err(i, "selector is expected"));
  }

  // `odd` and `even` are complete on their own.
  let rest: String = chars[i..].iter().map(|(_, c)| *c).collect();
  let lower_rest = rest.trim().to_ascii_lowercase();
  if lower_rest == "odd" || lower_rest == "even" {
    return Ok(());
  }

  // An optional leading sign.
  if matches!(chars.get(i).map(|(_, c)| *c), Some('+') | Some('-')) {
    i += 1;
  }
  // Optional digits for A (or B, when there is no `n`).
  let digits_start = i;
  while !at_end(i) && chars[i].1.is_ascii_digit() {
    i += 1;
  }
  let had_digits = i > digits_start;

  if at_end(i) {
    if !had_digits {
      return Err(err(i, "integer is expected"));
    }
    return Ok(());
  }

  if chars[i].1 == 'n' || chars[i].1 == 'N' {
    i += 1;
    skip_ws(&mut i);
    if at_end(i) {
      return Ok(());
    }
    // A `+` or `-` must be followed by an integer.
    if matches!(chars[i].1, '+' | '-') {
      i += 1;
      skip_ws(&mut i);
      if at_end(i) || !chars[i].1.is_ascii_digit() {
        return Err(err(i, "integer is expected"));
      }
      while !at_end(i) && chars[i].1.is_ascii_digit() {
        i += 1;
      }
      skip_ws(&mut i);
      if at_end(i) {
        return Ok(());
      }
      return Err(err(i, "unexpected input"));
    }
    return Err(err(i, "unexpected input"));
  }

  if !had_digits {
    return Err(err(i, "integer is expected"));
  }
  skip_ws(&mut i);
  if at_end(i) {
    return Ok(());
  }
  Err(err(i, "unexpected input"))
}

/// Parse a selector list, returning a structural error if the source is not a
/// syntactically valid selector list.
pub fn parse_selector_list(source: &str) -> Result<SelectorList, ParseError> {
  let mut parser = Parser::new(source, 0);
  parser.parse()
}

/// Whether a selector uses only standard CSS syntax; interpolation and
/// preprocessor placeholders are left alone.
pub fn is_standard_syntax_selector(selector: &str) -> bool {
  let trimmed = selector.trim_start();
  !(trimmed.starts_with('%')
    || selector.contains("#{")
    || selector.contains("@{")
    || selector.contains('$'))
}

/// A style rule's prelude as written in the source, found by scanning the
/// text rather than the parsed AST.
///
/// Gale's CSS parser drops rules whose selectors it cannot parse and
/// re-serialises the ones it keeps, so rules that must see selectors exactly
/// as the author wrote them (including invalid ones) read them from here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawStyleRule {
  /// The prelude text with surrounding whitespace removed.
  pub prelude: String,
  /// Byte offset of `prelude` in the source.
  pub offset: usize,
  /// Index into the returned list of the enclosing style rule, if nested.
  pub parent: Option<usize>,
  /// Whether an enclosing at-rule is `@keyframes` (or a vendor variant).
  pub in_keyframes: bool,
}

/// Find every style rule prelude in `source`, in document order.
///
/// `scss_comments` enables `//` line comments, which plain CSS lacks.
pub fn scan_style_rules(source: &str, scss_comments: bool) -> Vec<RawStyleRule> {
  let mut out = Vec::new();
  let mut scanner = BlockScanner {
    bytes: source.as_bytes(),
    source,
    scss_comments,
  };
  scanner.scan_block(0, source.len(), None, false, &mut out);
  out
}

/// Walks the byte stream of a stylesheet, tracking braces, strings and
/// comments, to recover rule preludes exactly as written.
struct BlockScanner<'a> {
  bytes: &'a [u8],
  source: &'a str,
  scss_comments: bool,
}

impl BlockScanner<'_> {
  /// Scan the statements in `[start, end)`, a block body or the top level.
  fn scan_block(
    &mut self,
    start: usize,
    end: usize,
    parent: Option<usize>,
    in_keyframes: bool,
    out: &mut Vec<RawStyleRule>,
  ) {
    let mut pos = start;
    loop {
      pos = self.skip_trivia(pos, end);
      if pos >= end || self.bytes[pos] == b'}' {
        return;
      }
      let stmt_start = pos;
      let Some((terminator, at)) = self.find_statement_end(pos, end) else {
        return;
      };
      match terminator {
        b';' => {
          pos = at + 1;
        }
        b'}' => {
          return;
        }
        _ => {
          // An opening brace: a rule, an at-rule, or a nested property block.
          let prelude = self.source[stmt_start..at].trim_end();
          let block_start = at + 1;
          let block_end = self.find_block_end(block_start, end);
          if prelude.starts_with('@') {
            let name = at_rule_name(prelude);
            let nested_keyframes = in_keyframes || name.ends_with("keyframes");
            self.scan_block(block_start, block_end, parent, nested_keyframes, out);
          } else if prelude.is_empty() || prelude.ends_with(':') {
            // A Sass nested-property block; its body holds declarations.
          } else {
            let index = out.len();
            out.push(RawStyleRule {
              prelude: prelude.to_string(),
              offset: stmt_start,
              parent,
              in_keyframes,
            });
            self.scan_block(block_start, block_end, Some(index), in_keyframes, out);
          }
          pos = (block_end + 1).min(end);
        }
      }
    }
  }

  /// Skip whitespace and comments starting at `pos`.
  fn skip_trivia(&self, mut pos: usize, end: usize) -> usize {
    while pos < end {
      let b = self.bytes[pos];
      if b.is_ascii_whitespace() {
        pos += 1;
      } else if b == b'/' && pos + 1 < end && self.bytes[pos + 1] == b'*' {
        pos = self.skip_block_comment(pos, end);
      } else if self.scss_comments && b == b'/' && pos + 1 < end && self.bytes[pos + 1] == b'/' {
        pos = self.skip_line_comment(pos, end);
      } else {
        break;
      }
    }
    pos
  }

  /// Position just past a `/* ... */` comment beginning at `pos`.
  fn skip_block_comment(&self, pos: usize, end: usize) -> usize {
    let mut i = pos + 2;
    while i + 1 < end {
      if self.bytes[i] == b'*' && self.bytes[i + 1] == b'/' {
        return i + 2;
      }
      i += 1;
    }
    end
  }

  /// Position just past a `//` comment beginning at `pos`.
  fn skip_line_comment(&self, pos: usize, end: usize) -> usize {
    let mut i = pos;
    while i < end && self.bytes[i] != b'\n' {
      i += 1;
    }
    i
  }

  /// Position just past a quoted string beginning at `pos`.
  fn skip_string(&self, pos: usize, end: usize) -> usize {
    let quote = self.bytes[pos];
    let mut i = pos + 1;
    while i < end {
      match self.bytes[i] {
        b'\\' => i += 2,
        b if b == quote => return i + 1,
        b'\n' => return i,
        _ => i += 1,
      }
    }
    end
  }

  /// Position just past an interpolation (`#{...}` / `@{...}`) beginning at
  /// `pos`, balancing any nested braces.
  fn skip_interpolation(&self, pos: usize, end: usize) -> usize {
    let mut depth = 0usize;
    let mut i = pos;
    while i < end {
      match self.bytes[i] {
        b'{' => depth += 1,
        b'}' => {
          depth -= 1;
          if depth == 0 {
            return i + 1;
          }
        }
        b'"' | b'\'' => {
          i = self.skip_string(i, end);
          continue;
        }
        _ => {}
      }
      i += 1;
    }
    end
  }

  /// Find the byte that terminates the statement starting at `pos`: one of
  /// `;`, `{` or `}` outside strings, comments, parentheses and
  /// interpolation. Returns the terminator and its position.
  fn find_statement_end(&self, mut pos: usize, end: usize) -> Option<(u8, usize)> {
    let mut paren_depth = 0usize;
    while pos < end {
      let b = self.bytes[pos];
      let next = if pos + 1 < end {
        self.bytes[pos + 1]
      } else {
        0
      };
      match b {
        b'"' | b'\'' => {
          pos = self.skip_string(pos, end);
          continue;
        }
        b'/' if next == b'*' => {
          pos = self.skip_block_comment(pos, end);
          continue;
        }
        b'/' if self.scss_comments && next == b'/' && paren_depth == 0 => {
          pos = self.skip_line_comment(pos, end);
          continue;
        }
        b'#' | b'@' if next == b'{' => {
          pos = self.skip_interpolation(pos + 1, end);
          continue;
        }
        b'(' => paren_depth += 1,
        b')' => paren_depth = paren_depth.saturating_sub(1),
        b';' | b'{' | b'}' => return Some((b, pos)),
        _ => {}
      }
      pos += 1;
    }
    None
  }

  /// Find the `}` closing the block whose body starts at `pos`.
  fn find_block_end(&self, mut pos: usize, end: usize) -> usize {
    let mut depth = 0usize;
    while pos < end {
      let b = self.bytes[pos];
      let next = if pos + 1 < end {
        self.bytes[pos + 1]
      } else {
        0
      };
      match b {
        b'"' | b'\'' => {
          pos = self.skip_string(pos, end);
          continue;
        }
        b'/' if next == b'*' => {
          pos = self.skip_block_comment(pos, end);
          continue;
        }
        b'/' if self.scss_comments && next == b'/' => {
          pos = self.skip_line_comment(pos, end);
          continue;
        }
        b'{' => depth += 1,
        b'}' => {
          if depth == 0 {
            return pos;
          }
          depth -= 1;
        }
        _ => {}
      }
      pos += 1;
    }
    end
  }
}

/// The lower-cased name of an at-rule prelude such as `@media (x)`.
fn at_rule_name(prelude: &str) -> String {
  prelude[1..]
    .chars()
    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
    .collect::<String>()
    .to_ascii_lowercase()
}

#[cfg(test)]
mod scan_tests {
  use super::*;

  fn preludes(src: &str) -> Vec<(String, usize, Option<usize>, bool)> {
    scan_style_rules(src, false)
      .into_iter()
      .map(|r| (r.prelude, r.offset, r.parent, r.in_keyframes))
      .collect()
  }

  #[test]
  fn finds_a_top_level_rule() {
    assert_eq!(preludes("a {}"), vec![("a".into(), 0, None, false)]);
  }

  #[test]
  fn keeps_invalid_selectors_verbatim() {
    assert_eq!(preludes("a ) b {}"), vec![("a ) b".into(), 0, None, false)]);
    assert_eq!(
      preludes(":not(::before) {}"),
      vec![(":not(::before)".into(), 0, None, false)]
    );
  }

  #[test]
  fn finds_nested_rules_with_their_parent() {
    assert_eq!(
      preludes("a { color: red; &:hover {} }"),
      vec![
        ("a".into(), 0, None, false),
        ("&:hover".into(), 16, Some(0), false)
      ]
    );
  }

  #[test]
  fn skips_declarations_but_not_rules_after_them() {
    assert_eq!(
      preludes("a { color: red }"),
      vec![("a".into(), 0, None, false)]
    );
    assert_eq!(
      preludes("a { color: red; .b {} }"),
      vec![
        ("a".into(), 0, None, false),
        (".b".into(), 16, Some(0), false)
      ]
    );
  }

  #[test]
  fn marks_rules_inside_keyframes() {
    assert_eq!(
      preludes("@keyframes foo { from {} 50% {} to {} }"),
      vec![
        ("from".into(), 17, None, true),
        ("50%".into(), 25, None, true),
        ("to".into(), 32, None, true),
      ]
    );
  }

  #[test]
  fn descends_into_other_at_rules_without_a_parent_selector() {
    assert_eq!(
      preludes("@media (x) { a {} }"),
      vec![("a".into(), 13, None, false)]
    );
  }

  #[test]
  fn ignores_braces_inside_strings() {
    assert_eq!(
      preludes("a[title=\"{\"] {}"),
      vec![("a[title=\"{\"]".into(), 0, None, false)]
    );
  }

  #[test]
  fn ignores_comments_before_a_prelude() {
    assert_eq!(preludes("/* x */ a {}"), vec![("a".into(), 8, None, false)]);
  }

  #[test]
  fn keeps_comments_inside_a_prelude() {
    assert_eq!(
      preludes("label/* foo */:enabled {}"),
      vec![("label/* foo */:enabled".into(), 0, None, false)]
    );
  }

  #[test]
  fn treats_double_slash_as_a_comment_only_for_scss() {
    let scss: Vec<_> = scan_style_rules("// x {\na {}", true)
      .into_iter()
      .map(|r| (r.prelude, r.offset))
      .collect();
    assert_eq!(scss, vec![("a".to_string(), 7)]);
  }

  #[test]
  fn balances_interpolation_braces() {
    assert_eq!(
      preludes("#{$foo} {}"),
      vec![("#{$foo}".into(), 0, None, false)]
    );
    assert_eq!(
      preludes("a { #{&}::before {} }"),
      vec![
        ("a".into(), 0, None, false),
        ("#{&}::before".into(), 4, Some(0), false)
      ]
    );
  }

  #[test]
  fn keeps_multi_line_preludes() {
    assert_eq!(preludes("a,\nb {}"), vec![("a,\nb".into(), 0, None, false)]);
  }

  #[test]
  fn skips_nested_property_declarations() {
    assert_eq!(
      preludes("a { font: { family: x } }"),
      vec![("a".into(), 0, None, false)]
    );
  }

  #[test]
  fn nests_through_at_rules_inside_rules() {
    assert_eq!(
      preludes("a { @media (x) { .b {} } }"),
      vec![
        ("a".into(), 0, None, false),
        (".b".into(), 17, Some(0), false)
      ]
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(src: &str) -> SelectorList {
    parse_selector_list(src).expect("should parse")
  }

  fn err(src: &str) -> ParseError {
    parse_selector_list(src).expect_err("should not parse")
  }

  #[test]
  fn parses_a_type_selector() {
    let list = parse("a");
    assert_eq!(list.selectors.len(), 1);
    assert_eq!(
      list.selectors[0].nodes,
      vec![SelectorNode::Tag {
        name: "a".into(),
        offset: 0
      }]
    );
  }

  #[test]
  fn parses_a_selector_list_with_offsets() {
    let list = parse("a, .b");
    assert_eq!(list.selectors.len(), 2);
    assert_eq!(list.selectors[0].offset, 0);
    assert_eq!(list.selectors[0].end, 1);
    assert_eq!(list.selectors[1].offset, 3);
    assert_eq!(list.selectors[1].end, 5);
  }

  #[test]
  fn parses_combinators() {
    let list = parse("a > b");
    let kinds: Vec<_> = list.selectors[0]
      .nodes
      .iter()
      .filter_map(|n| match n {
        SelectorNode::Combinator { kind, .. } => Some(*kind),
        _ => None,
      })
      .collect();
    assert_eq!(kinds, vec![Combinator::Child]);
  }

  #[test]
  fn parses_descendant_combinator() {
    let list = parse("a b");
    let kinds: Vec<_> = list.selectors[0]
      .nodes
      .iter()
      .filter_map(|n| match n {
        SelectorNode::Combinator { kind, .. } => Some(*kind),
        _ => None,
      })
      .collect();
    assert_eq!(kinds, vec![Combinator::Descendant]);
  }

  #[test]
  fn parses_a_pseudo_class_without_arguments() {
    let list = parse(":hover");
    assert!(matches!(
      &list.selectors[0].nodes[0],
      SelectorNode::PseudoClass { name, args: None, .. } if name == "hover"
    ));
  }

  #[test]
  fn parses_a_pseudo_element_with_two_colons() {
    let list = parse("::before");
    assert!(matches!(
      &list.selectors[0].nodes[0],
      SelectorNode::PseudoElement { name, .. } if name == "before"
    ));
  }

  #[test]
  fn parses_nested_selector_arguments() {
    let list = parse(":is(a, b)");
    let SelectorNode::PseudoClass {
      args: Some(inner), ..
    } = &list.selectors[0].nodes[0]
    else {
      panic!("expected a pseudo-class with arguments");
    };
    assert_eq!(inner.selectors.len(), 2);
  }

  #[test]
  fn records_absolute_offsets_inside_arguments() {
    let list = parse(":is(::before)");
    let SelectorNode::PseudoClass {
      args: Some(inner), ..
    } = &list.selectors[0].nodes[0]
    else {
      panic!("expected a pseudo-class with arguments");
    };
    assert_eq!(inner.selectors[0].nodes[0].offset(), 4);
  }

  #[test]
  fn parses_the_nesting_selector() {
    let list = parse("&:hover");
    assert!(matches!(
      list.selectors[0].nodes[0],
      SelectorNode::Nesting { offset: 0 }
    ));
  }

  #[test]
  fn keeps_comments_as_nodes() {
    let list = parse("label/* foo */:enabled");
    assert!(
      list.selectors[0]
        .nodes
        .iter()
        .any(|n| matches!(n, SelectorNode::Comment { .. }))
    );
  }

  #[test]
  fn parses_an_attribute_selector() {
    let list = parse("[foo=\"bar\"]");
    assert!(matches!(
      &list.selectors[0].nodes[0],
      SelectorNode::Attribute { raw, .. } if raw == "[foo=\"bar\"]"
    ));
  }

  #[test]
  fn parses_a_namespaced_type_selector() {
    let list = parse("svg|path");
    assert!(matches!(
      &list.selectors[0].nodes[0],
      SelectorNode::Tag { name, .. } if name == "svg|path"
    ));
  }

  #[test]
  fn parses_an_anb_argument_as_raw_text() {
    let list = parse(":nth-child(2n of .foo)");
    let SelectorNode::PseudoClass {
      raw_args: Some(raw),
      ..
    } = &list.selectors[0].nodes[0]
    else {
      panic!("expected raw arguments");
    };
    assert_eq!(raw, "2n of .foo");
  }

  // --- error cases, phrased as css-tree phrases them ---

  #[test]
  fn rejects_a_stray_closing_parenthesis() {
    let e = err("a ) b");
    assert_eq!(e.reason, "unexpected input");
    assert_eq!(e.offset, 2);
  }

  #[test]
  fn rejects_a_leading_comma() {
    let e = err(", a");
    assert_eq!(e.reason, "selector is expected");
    assert_eq!(e.offset, 0);
  }

  #[test]
  fn rejects_a_trailing_operator_in_anb() {
    let e = err(":nth-child(2n+)");
    assert_eq!(e.reason, "integer is expected");
    assert_eq!(e.offset, 14);
  }

  #[test]
  fn rejects_an_attribute_name_starting_with_a_digit() {
    let e = err("[0foo]");
    assert_eq!(e.reason, "identifier is expected");
    assert_eq!(e.offset, 1);
  }

  #[test]
  fn rejects_a_doubled_attribute_operator() {
    let e = err("[foo==bar]");
    assert_eq!(e.reason, "identifier is expected");
    assert_eq!(e.offset, 5);
  }

  #[test]
  fn rejects_a_doubled_class_dot() {
    let e = err(".foo..bar");
    assert_eq!(e.reason, "identifier is expected");
    assert_eq!(e.offset, 5);
  }
}
