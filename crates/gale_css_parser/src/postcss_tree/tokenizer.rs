//! The tokenizer: a port of `postcss/lib/tokenize.js` and
//! `postcss-scss/lib/scss-tokenize.js`.

use crate::scan::{find_unescaped, is_css_space as is_space};

/// Which PostCSS parser to imitate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Flavor {
  /// `postcss`.
  Css,
  /// `postcss-scss`.
  Scss,
  /// `postcss-less`, which keeps PostCSS's tokenizer.
  Less,
}

/// A token type, named after PostCSS's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tok {
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
pub(super) struct Token {
  pub(super) kind: Tok,
  pub(super) start: usize,
  pub(super) end: usize,
}

impl Token {
  /// Whether PostCSS gives the token type `comment`.
  pub(super) fn is_comment(&self) -> bool {
    matches!(self.kind, Tok::Comment | Tok::InlineComment)
  }

  /// Whether the token is whitespace or a comment.
  pub(super) fn is_space_or_comment(&self) -> bool {
    self.kind == Tok::Space || self.is_comment()
  }
}

/// Splits the source into PostCSS tokens, with `back` to push tokens back.
pub(super) struct Tokenizer<'a> {
  css: &'a str,
  bytes: &'a [u8],
  scss: bool,
  pub(super) pos: usize,
  /// Words seen so far, for the `url(` check (PostCSS's `buffer`).
  buffer: Vec<Token>,
  /// Tokens pushed back, read again before new ones.
  pub(super) returned: Vec<Token>,
  /// The end of the last `(` that could not be read as one token.
  last_bad_paren: Option<usize>,
}

impl<'a> Tokenizer<'a> {
  /// A tokenizer over `css`; `scss` switches on postcss-scss's changes.
  pub(super) fn new(css: &'a str, scss: bool) -> Self {
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
  pub(super) fn end_of_file(&self) -> bool {
    self.returned.is_empty() && self.pos >= self.bytes.len()
  }

  /// Push a token back to be read again.
  pub(super) fn back(&mut self, token: Token) {
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
  pub(super) fn next_token(&mut self) -> Option<Token> {
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
      return match find_unescaped(self.bytes, pos + 1, b')') {
        Some(close) => (Tok::Brackets, close + 1),
        None => (Tok::Brackets, pos + 1),
      };
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
