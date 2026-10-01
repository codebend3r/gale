//! A port of [postcss-selector-parser], the selector parser Stylelint's
//! selector rules walk, keeping every node's position in the source.
//!
//! [`super::parser`] builds Gale's own selector AST, which carries no
//! positions.  The autofixing selector rules need to know exactly where a
//! tag, pseudo-class or attribute value sits in the text the author wrote,
//! and need to agree with Stylelint on what each piece is, so they parse
//! with this instead.  It follows postcss-selector-parser's tokenizer and
//! parser, including its quirks (a pseudo-class argument is always parsed
//! as a selector list, comments between compounds fold into the
//! combinator), and fails wherever that parser throws.
//!
//! Offsets are byte offsets into the source the selector came from: the
//! `base` passed to [`parse`] plus the position within the selector text.
//!
//! [postcss-selector-parser]: https://github.com/postcss/postcss-selector-parser

/// What kind of selector node this is, as postcss-selector-parser names
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  /// A type selector, or anything else written as a bare word.
  Tag,
  /// `.name`
  Class,
  /// `#name`
  Id,
  /// `*`
  Universal,
  /// `&`
  Nesting,
  /// `:name` or `::name`, with any parenthesised argument.
  Pseudo,
  /// `[...]`
  Attribute,
  /// `>`, `+`, `~`, `||`, `/name/`, or whitespace (`" "`).
  Combinator,
  /// `/* ... */`
  Comment,
  /// A quoted string, a parenthesised group that follows no pseudo-class,
  /// or whitespace with nothing to attach to.
  String,
}

/// One node of a selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
  pub kind: Kind,
  /// The node's text as written: a tag or class name (without `.`/`#`), a
  /// pseudo's colons and name, a combinator, a comment with its
  /// delimiters, `&` or `*`.  Attributes keep their parts in [`Self::attr`]
  /// and have the whole `[...]` here.
  pub value: String,
  /// Byte offset where the node starts (`sourceIndex`).
  pub start: usize,
  /// Exclusive byte offset where the node ends.  A pseudo's end includes
  /// its argument.
  pub end: usize,
  /// The namespace prefix of a tag, universal or attribute (`svg` in
  /// `svg|a`; empty for `|a`).
  pub namespace: Option<String>,
  /// The argument of a pseudo-class or pseudo-element.
  pub args: Option<Args>,
  /// The parts of an attribute selector.
  pub attr: Option<Attr>,
}

/// The parenthesised argument of a pseudo, parsed as a selector list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
  /// Byte offset of the `(`.
  pub open: usize,
  /// Byte offset of the matching `)`.
  pub close: usize,
  pub selectors: Vec<Selector>,
}

/// The parts of an attribute selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
  /// The attribute name as written.
  pub name: String,
  /// `=`, `~=`, `|=`, `^=`, `$=` or `*=`.
  pub operator: Option<String>,
  /// The value, when there is one.
  pub value: Option<AttrValue>,
  /// The `i`/`s` flag after the value, as written.
  pub flag: Option<String>,
}

/// An attribute selector's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrValue {
  /// The value as written, quotes and escapes included.
  pub raw: String,
  /// The value with quotes removed and escapes resolved.
  pub unescaped: String,
  /// The quote mark it was written with, if any.
  pub quote: Option<char>,
  /// Byte offset where the value starts.
  pub start: usize,
  /// Exclusive byte offset where the value ends.
  pub end: usize,
}

/// One selector of a selector list: the nodes between two commas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
  pub nodes: Vec<Node>,
  /// Byte offset where the selector starts (`sourceIndex`).
  pub start: usize,
}

/// A node met by [`walk`], with where it sits.
pub struct Visit<'a> {
  pub node: &'a Node,
  /// The pseudo whose argument holds the node, if any.
  pub parent_pseudo: Option<&'a Node>,
  /// The nodes of the selector holding it.
  pub siblings: &'a [Node],
  /// Its index in `siblings`.
  pub index: usize,
}

impl Visit<'_> {
  /// The node before this one in its selector (postcss's `node.prev()`).
  pub fn prev(&self) -> Option<&Node> {
    self.index.checked_sub(1).map(|i| &self.siblings[i])
  }

  /// The node after this one in its selector (postcss's `node.next()`).
  pub fn next(&self) -> Option<&Node> {
    self.siblings.get(self.index + 1)
  }
}

/// Visit every node depth first in source order, descending into pseudo
/// arguments, the way postcss-selector-parser's `walk` does.
pub fn walk<'a>(selectors: &'a [Selector], visit: &mut dyn FnMut(&Visit<'a>)) {
  walk_in(selectors, None, visit);
}

/// [`walk`] over `selectors`, which sit inside `parent_pseudo`'s argument.
fn walk_in<'a>(
  selectors: &'a [Selector],
  parent_pseudo: Option<&'a Node>,
  visit: &mut dyn FnMut(&Visit<'a>),
) {
  for selector in selectors {
    for (index, node) in selector.nodes.iter().enumerate() {
      visit(&Visit {
        node,
        parent_pseudo,
        siblings: &selector.nodes,
        index,
      });
      if let Some(args) = &node.args {
        walk_in(&args.selectors, Some(node), visit);
      }
    }
  }
}

/// Parse `text`, which starts at byte `base` of the source, as a selector
/// list.  `None` wherever postcss-selector-parser would throw, which is
/// where Stylelint's rules give up on the selector.
pub fn parse(text: &str, base: usize) -> Option<Vec<Selector>> {
  let tokens = tokenize(text)?;
  let mut parser = Parser {
    text,
    base,
    tokens,
    position: 0,
  };
  let mut selectors = vec![Selector {
    nodes: Vec::new(),
    start: base,
  }];
  while parser.position < parser.tokens.len() {
    parser.parse(&mut selectors, true)?;
  }
  Some(selectors)
}

/// How [`cssesc`] escapes a string, as postcss-selector-parser asks it to
/// for an attribute value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escape {
  /// As a CSS identifier (an unquoted value).
  Identifier,
  /// As a string wrapped in double quotes.
  DoubleQuoted,
}

/// A port of the `cssesc` package that postcss-selector-parser serialises
/// attribute values with: `text` escaped for `mode`.
pub fn cssesc(text: &str, mode: Escape) -> String {
  let identifier = mode == Escape::Identifier;
  let mut out = String::with_capacity(text.len() + 2);
  for c in text.chars() {
    let code = c as u32;
    if !(0x20..=0x7E).contains(&code) || matches!(c, '\t' | '\n' | '\x0c' | '\r' | '\x0b') {
      out.push_str(&format!("\\{:X} ", code));
    } else if c == '\\' || (!identifier && c == '"') || (identifier && is_single_escape(c)) {
      out.push('\\');
      out.push(c);
    } else {
      out.push(c);
    }
  }
  if identifier {
    let bytes = out.as_bytes();
    if bytes.first() == Some(&b'-')
      && bytes
        .get(1)
        .is_some_and(|b| *b == b'-' || b.is_ascii_digit())
    {
      out = format!("\\-{}", &out[1..]);
    } else if let Some(first) = text.chars().next().filter(char::is_ascii_digit) {
      out = format!("\\3{first} {}", &out[first.len_utf8()..]);
    }
  }
  let out = strip_redundant_escape_spaces(&out);
  if identifier {
    out
  } else {
    format!("\"{out}\"")
  }
}

/// cssesc's `regexSingleEscape`: printable ASCII that an identifier must
/// escape with a backslash.
fn is_single_escape(c: char) -> bool {
  matches!(c, ' '..=',' | '.' | '/' | ':'..='@' | '[' | ']' | '^' | '`' | '{'..='~')
}

/// cssesc's last step: drop the space after a `\HEX` escape when what
/// follows is neither a hex digit nor a space, unless an odd run of
/// backslashes before it makes the escape literal text.
fn strip_redundant_escape_spaces(text: &str) -> String {
  let bytes = text.as_bytes();
  let mut out = String::with_capacity(text.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'\\' {
      let run_start = i;
      while i < bytes.len() && bytes[i] == b'\\' {
        i += 1;
      }
      let run = i - run_start;
      let hex_start = i;
      while i < bytes.len()
        && i - hex_start < 6
        && bytes[i].is_ascii_hexdigit()
        && !bytes[i].is_ascii_lowercase()
      {
        i += 1;
      }
      out.push_str(&text[run_start..i]);
      let is_hex_escape = i > hex_start;
      // The last backslash of the run starts the escape; those before it
      // pair up as escaped backslashes when there is an odd number of them.
      let preceding = run - 1;
      if is_hex_escape
        && bytes.get(i) == Some(&b' ')
        && !bytes
          .get(i + 1)
          .is_some_and(|b| b.is_ascii_hexdigit() || *b == b' ')
        && preceding % 2 == 0
      {
        i += 1;
      }
      continue;
    }
    let ch = text[i..].chars().next().expect("in bounds");
    out.push(ch);
    i += ch.len_utf8();
  }
  out
}

/// PostCSS's `rule.selectors`: the selector split at commas outside
/// parentheses, brackets and strings, each trimmed.
pub fn rule_selectors(selector: &str) -> Vec<&str> {
  let bytes = selector.as_bytes();
  let mut parts = Vec::new();
  let mut depth = 0usize;
  let mut quote: Option<u8> = None;
  let mut escaped = false;
  let mut start = 0;
  for (i, &b) in bytes.iter().enumerate() {
    if escaped {
      escaped = false;
      continue;
    }
    match (quote, b) {
      (_, b'\\') => escaped = true,
      (Some(q), _) if b == q => quote = None,
      (Some(_), _) => {}
      (None, b'"' | b'\'') => quote = Some(b),
      (None, b'(' | b'[') => depth += 1,
      (None, b')' | b']') => depth = depth.saturating_sub(1),
      (None, b',') if depth == 0 => {
        parts.push(selector[start..i].trim());
        start = i + 1;
      }
      _ => {}
    }
  }
  parts.push(selector[start..].trim());
  parts
}

/// `text` without its `/* ... */` comments.
pub fn strip_comments(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(open) = rest.find("/*") {
    out.push_str(&rest[..open]);
    rest = rest[open + 2..]
      .find("*/")
      .map_or("", |close| &rest[open + 2 + close + 2..]);
  }
  out.push_str(rest);
  out
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// A token type, as postcss-selector-parser's tokenizer names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
  Space,
  /// A run of `+`, `>`, `~` and `|`.
  Combinator,
  Word,
  Str,
  Comment,
  /// Any other single character: `* & ! , = $ ^ [ ] : ; ( ) /`.
  Char(u8),
}

/// A token and its byte range in the selector text.
#[derive(Debug, Clone, Copy)]
struct Token {
  kind: Tok,
  start: usize,
  end: usize,
}

/// Whether `b` is whitespace to the tokenizer.
fn is_space(b: u8) -> bool {
  matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// Whether `b` ends a word.
fn is_word_delimiter(b: u8) -> bool {
  is_space(b)
    || matches!(
      b,
      b'&'
        | b'*'
        | b'!'
        | b','
        | b':'
        | b';'
        | b'('
        | b')'
        | b'['
        | b']'
        | b'\''
        | b'"'
        | b'+'
        | b'|'
        | b'~'
        | b'>'
        | b'='
        | b'$'
        | b'^'
        | b'/'
    )
}

/// The last byte of the escape sequence whose `\` is at `start`.
fn consume_escape(bytes: &[u8], start: usize) -> usize {
  let mut next = start;
  match bytes.get(next + 1) {
    Some(b'\t' | b'\n' | b'\r' | b'\x0c') => {}
    Some(b) if b.is_ascii_hexdigit() => {
      let mut digits = 0;
      loop {
        next += 1;
        digits += 1;
        match bytes.get(next + 1) {
          Some(b) if b.is_ascii_hexdigit() && digits < 6 => {}
          _ => break,
        }
      }
      if digits < 6 && bytes.get(next + 1) == Some(&b' ') {
        next += 1;
      }
    }
    _ => next += 1,
  }
  next
}

/// The last byte of the word starting at `start`.
fn consume_word(bytes: &[u8], start: usize) -> usize {
  let mut next = start;
  loop {
    match bytes.get(next) {
      Some(&b) if is_word_delimiter(b) => return next.saturating_sub(1).max(start),
      Some(b'\\') => next = consume_escape(bytes, next) + 1,
      Some(_) => next += 1,
      None => return next - 1,
    }
    if next >= bytes.len() {
      return next - 1;
    }
  }
}

/// Split `text` into tokens; `None` for an unclosed string or comment.
fn tokenize(text: &str) -> Option<Vec<Token>> {
  let bytes = text.as_bytes();
  let mut tokens = Vec::new();
  let mut start = 0;
  while start < bytes.len() {
    let b = bytes[start];
    let (kind, end) = if is_space(b) {
      let mut next = start + 1;
      while next < bytes.len() && is_space(bytes[next]) {
        next += 1;
      }
      (Tok::Space, next)
    } else if matches!(b, b'+' | b'>' | b'~' | b'|') {
      let mut next = start + 1;
      while next < bytes.len() && matches!(bytes[next], b'+' | b'>' | b'~' | b'|') {
        next += 1;
      }
      (Tok::Combinator, next)
    } else if matches!(
      b,
      b'*' | b'&' | b'!' | b',' | b'=' | b'$' | b'^' | b'[' | b']' | b':' | b';' | b'(' | b')'
    ) {
      (Tok::Char(b), start + 1)
    } else if b == b'\'' || b == b'"' {
      let mut next = start;
      loop {
        next = next + 1 + bytes[next + 1..].iter().position(|c| *c == b)?;
        let mut backslashes = 0;
        while bytes[next - 1 - backslashes] == b'\\' {
          backslashes += 1;
        }
        if backslashes % 2 == 0 {
          break;
        }
      }
      (Tok::Str, next + 1)
    } else if b == b'/' && bytes.get(start + 1) == Some(&b'*') {
      let close = text.get(start + 2..)?.find("*/")?;
      (Tok::Comment, start + 2 + close + 2)
    } else if b == b'/' {
      (Tok::Char(b), start + 1)
    } else {
      (Tok::Word, consume_word(bytes, start) + 1)
    };
    tokens.push(Token { kind, start, end });
    start = end;
  }
  Some(tokens)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Whether a token is whitespace or a comment.
fn is_trivia(kind: Tok) -> bool {
  matches!(kind, Tok::Space | Tok::Comment)
}

/// The parser state: tokens and a cursor over them.
struct Parser<'a> {
  text: &'a str,
  base: usize,
  tokens: Vec<Token>,
  position: usize,
}

impl Parser<'_> {
  /// The token at `index`, if any.
  fn token(&self, index: usize) -> Option<Token> {
    self.tokens.get(index).copied()
  }

  /// The token at the cursor.
  fn curr(&self) -> Option<Token> {
    self.token(self.position)
  }

  /// The token after the cursor.
  fn next_token(&self) -> Option<Token> {
    self.token(self.position + 1)
  }

  /// The token before the cursor.
  fn prev_token(&self) -> Option<Token> {
    self.position.checked_sub(1).and_then(|i| self.token(i))
  }

  /// A token's text.
  fn content(&self, token: Token) -> &str {
    self.text.get(token.start..token.end).unwrap_or("")
  }

  /// Append `node` to the selector being built.
  fn push(&mut self, selectors: &mut [Selector], node: Node) {
    selectors
      .last_mut()
      .expect("a selector list is never empty")
      .nodes
      .push(node);
  }

  /// A node of `kind` covering `start..end` of the selector text.
  fn node(&self, kind: Kind, start: usize, end: usize) -> Node {
    Node {
      kind,
      value: self.text.get(start..end).unwrap_or("").to_string(),
      start: self.base + start,
      end: self.base + end,
      namespace: None,
      args: None,
      attr: None,
    }
  }

  /// Parse the construct at the cursor into the last selector of
  /// `selectors`.  `top` is postcss-selector-parser's `throwOnParenthesis`.
  fn parse(&mut self, selectors: &mut Vec<Selector>, top: bool) -> Option<()> {
    let token = self.curr()?;
    match token.kind {
      Tok::Space => self.space(selectors),
      Tok::Comment => {
        let node = self.node(Kind::Comment, token.start, token.end);
        self.push(selectors, node);
        self.position += 1;
        Some(())
      }
      Tok::Char(b'(') => self.parentheses(selectors),
      Tok::Char(b')') => {
        if top {
          return None;
        }
        Some(())
      }
      Tok::Char(b'[') => self.attribute(selectors),
      Tok::Char(b'$' | b'^' | b'=') | Tok::Word => self.word(selectors, None),
      Tok::Char(b':') => self.pseudo(selectors),
      Tok::Char(b',') => {
        self.comma(selectors);
        Some(())
      }
      Tok::Char(b'*') => self.universal(selectors, None),
      Tok::Char(b'&') => {
        if self.next_token().is_some_and(|t| self.content(t) == "|") {
          self.position += 1;
          return Some(());
        }
        let node = self.node(Kind::Nesting, token.start, token.end);
        self.push(selectors, node);
        self.position += 1;
        Some(())
      }
      Tok::Char(b'/') | Tok::Combinator => self.combinator(selectors),
      Tok::Str => {
        let node = self.node(Kind::String, token.start, token.end);
        self.push(selectors, node);
        self.position += 1;
        Some(())
      }
      _ => None,
    }
  }

  /// A comma starts the next selector, unless it is the very last token.
  fn comma(&mut self, selectors: &mut Vec<Selector>) {
    if self.position == self.tokens.len() - 1 {
      self.position += 1;
      return;
    }
    let start = self.token(self.position + 1).map_or(0, |t| t.start);
    selectors.push(Selector {
      nodes: Vec::new(),
      start: self.base + start,
    });
    self.position += 1;
  }

  /// Whitespace: before the first node, after the last one, or a
  /// descendant combinator.
  fn space(&mut self, selectors: &mut Vec<Selector>) -> Option<()> {
    let prev = self.prev_token().map(|t| t.kind);
    let current = selectors.last().expect("never empty");
    if self.position == 0
      || prev == Some(Tok::Char(b','))
      || prev == Some(Tok::Char(b'('))
      || current.nodes.iter().all(|n| n.kind == Kind::Comment)
    {
      self.position += 1;
      return Some(());
    }
    let next = self.next_token().map(|t| t.kind);
    if self.position == self.tokens.len() - 1
      || next == Some(Tok::Char(b','))
      || next == Some(Tok::Char(b')'))
    {
      self.position += 1;
      return Some(());
    }
    self.combinator(selectors)
  }

  /// The index of the next token at or after `from` that is not whitespace
  /// or a comment.
  fn next_meaningful(&self, from: usize) -> Option<usize> {
    (from..self.tokens.len()).find(|&i| !is_trivia(self.tokens[i].kind))
  }

  /// A combinator, or whitespace and comments that turn out to end the
  /// selector.
  fn combinator(&mut self, selectors: &mut Vec<Selector>) -> Option<()> {
    let token = self.curr()?;
    if self.content(token) == "|" {
      return self.namespace(selectors);
    }
    let next_sig = self.next_meaningful(self.position);
    let ends_selector = match next_sig {
      None => true,
      Some(i) => matches!(self.tokens[i].kind, Tok::Char(b',' | b')')),
    };
    if ends_selector {
      // Trailing whitespace and comments: comments become nodes only when
      // the selector has nothing else to hang them on.
      let stop = next_sig.unwrap_or(self.tokens.len());
      let has_last = !selectors.last().expect("never empty").nodes.is_empty();
      let mut nodes = Vec::new();
      let mut saw_space = false;
      while self.position < stop {
        let t = self.tokens[self.position];
        match t.kind {
          Tok::Comment => nodes.push(self.node(Kind::Comment, t.start, t.end)),
          _ => saw_space = true,
        }
        self.position += 1;
      }
      if !has_last {
        if nodes.is_empty() && saw_space {
          let t = self.tokens[stop - 1];
          nodes.push(self.node(Kind::String, token.start, t.end));
          nodes.last_mut().expect("just pushed").value.clear();
        }
        for node in nodes {
          self.push(selectors, node);
        }
      }
      return Some(());
    }
    let first = token;
    let next_sig = next_sig.expect("checked above");
    let had_trivia = next_sig > self.position;
    self.position = next_sig;
    let curr = self.curr()?;
    let is_named = curr.kind == Tok::Char(b'/')
      && self.token(self.position + 1).map(|t| t.kind) == Some(Tok::Word)
      && self.token(self.position + 2).map(|t| t.kind) == Some(Tok::Char(b'/'));
    let node = if is_named {
      let end = self.tokens[self.position + 2].end;
      self.position += 3;
      self.node(Kind::Combinator, curr.start, end)
    } else if curr.kind == Tok::Combinator {
      self.position += 1;
      self.node(Kind::Combinator, curr.start, curr.end)
    } else if !had_trivia {
      return None;
    } else {
      // A descendant combinator, spanning the whitespace and comments.
      let last = self.tokens[next_sig - 1];
      let mut node = self.node(Kind::Combinator, first.start, last.end);
      node.value = " ".to_string();
      node
    };
    if self.curr().is_some_and(|t| t.kind == Tok::Space) {
      self.position += 1;
    }
    self.push(selectors, node);
    Some(())
  }

  /// A namespace separator `|`, followed by a word or `*`.
  fn namespace(&mut self, selectors: &mut Vec<Selector>) -> Option<()> {
    let prefix = match self.prev_token() {
      Some(t) if matches!(t.kind, Tok::Word | Tok::Char(b'*' | b'&')) => {
        self.content(t).to_string()
      }
      _ => String::new(),
    };
    let next = self.next_token()?;
    match next.kind {
      Tok::Word => {
        self.position += 1;
        self.word(selectors, Some(prefix))
      }
      Tok::Char(b'*') => {
        self.position += 1;
        self.universal(selectors, Some(prefix))
      }
      _ => None,
    }
  }

  /// `*`, possibly a namespace prefix.
  fn universal(&mut self, selectors: &mut Vec<Selector>, namespace: Option<String>) -> Option<()> {
    if self.next_token().is_some_and(|t| self.content(t) == "|") {
      self.position += 1;
      return self.namespace(selectors);
    }
    let token = self.curr()?;
    let mut node = self.node(Kind::Universal, token.start, token.end);
    node.namespace = namespace;
    self.push(selectors, node);
    self.position += 1;
    Some(())
  }

  /// A word, possibly a namespace prefix.
  fn word(&mut self, selectors: &mut Vec<Selector>, namespace: Option<String>) -> Option<()> {
    if self.next_token().is_some_and(|t| self.content(t) == "|") {
      self.position += 1;
      return self.namespace(selectors);
    }
    self.split_word(selectors, namespace, None).map(drop)
  }

  /// The word at the cursor (merged with any `$`, `^`, `=` and words that
  /// follow it), split into a tag, classes and ids at unescaped `.` and `#`.
  ///
  /// With `pseudo` (the offset of the colons just read), the first piece is
  /// that pseudo's name instead.  Returns how many pieces there were.
  fn split_word(
    &mut self,
    selectors: &mut [Selector],
    mut namespace: Option<String>,
    pseudo: Option<usize>,
  ) -> Option<usize> {
    let first = self.curr()?;
    let mut end = first.end;
    while let Some(next) = self.next_token() {
      if !matches!(next.kind, Tok::Char(b'$' | b'^' | b'=') | Tok::Word) {
        break;
      }
      self.position += 1;
      end = next.end;
      if self.content(next).ends_with('\\')
        && let Some(space) = self.next_token().filter(|t| t.kind == Tok::Space)
      {
        self.position += 1;
        end = space.end;
      }
    }
    let word = self.text.get(first.start..end)?;
    let bytes = word.as_bytes();
    let keyframes_percent = is_keyframes_percent(word);
    let escaped = |i: usize| i > 0 && bytes[i - 1] == b'\\';
    let classes: Vec<usize> = (0..bytes.len())
      .filter(|&i| bytes[i] == b'.' && !escaped(i) && !keyframes_percent)
      .collect();
    let ids: Vec<usize> = (0..bytes.len())
      .filter(|&i| bytes[i] == b'#' && !escaped(i) && bytes.get(i + 1) != Some(&b'{'))
      .collect();
    let mut indices: Vec<usize> = std::iter::once(0)
      .chain(classes.iter().copied())
      .chain(ids.iter().copied())
      .collect();
    indices.sort_unstable();
    indices.dedup();
    for (i, &ind) in indices.iter().enumerate() {
      let seg_end = indices.get(i + 1).copied().unwrap_or(bytes.len());
      let (start, stop) = (first.start + ind, first.start + seg_end);
      let mut node = match pseudo {
        Some(colons) if i == 0 => self.node(Kind::Pseudo, colons, stop),
        _ if classes.contains(&ind) => {
          let mut n = self.node(Kind::Class, start, stop);
          n.value.remove(0);
          n
        }
        _ if ids.contains(&ind) => {
          let mut n = self.node(Kind::Id, start, stop);
          n.value.remove(0);
          n
        }
        _ => self.node(Kind::Tag, start, stop),
      };
      node.namespace = namespace.take();
      self.push(selectors, node);
    }
    self.position += 1;
    Some(indices.len())
  }

  /// `:name` or `::name`.
  fn pseudo(&mut self, selectors: &mut [Selector]) -> Option<()> {
    let start = self.curr()?.start;
    while self.curr().is_some_and(|t| t.kind == Tok::Char(b':')) {
      self.position += 1;
    }
    if self.curr()?.kind != Tok::Word {
      return None;
    }
    let pieces = self.split_word(selectors, None, Some(start))?;
    // `:a.b(` is a misplaced parenthesis.
    if pieces > 1 && self.curr().is_some_and(|t| t.kind == Tok::Char(b'(')) {
      return None;
    }
    Some(())
  }

  /// A parenthesised group: a pseudo's argument, parsed as a selector list,
  /// or raw text appended to whatever precedes it.
  fn parentheses(&mut self, selectors: &mut [Selector]) -> Option<()> {
    let open = self.curr()?;
    self.position += 1;
    let mut unbalanced = 1usize;
    let last_is_pseudo = selectors
      .last()
      .and_then(|s| s.nodes.last())
      .is_some_and(|n| n.kind == Kind::Pseudo);
    if last_is_pseudo {
      let mut inner = vec![Selector {
        nodes: Vec::new(),
        start: self.base + self.curr()?.start,
      }];
      let mut close = None;
      while unbalanced > 0 {
        let Some(token) = self.curr() else { break };
        match token.kind {
          Tok::Char(b'(') => unbalanced += 1,
          Tok::Char(b')') => unbalanced -= 1,
          _ => {}
        }
        if unbalanced > 0 {
          // A nested `(` is consumed whole by the recursive call, which
          // leaves `unbalanced` one too high; the extra `)` visit below
          // brings it back, as in postcss-selector-parser.
          self.parse(&mut inner, false)?;
        } else {
          close = Some(token.start);
          self.position += 1;
        }
      }
      let close = close?;
      let pseudo = selectors
        .last_mut()
        .and_then(|s| s.nodes.last_mut())
        .expect("checked above");
      pseudo.end = self.base + close + 1;
      match &mut pseudo.args {
        Some(args) => {
          args.selectors.extend(inner);
          args.close = self.base + close;
        }
        None => {
          pseudo.args = Some(Args {
            open: self.base + open.start,
            close: self.base + close,
            selectors: inner,
          });
        }
      }
      return Some(());
    }
    let mut end = open.end;
    while unbalanced > 0 {
      let Some(token) = self.curr() else { break };
      match token.kind {
        Tok::Char(b'(') => unbalanced += 1,
        Tok::Char(b')') => unbalanced -= 1,
        _ => {}
      }
      end = token.end;
      self.position += 1;
    }
    if unbalanced > 0 {
      return None;
    }
    let raw = self.text.get(open.start..end)?.to_string();
    match selectors.last_mut().and_then(|s| s.nodes.last_mut()) {
      Some(last) => {
        last.value.push_str(&raw);
        last.end = self.base + end;
      }
      None => {
        let node = self.node(Kind::String, open.start, end);
        self.push(selectors, node);
      }
    }
    Some(())
  }

  /// `[name op value flag]`.
  fn attribute(&mut self, selectors: &mut [Selector]) -> Option<()> {
    let open = self.curr()?;
    self.position += 1;
    let mut attr: Vec<Token> = Vec::new();
    while let Some(t) = self.curr() {
      if t.kind == Tok::Char(b']') {
        break;
      }
      attr.push(t);
      self.position += 1;
    }
    let close = self.curr()?;
    if attr.len() == 1 && attr[0].kind != Tok::Word {
      return None;
    }

    /// Which part of the attribute the last token went to.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Part {
      None,
      Namespace,
      Attribute,
      Operator,
      Value,
      Insensitive,
    }

    let mut name: Option<String> = None;
    let mut namespace: Option<String> = None;
    let mut operator: Option<String> = None;
    let mut value: Option<(String, usize, usize)> = None;
    let mut quote: Option<char> = None;
    let mut flag: Option<String> = None;
    let mut last = Part::None;
    let mut space_after = false;

    for pos in 0..attr.len() {
      let token = attr[pos];
      let content = self.content(token).to_string();
      let next = attr.get(pos + 1).copied();
      let next_is_equals = || next.map(|t| t.kind == Tok::Char(b'='));
      match token.kind {
        Tok::Space => space_after = true,
        Tok::Char(b'*') => {
          if next_is_equals()? {
            operator = Some(content);
            last = Part::Operator;
          } else if namespace.is_none() || (last == Part::Namespace && !space_after) {
            namespace.get_or_insert_with(String::new).push_str(&content);
            last = Part::Namespace;
          }
          space_after = false;
        }
        Tok::Char(b'$') if last == Part::Value => {
          let v = value.as_mut()?;
          v.0.push('$');
          v.2 = token.end;
        }
        Tok::Char(b'$' | b'^') => {
          if next_is_equals()? {
            operator = Some(content);
            last = Part::Operator;
          }
          space_after = false;
        }
        Tok::Combinator => {
          if content == "~" && next_is_equals()? {
            operator = Some(content.clone());
            last = Part::Operator;
          }
          if content == "|" {
            if next_is_equals()? {
              operator = Some(content);
              last = Part::Operator;
            } else if namespace.is_none() && name.is_none() {
              namespace = Some(String::new());
            }
          }
          space_after = false;
        }
        Tok::Word => {
          let next_is_pipe = next.is_some_and(|t| self.content(t) == "|");
          let after_pipe_not_equals = attr.get(pos + 2).is_some_and(|t| t.kind != Tok::Char(b'='));
          if next_is_pipe && after_pipe_not_equals && operator.is_none() && namespace.is_none() {
            namespace = Some(content);
            last = Part::Namespace;
          } else if name.is_none() || (last == Part::Attribute && !space_after) {
            name.get_or_insert_with(String::new).push_str(&content);
            last = Part::Attribute;
          } else if value.is_none() || (last == Part::Value && !(space_after || quote.is_some())) {
            match &mut value {
              Some(v) => {
                v.0.push_str(&content);
                v.2 = token.end;
              }
              None => value = Some((content, token.start, token.end)),
            }
            quote = None;
            last = Part::Value;
          } else if let Some(v) = &mut value {
            if quote.is_some() || space_after {
              flag = Some(content);
              last = Part::Insensitive;
            } else {
              v.0.push_str(&content);
              v.2 = token.end;
              last = Part::Value;
            }
          }
          space_after = false;
        }
        Tok::Str => {
          if name.is_none() || operator.is_none() {
            return None;
          }
          quote = content.chars().next();
          value = Some((content, token.start, token.end));
          last = Part::Value;
          space_after = false;
        }
        Tok::Char(b'=') => {
          if name.is_none() || value.as_ref().is_some_and(|v| !v.0.is_empty()) {
            return None;
          }
          operator.get_or_insert_with(String::new).push('=');
          last = Part::Operator;
          space_after = false;
        }
        Tok::Comment => {
          let to_spaces =
            space_after || next.is_some_and(|t| t.kind == Tok::Space) || last == Part::Insensitive;
          if last == Part::Value
            && !to_spaces
            && let Some(v) = &mut value
          {
            v.0.push_str(&content);
            v.2 = token.end;
          }
        }
        _ => return None,
      }
    }

    let value = value.map(|(raw, start, end)| {
      let unescaped = match quote {
        Some(q) => {
          let inner = raw.strip_prefix(q).unwrap_or(&raw);
          unescape(inner.strip_suffix(q).unwrap_or(inner))
        }
        None => unescape(&raw),
      };
      AttrValue {
        unescaped,
        quote,
        raw,
        start: self.base + start,
        end: self.base + end,
      }
    });
    let mut node = self.node(Kind::Attribute, open.start, close.end);
    node.namespace = namespace;
    node.attr = Some(Attr {
      name: name.unwrap_or_default(),
      operator,
      value,
      flag,
    });
    self.push(selectors, node);
    self.position += 1;
    Some(())
  }
}

/// Resolve CSS escapes the way postcss-selector-parser's `unesc` does:
/// `\` and up to six hex digits (and one following space) for a code point,
/// `\` and any other character for that character.
pub fn unescape(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut chars = text.chars().peekable();
  while let Some(c) = chars.next() {
    if c != '\\' {
      out.push(c);
      continue;
    }
    let mut hex = String::new();
    while hex.len() < 6 {
      match chars.peek() {
        Some(h) if h.is_ascii_hexdigit() => {
          hex.push(*h);
          chars.next();
        }
        _ => break,
      }
    }
    if hex.is_empty() {
      if let Some(next) = chars.next() {
        out.push(next);
      }
      continue;
    }
    if chars.peek() == Some(&' ') {
      chars.next();
    }
    let code = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
    let code = if code == 0 || (0xD800..=0xDFFF).contains(&code) || code > 0x10FFFF {
      0xFFFD
    } else {
      code
    };
    out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
  }
  out
}

/// Whether `word` is a keyframe percentage such as `12.5%`, whose `.` is not
/// a class.
fn is_keyframes_percent(word: &str) -> bool {
  let Some(number) = word.strip_suffix('%') else {
    return false;
  };
  let Some((int, frac)) = number.split_once('.') else {
    return false;
  };
  !int.is_empty()
    && !frac.is_empty()
    && int.bytes().all(|b| b.is_ascii_digit())
    && frac.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `(kind, value, start, end)` of every node of every selector, flattened
  /// in walk order.
  fn flat(text: &str) -> Vec<(Kind, String, usize, usize)> {
    let selectors = parse(text, 0).expect("parses");
    let mut out = Vec::new();
    walk(&selectors, &mut |v| {
      out.push((v.node.kind, v.node.value.clone(), v.node.start, v.node.end));
    });
    out
  }

  #[test]
  fn splits_compounds_and_combinators_with_positions() {
    assert_eq!(
      flat("A.b#c > d"),
      vec![
        (Kind::Tag, "A".into(), 0, 1),
        (Kind::Class, "b".into(), 1, 3),
        (Kind::Id, "c".into(), 3, 5),
        (Kind::Combinator, ">".into(), 6, 7),
        (Kind::Tag, "d".into(), 8, 9),
      ]
    );
    let descendant = flat("a /* c */ b");
    assert_eq!(descendant[1], (Kind::Combinator, " ".into(), 1, 10));
  }

  #[test]
  fn parses_pseudo_arguments_as_selector_lists() {
    let selectors = parse("p, img:not(a , div) {", 10);
    // An unbalanced `{` is just a word to the tokenizer.
    assert!(selectors.is_some());
    let selectors = parse("p, img:not(a , div)", 10).unwrap();
    assert_eq!(selectors.len(), 2);
    let not = &selectors[1].nodes[1];
    assert_eq!(not.kind, Kind::Pseudo);
    assert_eq!(not.value, ":not");
    assert_eq!((not.start, not.end), (16, 29));
    let args = not.args.as_ref().unwrap();
    assert_eq!((args.open, args.close), (20, 28));
    assert_eq!(args.selectors.len(), 2);
    assert_eq!(args.selectors[1].nodes[0].value, "div");
  }

  #[test]
  fn reads_attribute_parts() {
    let selectors = parse("[class ^= top i]", 0).unwrap();
    let attr = selectors[0].nodes[0].attr.as_ref().unwrap();
    assert_eq!(attr.name, "class");
    assert_eq!(attr.operator.as_deref(), Some("^="));
    let value = attr.value.as_ref().unwrap();
    assert_eq!(
      (value.raw.as_str(), value.start, value.end),
      ("top", 10, 13)
    );
    assert_eq!(attr.flag.as_deref(), Some("i"));

    let selectors = parse("[href='te\\'s']", 0).unwrap();
    let value = selectors[0].nodes[0]
      .attr
      .as_ref()
      .unwrap()
      .value
      .clone()
      .unwrap();
    assert_eq!(value.quote, Some('\''));
    assert_eq!(value.unescaped, "te's");
  }

  #[test]
  fn fails_where_postcss_selector_parser_throws() {
    for text in ["a]", "a;", ":", "a)", "[\"x\"]", "a\"b", "/* x"] {
      assert!(parse(text, 0).is_none(), "{text}");
    }
  }

  #[test]
  fn splits_rule_selectors_like_postcss() {
    assert_eq!(
      rule_selectors("a, b:is(c, d) , [e=','] f"),
      vec!["a", "b:is(c, d)", "[e=','] f"]
    );
    assert_eq!(strip_comments("a /* b */ c/**/"), "a  c");
  }

  #[test]
  fn escapes_like_cssesc() {
    assert_eq!(cssesc("te's\"t", Escape::DoubleQuoted), "\"te's\\\"t\"");
    assert_eq!(cssesc("'test'", Escape::DoubleQuoted), "\"'test'\"");
    assert_eq!(cssesc("te's't", Escape::Identifier), "te\\'s\\'t");
    assert_eq!(cssesc("_blank", Escape::Identifier), "_blank");
    assert_eq!(cssesc("caf\u{e9}", Escape::DoubleQuoted), "\"caf\\E9\"");
    assert_eq!(cssesc("\u{e9}a", Escape::DoubleQuoted), "\"\\E9 a\"");
    assert_eq!(cssesc("1a", Escape::Identifier), "\\31 a");
    assert_eq!(cssesc("-1", Escape::Identifier), "\\-1");
  }

  #[test]
  fn unescapes_like_postcss_selector_parser() {
    assert_eq!(unescape("a\\:b"), "a:b");
    assert_eq!(unescape("\\31 0"), "10");
    assert_eq!(unescape("\\\"x\\\""), "\"x\"");
  }
}
