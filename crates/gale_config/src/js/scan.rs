//! Splitting JavaScript source into tokens: the one place that knows where
//! strings, template literals, regular expressions and comments begin and
//! end.
//!
//! Every offset a token carries is a byte offset taken from `char_indices`,
//! so slicing the source with it never lands inside a multibyte character.

/// What a token is.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Kind {
  /// An identifier or keyword.
  Ident,
  /// A numeric literal, as written.
  Number,
  /// A string literal's value, escapes decoded.
  Str(String),
  /// A template literal's value, escapes decoded; any `${...}` is kept as
  /// written.
  Template(String),
  /// A regular expression literal, as written (`/source/flags`).
  Regex,
  /// Punctuation: `...`, `?.`, `=>` or a single character; `""` for a
  /// character that is none of [`PUNCTUATION`].
  Punct(&'static str),
}

/// One token and where it sits in the source.
#[derive(Debug, Clone)]
pub(super) struct Token {
  pub kind: Kind,
  pub start: usize,
  pub end: usize,
  /// A line break separates this token from the previous one, which is
  /// where JavaScript may end a statement without a `;`.
  pub newline_before: bool,
}

impl Token {
  /// Whether this token is the punctuation `p`.
  pub fn is(&self, p: &str) -> bool {
    matches!(self.kind, Kind::Punct(q) if q == p)
  }
}

/// The punctuation the reader tells apart; anything else is one character.
const PUNCTUATION: &[&str] = &[
  "...", "?.", "=>", "{", "}", "(", ")", "[", "]", ";", ",", ":", "=", ".", "?", "+", "-", "*",
  "/", "%", "!", "<", ">", "&", "|", "^", "~", "@", "#",
];

/// Tokenize `src`.  Comments are dropped; text the scanner does not
/// recognise becomes single-character punctuation, so this never fails.
pub(super) fn tokenize(src: &str) -> Vec<Token> {
  let mut scanner = Scanner {
    src,
    pos: 0,
    tokens: Vec::new(),
  };
  // A `#!` line is only allowed first, and is not JavaScript.
  if src.starts_with("#!") {
    scanner.pos = src.find('\n').unwrap_or(src.len());
  }
  scanner.run(None);
  scanner.tokens
}

struct Scanner<'a> {
  src: &'a str,
  pos: usize,
  tokens: Vec<Token>,
}

impl Scanner<'_> {
  fn peek(&self) -> Option<char> {
    self.src[self.pos..].chars().next()
  }

  fn peek_second(&self) -> Option<char> {
    self.src[self.pos..].chars().nth(1)
  }

  fn bump(&mut self) -> Option<char> {
    let c = self.peek()?;
    self.pos += c.len_utf8();
    Some(c)
  }

  /// Scan tokens until the end of input or, inside a template literal's
  /// `${`, until the `}` that closes it (which is consumed, not emitted).
  fn run(&mut self, mut substitution_depth: Option<usize>) {
    let mut newline_before = false;
    while let Some(c) = self.peek() {
      let start = self.pos;
      if c.is_whitespace() {
        newline_before |= matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}');
        self.bump();
        continue;
      }
      if c == '/' && self.peek_second() == Some('/') {
        self.pos = self.src[start..]
          .find('\n')
          .map_or(self.src.len(), |n| start + n);
        continue;
      }
      if c == '/' && self.peek_second() == Some('*') {
        let end = self.src[start + 2..]
          .find("*/")
          .map_or(self.src.len(), |n| start + 2 + n + 2);
        newline_before |= self.src[start..end].contains('\n');
        self.pos = end;
        continue;
      }
      if let Some(depth) = substitution_depth.as_mut() {
        match c {
          '{' => *depth += 1,
          '}' if *depth == 0 => {
            self.bump();
            return;
          }
          '}' => *depth -= 1,
          _ => {}
        }
      }

      let kind = if c == '\'' || c == '"' {
        Kind::Str(self.string(c))
      } else if c == '`' {
        Kind::Template(self.template())
      } else if c.is_ascii_digit()
        || (c == '.' && self.peek_second().is_some_and(|d| d.is_ascii_digit()))
      {
        self.number();
        Kind::Number
      } else if is_ident_start(c) {
        while self.peek().is_some_and(is_ident_part) {
          self.bump();
        }
        Kind::Ident
      } else if c == '/' && self.regex_allowed() && self.regex() {
        Kind::Regex
      } else {
        match PUNCTUATION
          .iter()
          .find(|p| self.src[start..].starts_with(**p))
        {
          Some(punct) => {
            self.pos += punct.len();
            Kind::Punct(punct)
          }
          None => {
            self.bump();
            Kind::Punct("")
          }
        }
      };
      self.tokens.push(Token {
        kind,
        start,
        end: self.pos,
        newline_before,
      });
      newline_before = false;
    }
  }

  /// A `/` starts a regular expression where a value is expected: at the
  /// start, or after punctuation other than a closing bracket.
  fn regex_allowed(&self) -> bool {
    match self.tokens.last() {
      None => true,
      Some(token) => match token.kind {
        Kind::Punct(p) => !matches!(p, ")" | "]" | "}"),
        Kind::Ident => matches!(&self.src[token.start..token.end], "return" | "typeof"),
        _ => false,
      },
    }
  }

  /// Consume a regular expression literal.  When there is none (a line
  /// ends first), consume nothing and say so.
  fn regex(&mut self) -> bool {
    let start = self.pos;
    self.bump();
    let mut in_class = false;
    loop {
      match self.bump() {
        None | Some('\n' | '\r') => {
          self.pos = start;
          return false;
        }
        Some('\\') => {
          self.bump();
        }
        Some('[') => in_class = true,
        Some(']') => in_class = false,
        Some('/') if !in_class => break,
        Some(_) => {}
      }
    }
    while self.peek().is_some_and(is_ident_part) {
      self.bump();
    }
    true
  }

  fn number(&mut self) {
    let start = self.pos;
    while let Some(c) = self.peek() {
      let so_far = &self.src[start..self.pos];
      let exponent_sign = matches!(c, '+' | '-')
        && so_far.ends_with(['e', 'E'])
        && !so_far.starts_with("0x")
        && !so_far.starts_with("0X");
      if c.is_ascii_alphanumeric() || c == '.' || c == '_' || exponent_sign {
        self.bump();
      } else {
        break;
      }
    }
  }

  /// Consume a quoted string, returning its value.
  fn string(&mut self, quote: char) -> String {
    self.bump();
    let body_start = self.pos;
    while let Some(c) = self.peek() {
      if c == quote || c == '\n' {
        break;
      }
      self.bump();
      if c == '\\' {
        self.bump();
      }
    }
    let value = cook(&self.src[body_start..self.pos]);
    if self.peek() == Some(quote) {
      self.bump();
    }
    value
  }

  /// Consume a template literal, returning its value.  A `${...}` is
  /// scanned like any other code (so a `` ` `` or `}` inside it is read
  /// correctly) and kept in the value as written.
  fn template(&mut self) -> String {
    self.bump();
    let mut value = String::new();
    let mut chunk_start = self.pos;
    while let Some(c) = self.peek() {
      match c {
        '`' => break,
        '\\' => {
          self.bump();
          self.bump();
        }
        '$' if self.peek_second() == Some('{') => {
          value.push_str(&cook(&self.src[chunk_start..self.pos]));
          let substitution_start = self.pos;
          self.pos += 2;
          let mut nested = Scanner {
            src: self.src,
            pos: self.pos,
            tokens: Vec::new(),
          };
          nested.run(Some(0));
          self.pos = nested.pos;
          value.push_str(&self.src[substitution_start..self.pos]);
          chunk_start = self.pos;
        }
        _ => {
          self.bump();
        }
      }
    }
    value.push_str(&cook(&self.src[chunk_start..self.pos]));
    self.bump();
    value
  }
}

fn is_ident_start(c: char) -> bool {
  c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_part(c: char) -> bool {
  c.is_alphanumeric() || c == '_' || c == '$'
}

/// The value of a string or template literal body: each escape decoded,
/// line continuations removed, everything else kept.
pub(super) fn cook(body: &str) -> String {
  let mut out = String::with_capacity(body.len());
  let mut chars = body.chars().peekable();
  while let Some(c) = chars.next() {
    if c != '\\' {
      out.push(c);
      continue;
    }
    match chars.next() {
      None => out.push('\\'),
      Some('n') => out.push('\n'),
      Some('r') => out.push('\r'),
      Some('t') => out.push('\t'),
      Some('b') => out.push('\u{8}'),
      Some('f') => out.push('\u{c}'),
      Some('v') => out.push('\u{b}'),
      Some('0') if !chars.peek().is_some_and(char::is_ascii_digit) => out.push('\0'),
      Some('x') => {
        let hex: String = chars.by_ref().take(2).collect();
        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
          Some(decoded) if hex.len() == 2 => out.push(decoded),
          _ => out.push_str(&hex),
        }
      }
      Some('u') => {
        let hex: String = if chars.peek() == Some(&'{') {
          chars.next();
          chars.by_ref().take_while(|&h| h != '}').collect()
        } else {
          chars.by_ref().take(4).collect()
        };
        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
          Some(decoded) => out.push(decoded),
          None => out.push_str(&hex),
        }
      }
      // A line continuation: the backslash and line break vanish.
      Some('\r') => {
        if chars.peek() == Some(&'\n') {
          chars.next();
        }
      }
      Some('\n' | '\u{2028}' | '\u{2029}') => {}
      // Any other escaped character stands for itself (`\'`, `` \` ``, `\d`).
      Some(other) => out.push(other),
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  fn kinds(src: &str) -> Vec<Kind> {
    tokenize(src).into_iter().map(|t| t.kind).collect()
  }

  #[test]
  fn escapes_are_decoded() {
    assert_eq!(cook(r"it\'s"), "it's");
    assert_eq!(cook(r#"say \"hi\""#), r#"say "hi""#);
    assert_eq!(cook(r"\x41B\u{43}\v"), "ABC\u{b}");
    assert_eq!(cook("a\\\nb\tc"), "ab\tc");
    assert_eq!(cook(r"\d\$\\"), r"d$\");
  }

  #[test]
  fn strings_comments_and_regexes_hide_what_looks_like_code() {
    assert_eq!(
      kinds("a /* b */ 'c // d', // e\n/f\\/[/]/g"),
      vec![
        Kind::Ident,
        Kind::Str("c // d".into()),
        Kind::Punct(","),
        Kind::Regex,
      ]
    );
    // After a value, `/` divides.
    assert_eq!(
      kinds("x / 2"),
      vec![Kind::Ident, Kind::Punct("/"), Kind::Number]
    );
  }

  #[test]
  fn template_substitutions_are_kept_as_written() {
    assert_eq!(
      kinds("`a ${ { b: '`}' } } \\` c`"),
      vec![Kind::Template("a ${ { b: '`}' } } ` c".into())]
    );
  }

  #[test]
  fn offsets_fall_on_character_boundaries() {
    let src = "'français — “ok”' é";
    for token in tokenize(src) {
      assert!(src.is_char_boundary(token.start) && src.is_char_boundary(token.end));
    }
    assert_eq!(kinds(src)[1], Kind::Ident);
  }
}
