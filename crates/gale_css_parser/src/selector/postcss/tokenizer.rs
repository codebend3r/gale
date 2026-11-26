//! postcss-selector-parser's tokenizer.

use crate::scan::{find_unescaped, is_css_space as is_space};

/// A token type, as postcss-selector-parser's tokenizer names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tok {
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
pub(super) struct Token {
  pub(super) kind: Tok,
  pub(super) start: usize,
  pub(super) end: usize,
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
pub(super) fn tokenize(text: &str) -> Option<Vec<Token>> {
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
      (Tok::Str, find_unescaped(bytes, start + 1, b)? + 1)
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
