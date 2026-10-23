//! A recursive-descent parser from selector text to the selector AST.
//!
//! The parser is deliberately permissive about things CSS allows but Gale does
//! not care about, and deliberately strict about the error classes that
//! `selector-no-invalid` reports, so that its diagnostics can carry the same
//! reasons and offsets Stylelint produces.

use super::{Combinator, Pseudo, PseudoArg, Selector, SelectorList, SelectorNode};

/// Why a selector failed to parse, phrased the way `css-tree` phrases it so
/// that `selector-no-invalid` can report the same reason Stylelint does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
  /// The lower-cased reason, e.g. `"identifier is expected"`.
  pub reason: String,
  /// Byte offset of the offending character within the selector source.
  pub offset: usize,
}

/// Parse a selector list, returning a structural error if the source is not a
/// syntactically valid selector list.
pub fn parse_selector_list(source: &str) -> Result<SelectorList, ParseError> {
  Parser::new(source, 0).parse()
}

/// Pseudo-classes and pseudo-elements whose argument is a selector list.
///
/// For the `nth-*` pseudo-classes this refers to the `of S` clause; the An+B
/// part is parsed separately.
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

/// Parse a pseudo-selector's argument according to what `lower_name` takes.
/// `offset` is where `raw` starts in the original source.
fn parse_pseudo_argument(
  lower_name: &str,
  raw: String,
  offset: usize,
) -> Result<PseudoArg, ParseError> {
  let takes_selectors = SELECTOR_ARGUMENT_PSEUDOS.contains(&lower_name);
  if ANB_PSEUDOS.contains(&lower_name) {
    let of = Parser::new(&raw, offset).parse_anb_argument(takes_selectors)?;
    return Ok(PseudoArg::Anb { raw, of });
  }
  if !takes_selectors {
    return Ok(PseudoArg::Raw(raw));
  }
  if raw.trim().is_empty() {
    return Ok(PseudoArg::Selectors(SelectorList::default()));
  }
  Ok(PseudoArg::Selectors(Parser::new(&raw, offset).parse()?))
}

/// A cursor over the characters of one selector list. `base` is added to
/// every recorded offset so that arguments parsed out of a nested `:is(...)`
/// still carry offsets into the original source.
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
    self.peek_at(0)
  }

  /// The character `n` positions past the cursor.
  fn peek_at(&self, n: usize) -> Option<char> {
    self.chars.get(self.pos + n).map(|(_, c)| *c)
  }

  /// The absolute byte offset of the cursor.
  fn offset(&self) -> usize {
    self.base + self.local_offset()
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
    if !self.peek().is_some_and(is_ident_start) {
      return Err(self.error("identifier is expected"));
    }
    let start = self.local_offset();
    while self.peek().is_some_and(is_ident_char) {
      self.bump();
    }
    Ok(self.source[start..self.local_offset()].to_string())
  }

  /// Skip whitespace, returning whether any was consumed.
  fn skip_whitespace(&mut self) -> bool {
    let start = self.pos;
    while self.peek().is_some_and(char::is_whitespace) {
      self.bump();
    }
    self.pos > start
  }

  /// Consume a run of ASCII digits, returning whether there were any.
  fn take_digits(&mut self) -> bool {
    let start = self.pos;
    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
      self.bump();
    }
    self.pos > start
  }

  /// Consume `keyword` if it sits at the cursor as a whole word, ignoring
  /// case.
  fn take_keyword(&mut self, keyword: &str) -> bool {
    let matches = keyword
      .chars()
      .enumerate()
      .all(|(i, k)| self.peek_at(i).is_some_and(|c| c.eq_ignore_ascii_case(&k)))
      && !self.peek_at(keyword.len()).is_some_and(is_ident_char);
    if matches {
      self.pos += keyword.len();
    }
    matches
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

  /// Whether the cursor sits on a namespace `|` rather than a `|=` operator.
  fn at_namespace_separator(&self) -> bool {
    self.peek() == Some('|') && self.peek_at(1) != Some('=')
  }

  /// Consume an explicit combinator at the cursor, if there is one.
  fn take_combinator(&mut self) -> Option<Combinator> {
    let kind = match self.peek()? {
      '>' => Combinator::Child,
      '+' => Combinator::NextSibling,
      '~' => Combinator::SubsequentSibling,
      '|' if self.peek_at(1) == Some('|') => {
        self.bump();
        Combinator::Column
      }
      _ => return None,
    };
    self.bump();
    Some(kind)
  }

  /// Consume `*`, `*|*`, or a namespaced type selector such as `*|a`.
  fn take_universal(&mut self) -> Result<SelectorNode, ParseError> {
    self.bump();
    if !self.at_namespace_separator() {
      return Ok(SelectorNode::Universal);
    }
    self.bump();
    if self.peek() == Some('*') {
      self.bump();
      return Ok(SelectorNode::Universal);
    }
    Ok(SelectorNode::Tag(format!("*|{}", self.take_ident()?)))
  }

  /// Consume a type selector, with its namespace when it has one, e.g.
  /// `svg|path` or `svg|*`.
  fn take_type(&mut self) -> Result<SelectorNode, ParseError> {
    let mut name = self.take_ident()?;
    if self.at_namespace_separator() {
      self.bump();
      name.push('|');
      if self.peek() == Some('*') {
        self.bump();
        name.push('*');
      } else {
        name.push_str(&self.take_ident()?);
      }
    }
    Ok(SelectorNode::Tag(name))
  }

  /// Consume an attribute selector, validating its shape.
  fn take_attribute(&mut self) -> Result<SelectorNode, ParseError> {
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
    if self.at_namespace_separator() {
      self.bump();
      self.take_ident()?;
    }
    self.skip_whitespace();
    if let Some(c) = self.peek() {
      if c == ']' {
        self.bump();
        return Ok(SelectorNode::Attribute(
          self.source[start..self.local_offset()].to_string(),
        ));
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
          while self.peek().is_some_and(is_ident_char) {
            self.bump();
          }
        }
        _ => return Err(self.error("identifier is expected")),
      }
      self.skip_whitespace();
      // An optional case-sensitivity flag, e.g. `[foo="bar" i]`.
      if self.peek().is_some_and(is_ident_start) {
        self.take_ident()?;
        self.skip_whitespace();
      }
    }
    if self.peek() != Some(']') {
      return Err(self.error("\"]\" is expected"));
    }
    self.bump();
    Ok(SelectorNode::Attribute(
      self.source[start..self.local_offset()].to_string(),
    ))
  }

  /// Consume a `/* ... */` comment.
  fn take_comment(&mut self) -> SelectorNode {
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
    SelectorNode::Comment(self.source[start..self.local_offset()].to_string())
  }

  /// Consume a pseudo-class or pseudo-element.
  fn take_pseudo(&mut self) -> Result<SelectorNode, ParseError> {
    self.bump(); // first ':'
    let element = self.peek() == Some(':');
    if element {
      self.bump();
    }
    let name = self.take_ident()?;
    let arg = if self.peek() == Some('(') {
      let (raw, offset) = self.take_parenthesised()?;
      parse_pseudo_argument(&name.to_ascii_lowercase(), raw, offset)?
    } else {
      PseudoArg::None
    };
    Ok(SelectorNode::Pseudo(Pseudo { element, name, arg }))
  }

  /// Consume An+B notation, leaving any trailing whitespace in place.
  fn take_anb(&mut self) -> Result<(), ParseError> {
    self.skip_whitespace();
    if self.peek().is_none() {
      return Err(self.error("selector is expected"));
    }
    // `odd` and `even` are complete on their own.
    if self.take_keyword("odd") || self.take_keyword("even") {
      return Ok(());
    }
    if matches!(self.peek(), Some('+' | '-')) {
      self.bump();
    }
    let had_digits = self.take_digits();
    if !matches!(self.peek(), Some('n' | 'N')) {
      return if had_digits {
        Ok(())
      } else {
        Err(self.error("integer is expected"))
      };
    }
    self.bump();
    // A `+` or `-` after `n` must be followed by an integer.
    let after_n = self.pos;
    self.skip_whitespace();
    if !matches!(self.peek(), Some('+' | '-')) {
      self.pos = after_n;
      return Ok(());
    }
    self.bump();
    self.skip_whitespace();
    if !self.take_digits() {
      return Err(self.error("integer is expected"));
    }
    Ok(())
  }

  /// Parse an `nth-*` argument: An+B notation, then an optional `of S`
  /// clause, which is returned parsed when `allows_of` and skipped otherwise.
  fn parse_anb_argument(&mut self, allows_of: bool) -> Result<Option<SelectorList>, ParseError> {
    self.take_anb()?;
    let spaced = self.skip_whitespace();
    if self.peek().is_none() {
      return Ok(None);
    }
    if !spaced || !self.take_keyword("of") {
      return Err(self.error("unexpected input"));
    }
    self.skip_whitespace();
    if !allows_of || self.peek().is_none() {
      return Ok(None);
    }
    Ok(Some(self.parse()?))
  }

  /// Parse the whole selector list.
  fn parse(&mut self) -> Result<SelectorList, ParseError> {
    let mut selectors = Vec::new();
    loop {
      selectors.push(self.parse_selector()?);
      self.skip_whitespace();
      match self.peek() {
        Some(',') => self.bump(),
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
    // The extent of the selector's non-comment nodes, ignoring trailing
    // combinators.
    let mut first_start: Option<usize> = None;
    let mut last_end: Option<usize> = None;
    self.skip_whitespace();
    let start_abs = self.offset();

    loop {
      pending_space |= self.skip_whitespace();
      let Some(c) = self.peek() else { break };
      if c == ',' {
        break;
      }
      if c == ')' {
        return Err(self.error("unexpected input"));
      }
      let node_start = self.offset();

      if let Some(kind) = self.take_combinator() {
        nodes.push(SelectorNode::Combinator(kind));
        first_start.get_or_insert(node_start);
        pending_space = false;
        continue;
      }

      // A descendant combinator, only between two compound selectors. A
      // comment sitting inside the whitespace belongs to the one combinator
      // around it rather than splitting it in two.
      let after_comment_in_combinator = matches!(nodes.last(), Some(SelectorNode::Comment(_)))
        && matches!(
          nodes.len().checked_sub(2).map(|i| &nodes[i]),
          Some(SelectorNode::Combinator(_))
        );
      if pending_space
        && !nodes.is_empty()
        && !matches!(nodes.last(), Some(SelectorNode::Combinator(_)))
        && !after_comment_in_combinator
      {
        nodes.push(SelectorNode::Combinator(Combinator::Descendant));
      }
      pending_space = false;

      let node = match c {
        '/' if self.peek_at(1) == Some('*') => self.take_comment(),
        '.' => {
          self.bump();
          SelectorNode::Class(self.take_ident()?)
        }
        '#' => {
          self.bump();
          SelectorNode::Id(self.take_ident()?)
        }
        '[' => self.take_attribute()?,
        ':' => self.take_pseudo()?,
        '&' => {
          self.bump();
          SelectorNode::Nesting
        }
        '*' => self.take_universal()?,
        c if is_ident_start(c) => self.take_type()?,
        _ => return Err(self.error("unexpected input")),
      };
      if !matches!(node, SelectorNode::Comment(_)) {
        first_start.get_or_insert(node_start);
        last_end = Some(self.offset());
      }
      nodes.push(node);
    }

    if nodes.is_empty() {
      return Err(self.error("selector is expected"));
    }
    let offset = first_start.unwrap_or(start_abs);
    Ok(Selector {
      nodes,
      offset,
      end: last_end.unwrap_or(offset),
    })
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

  fn combinators(src: &str) -> Vec<Combinator> {
    parse(src).selectors[0]
      .nodes
      .iter()
      .filter_map(|n| match n {
        SelectorNode::Combinator(kind) => Some(*kind),
        _ => None,
      })
      .collect()
  }

  /// The first node of `src`, which must be a pseudo.
  fn pseudo(src: &str) -> Pseudo {
    let mut list = parse(src);
    match list.selectors.remove(0).nodes.remove(0) {
      SelectorNode::Pseudo(pseudo) => pseudo,
      other => panic!("expected a pseudo, got {other:?}"),
    }
  }

  #[test]
  fn parses_a_type_selector() {
    let list = parse("a");
    assert_eq!(list.selectors.len(), 1);
    assert_eq!(list.selectors[0].nodes, vec![SelectorNode::Tag("a".into())]);
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
  fn extent_skips_comments_and_trailing_combinators() {
    let list = parse("a, /* c */ b /* d */");
    assert_eq!((list.selectors[1].offset, list.selectors[1].end), (11, 12));
    let list = parse("a >");
    assert_eq!((list.selectors[0].offset, list.selectors[0].end), (0, 1));
    let list = parse("> a");
    assert_eq!((list.selectors[0].offset, list.selectors[0].end), (0, 3));
  }

  #[test]
  fn parses_combinators() {
    assert_eq!(combinators("a > b"), vec![Combinator::Child]);
    assert_eq!(combinators("a b"), vec![Combinator::Descendant]);
    assert_eq!(combinators("a || b"), vec![Combinator::Column]);
  }

  #[test]
  fn parses_a_pseudo_class_without_arguments() {
    let hover = pseudo(":hover");
    assert!(!hover.element);
    assert_eq!(hover.name, "hover");
    assert_eq!(hover.arg, PseudoArg::None);
  }

  #[test]
  fn parses_a_pseudo_element_with_two_colons() {
    let before = pseudo("::before");
    assert!(before.element);
    assert_eq!(before.name, "before");
  }

  #[test]
  fn parses_nested_selector_arguments() {
    let is = pseudo(":is(a, b)");
    assert_eq!(is.selectors().expect("a selector list").selectors.len(), 2);
  }

  #[test]
  fn records_absolute_offsets_inside_arguments() {
    let is = pseudo(":is(::before)");
    assert_eq!(
      is.selectors().expect("a selector list").selectors[0].offset,
      4
    );
  }

  #[test]
  fn parses_the_nesting_selector() {
    assert_eq!(
      parse("&:hover").selectors[0].nodes[0],
      SelectorNode::Nesting
    );
  }

  #[test]
  fn keeps_comments_as_nodes() {
    let list = parse("label/* foo */:enabled");
    assert!(
      list.selectors[0]
        .nodes
        .iter()
        .any(|n| matches!(n, SelectorNode::Comment(_)))
    );
  }

  #[test]
  fn parses_an_attribute_selector() {
    assert_eq!(
      parse("[foo=\"bar\"]").selectors[0].nodes[0],
      SelectorNode::Attribute("[foo=\"bar\"]".into())
    );
  }

  #[test]
  fn parses_a_namespaced_type_selector() {
    assert_eq!(
      parse("svg|path").selectors[0].nodes[0],
      SelectorNode::Tag("svg|path".into())
    );
  }

  #[test]
  fn parses_an_anb_argument_with_its_of_clause() {
    let PseudoArg::Anb { raw, of: Some(of) } = pseudo(":nth-child(2n of .foo)").arg else {
      panic!("expected An+B with an `of` clause");
    };
    assert_eq!(raw, "2n of .foo");
    assert_eq!(of.selectors.len(), 1);
  }

  #[test]
  fn accepts_every_anb_form() {
    for anb in ["odd", "EVEN", "n", "-n+3", "2n + 1", "+5", "2N-1", " 3 "] {
      let arg = pseudo(&format!(":nth-child({anb})")).arg;
      assert!(matches!(arg, PseudoArg::Anb { of: None, .. }), "{anb:?}");
    }
  }

  #[test]
  fn skips_an_of_clause_where_it_is_not_allowed() {
    let arg = pseudo(":nth-of-type(2n of a)").arg;
    assert!(matches!(arg, PseudoArg::Anb { of: None, .. }));
  }

  #[test]
  fn keeps_other_arguments_as_written() {
    assert_eq!(pseudo(":lang(en)").arg, PseudoArg::Raw("en".into()));
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
  fn rejects_text_after_anb() {
    let e = err(":nth-child(2n foo)");
    assert_eq!(e.reason, "unexpected input");
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
