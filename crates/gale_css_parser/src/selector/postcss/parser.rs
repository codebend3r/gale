//! postcss-selector-parser's parser, over the tokens of [`super::tokenizer`].

use super::escape::unescape;
use super::tokenizer::{Tok, Token};
use super::{Args, Attr, AttrValue, Kind, Node, Selector};

/// Whether a token is whitespace or a comment.
fn is_trivia(kind: Tok) -> bool {
  matches!(kind, Tok::Space | Tok::Comment)
}

/// The parser state: tokens and a cursor over them.
pub(super) struct Parser<'a> {
  pub(super) text: &'a str,
  pub(super) base: usize,
  pub(super) tokens: Vec<Token>,
  pub(super) position: usize,
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
  pub(super) fn parse(&mut self, selectors: &mut Vec<Selector>, top: bool) -> Option<()> {
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
