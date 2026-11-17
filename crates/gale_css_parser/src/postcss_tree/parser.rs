//! The parser: a port of `postcss/lib/parser.js` with the `postcss-scss`
//! and `postcss-less` changes, building the tree from the token stream.

use super::tokenizer::{Flavor, Tok, Token, Tokenizer};
use super::{Node, NodeKind};

/// Builds the tree from the token stream.
pub(super) struct Parser<'a> {
  css: &'a str,
  flavor: Flavor,
  tokenizer: Tokenizer<'a>,
  pub(super) nodes: Vec<Node>,
  pub(super) root: Vec<usize>,
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
  pub(super) root_semicolon: bool,
}

impl<'a> Parser<'a> {
  /// A parser over `css` for `flavor`.
  pub(super) fn new(css: &'a str, flavor: Flavor) -> Self {
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
  pub(super) fn parse(&mut self) {
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
