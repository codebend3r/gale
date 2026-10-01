//! A port of the parts of `@csstools/css-tokenizer` and
//! `@csstools/css-parser-algorithms` that Stylelint's math-function rules
//! use: the CSS Syntax Level 3 tokenizer, and the grouping of tokens into
//! component values (functions, simple blocks, whitespace and comments).
//!
//! Offsets are byte offsets into the tokenized text, where the JavaScript
//! packages use UTF-16 code units; they agree on ASCII text.

/// A token type, as CSS Syntax Level 3 names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
  Comment,
  AtKeyword,
  BadString,
  BadUrl,
  Cdc,
  Cdo,
  Colon,
  Comma,
  Delim,
  Dimension,
  Function,
  Hash,
  Ident,
  Number,
  Percentage,
  Semicolon,
  String,
  Url,
  Whitespace,
  OpenParen,
  CloseParen,
  OpenSquare,
  CloseSquare,
  OpenCurly,
  CloseCurly,
}

/// One token.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
  pub kind: TokenType,
  /// The token as written (its representation).
  pub raw: String,
  /// Byte offset where the token starts.
  pub start: usize,
  /// Exclusive byte offset where the token ends.
  pub end: usize,
  /// The decoded name of an ident, function, at-keyword or hash, or the
  /// character of a delim.
  pub value: String,
  /// The numeric value of a number, percentage or dimension.
  pub number: f64,
  /// The explicit sign of a number, percentage or dimension.
  pub sign: Option<char>,
  /// The decoded unit of a dimension.
  pub unit: String,
}

impl Token {
  /// A token of `kind` covering `start..end` of `source`.
  fn new(kind: TokenType, source: &str, start: usize, end: usize) -> Self {
    Self {
      kind,
      raw: source.get(start..end).unwrap_or("").to_string(),
      start,
      end,
      value: String::new(),
      number: 0.0,
      sign: None,
      unit: String::new(),
    }
  }

  /// Whether this is a number, percentage or dimension.
  pub fn is_numeric(&self) -> bool {
    matches!(
      self.kind,
      TokenType::Number | TokenType::Percentage | TokenType::Dimension
    )
  }

  /// Whether this is the delim `c`.
  pub fn is_delim(&self, c: char) -> bool {
    self.kind == TokenType::Delim && self.value.starts_with(c) && self.value.len() == c.len_utf8()
  }

  /// `@csstools/css-tokenizer`'s `mutateUnit`: give a dimension a new unit,
  /// rewriting its representation as the sign (`+` only), the number as
  /// JavaScript prints it, and the unit serialised as an identifier.
  pub fn mutate_unit(&mut self, unit: &str) {
    let mut serialized = serialize_ident(unit);
    if serialized.starts_with('e') {
      // An `e` right after the number would read as an exponent.
      serialized = format!("\\65 {}", &serialized[1..]);
    }
    let sign = if self.sign == Some('+') { "+" } else { "" };
    self.raw = format!("{sign}{}{serialized}", js_number_to_string(self.number));
    self.unit = unit.to_string();
  }

  /// `@csstools/css-tokenizer`'s `mutateIdent`: give an ident a new value,
  /// rewriting its representation as the value serialised as an
  /// identifier.
  pub fn mutate_ident(&mut self, value: &str) {
    self.raw = serialize_ident(value);
    self.value = value.to_string();
  }
}

/// A number as JavaScript's `Number.prototype.toString` prints it, for the
/// values CSS numbers take.
fn js_number_to_string(n: f64) -> String {
  if n == 0.0 {
    return "0".to_string();
  }
  if n.fract() == 0.0 && n.abs() < 1e21 {
    return format!("{}", n as i64);
  }
  format!("{n}")
}

/// `@csstools/css-tokenizer`'s `serializeIdent`: escape what an identifier
/// cannot hold as is.
pub fn serialize_ident(text: &str) -> String {
  let chars: Vec<char> = text.chars().collect();
  let mut out = String::new();
  let escape_code_point = |c: char, out: &mut String| out.push_str(&format!("\\{:x} ", c as u32));
  let mut rest_from = 0;
  match chars.as_slice() {
    [] => return out,
    ['\0', ..] => {
      out.push('\u{FFFD}');
      rest_from = 1;
    }
    ['-', '-', ..] => {
      out.push_str("--");
      rest_from = 2;
    }
    ['-', second, ..] => {
      out.push('-');
      if is_ident_start(*second) {
        out.push(*second);
      } else {
        escape_code_point(*second, &mut out);
      }
      rest_from = 2;
    }
    ['-'] => return "\\-".to_string(),
    [first, ..] => {
      if is_ident_start(*first) {
        out.push(*first);
      } else {
        escape_code_point(*first, &mut out);
      }
      rest_from = 1;
    }
  }
  for &c in &chars[rest_from..] {
    if c == '\0' {
      out.push('\u{FFFD}');
    } else if is_ident_char(c) {
      out.push(c);
    } else {
      out.push('\\');
      out.push(c);
    }
  }
  out
}

/// Whether `c` can start an identifier: a letter, `_`, or one of the
/// non-ASCII ranges CSS Syntax Level 3 allows.
fn is_ident_start(c: char) -> bool {
  c.is_ascii_alphabetic() || c == '_' || is_non_ascii_ident(c)
}

/// CSS Syntax Level 3's non-ASCII ident code points.
fn is_non_ascii_ident(c: char) -> bool {
  matches!(c as u32,
    0xB7
      | 0xC0..=0xD6
      | 0xD8..=0xF6
      | 0xF8..=0x37D
      | 0x37F..=0x1FFF
      | 0x200C
      | 0x200D
      | 0x203F
      | 0x2040
      | 0x2070..=0x218F
      | 0x2C00..=0x2FEF
      | 0x3001..=0xD7FF
      | 0xF900..=0xFDCF
      | 0xFDF0..=0xFFFD
      | 0x10000..)
}

/// Whether `c` can continue an identifier.
fn is_ident_char(c: char) -> bool {
  is_ident_start(c) || c.is_ascii_digit() || c == '-'
}

/// Whether `c` is whitespace to the tokenizer.
fn is_whitespace(c: char) -> bool {
  matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0c')
}

/// Whether `c` is a newline to the tokenizer.
fn is_newline(c: char) -> bool {
  matches!(c, '\n' | '\r' | '\x0c')
}

/// The tokenizer's cursor over the code points of the input.
struct Reader<'a> {
  source: &'a str,
  chars: Vec<(usize, char)>,
  pos: usize,
}

impl Reader<'_> {
  /// The code point `n` past the cursor.
  fn peek(&self, n: usize) -> Option<char> {
    self.chars.get(self.pos + n).map(|(_, c)| *c)
  }

  /// The byte offset of the cursor.
  fn offset(&self) -> usize {
    self
      .chars
      .get(self.pos)
      .map_or(self.source.len(), |(i, _)| *i)
  }

  /// Whether the code points at `n` and `n + 1` past the cursor are a
  /// valid escape.
  fn valid_escape_at(&self, n: usize) -> bool {
    self.peek(n) == Some('\\') && !self.peek(n + 1).is_some_and(is_newline)
  }

  /// Whether the code points from `n` past the cursor would start an
  /// identifier.
  fn starts_ident_at(&self, n: usize) -> bool {
    match self.peek(n) {
      Some('-') => {
        self
          .peek(n + 1)
          .is_some_and(|c| is_ident_start(c) || c == '-')
          || self.valid_escape_at(n + 1)
      }
      Some('\\') => self.valid_escape_at(n),
      Some(c) => is_ident_start(c),
      None => false,
    }
  }

  /// Whether the code points at the cursor would start a number.
  fn starts_number(&self) -> bool {
    let digit = |n: usize| self.peek(n).is_some_and(|c| c.is_ascii_digit());
    match self.peek(0) {
      Some('+' | '-') => digit(1) || (self.peek(1) == Some('.') && digit(2)),
      Some('.') => digit(1),
      Some(c) => c.is_ascii_digit(),
      None => false,
    }
  }

  /// Consume an escaped code point; the cursor is just past the `\`.
  fn consume_escape(&mut self) -> char {
    let Some(c) = self.peek(0) else {
      return '\u{FFFD}';
    };
    self.pos += 1;
    if !c.is_ascii_hexdigit() {
      return c;
    }
    let mut hex = String::from(c);
    while hex.len() < 6 && self.peek(0).is_some_and(|c| c.is_ascii_hexdigit()) {
      hex.push(self.peek(0).expect("checked"));
      self.pos += 1;
    }
    if self.peek(0) == Some('\r') && self.peek(1) == Some('\n') {
      self.pos += 2;
    } else if self.peek(0).is_some_and(is_whitespace) {
      self.pos += 1;
    }
    let code = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
    match char::from_u32(code) {
      Some(c) if code != 0 => c,
      _ => '\u{FFFD}',
    }
  }

  /// Consume an identifier sequence and return its decoded value.
  fn consume_ident_sequence(&mut self) -> String {
    let mut out = String::new();
    loop {
      match self.peek(0) {
        Some(c) if is_ident_char(c) => {
          out.push(c);
          self.pos += 1;
        }
        Some('\\') if self.valid_escape_at(0) => {
          self.pos += 1;
          out.push(self.consume_escape());
        }
        _ => return out,
      }
    }
  }

  /// Consume a number, returning its text.
  fn consume_number(&mut self) {
    let digit = |r: &Self, n: usize| r.peek(n).is_some_and(|c| c.is_ascii_digit());
    if matches!(self.peek(0), Some('+' | '-')) {
      self.pos += 1;
    }
    while digit(self, 0) {
      self.pos += 1;
    }
    if self.peek(0) == Some('.') && digit(self, 1) {
      self.pos += 2;
      while digit(self, 0) {
        self.pos += 1;
      }
    }
    if matches!(self.peek(0), Some('e' | 'E')) {
      if digit(self, 1) {
        self.pos += 2;
      } else if matches!(self.peek(1), Some('+' | '-')) && digit(self, 2) {
        self.pos += 3;
      } else {
        return;
      }
      while digit(self, 0) {
        self.pos += 1;
      }
    }
  }

  /// Consume a number, percentage or dimension starting at `start`.
  fn consume_numeric(&mut self, start: usize) -> Token {
    let sign = match self.peek(0) {
      Some(c @ ('+' | '-')) => Some(c),
      _ => None,
    };
    self.consume_number();
    let number_text = self.source.get(start..self.offset()).unwrap_or("0");
    let number = parse_float(number_text);
    let (kind, unit) = if self.starts_ident_at(0) {
      (TokenType::Dimension, self.consume_ident_sequence())
    } else if self.peek(0) == Some('%') {
      self.pos += 1;
      (TokenType::Percentage, String::new())
    } else {
      (TokenType::Number, String::new())
    };
    let mut token = Token::new(kind, self.source, start, self.offset());
    token.number = number;
    token.sign = sign;
    token.unit = unit;
    token
  }

  /// Consume an ident, function or url token starting at `start`.
  fn consume_ident_like(&mut self, start: usize) -> Token {
    let name = self.consume_ident_sequence();
    if name.eq_ignore_ascii_case("url") && self.peek(0) == Some('(') {
      self.pos += 1;
      while self.peek(0).is_some_and(is_whitespace) && self.peek(1).is_some_and(is_whitespace) {
        self.pos += 1;
      }
      let quote_next = matches!(self.peek(0), Some('"' | '\''))
        || (self.peek(0).is_some_and(is_whitespace) && matches!(self.peek(1), Some('"' | '\'')));
      if quote_next {
        let mut token = Token::new(TokenType::Function, self.source, start, self.offset());
        token.value = name;
        return token;
      }
      return self.consume_url(start);
    }
    if self.peek(0) == Some('(') {
      self.pos += 1;
      let mut token = Token::new(TokenType::Function, self.source, start, self.offset());
      token.value = name;
      return token;
    }
    let mut token = Token::new(TokenType::Ident, self.source, start, self.offset());
    token.value = name;
    token
  }

  /// Consume a url token; the cursor is just past `url(`.
  fn consume_url(&mut self, start: usize) -> Token {
    while self.peek(0).is_some_and(is_whitespace) {
      self.pos += 1;
    }
    let mut kind = TokenType::Url;
    loop {
      match self.peek(0) {
        None => break,
        Some(')') => {
          self.pos += 1;
          break;
        }
        Some(c) if is_whitespace(c) => {
          while self.peek(0).is_some_and(is_whitespace) {
            self.pos += 1;
          }
          match self.peek(0) {
            None => break,
            Some(')') => {
              self.pos += 1;
              break;
            }
            _ => {
              kind = TokenType::BadUrl;
              self.consume_bad_url_remnants();
              break;
            }
          }
        }
        Some('"' | '\'' | '(') => {
          kind = TokenType::BadUrl;
          self.consume_bad_url_remnants();
          break;
        }
        Some(c) if (c as u32) < 0x20 && c != '\t' || c == '\u{7f}' => {
          kind = TokenType::BadUrl;
          self.consume_bad_url_remnants();
          break;
        }
        Some('\\') => {
          if self.valid_escape_at(0) {
            self.pos += 1;
            self.consume_escape();
          } else {
            kind = TokenType::BadUrl;
            self.consume_bad_url_remnants();
            break;
          }
        }
        Some(_) => self.pos += 1,
      }
    }
    Token::new(kind, self.source, start, self.offset())
  }

  /// Skip the rest of a bad url, up to and including its `)`.
  fn consume_bad_url_remnants(&mut self) {
    loop {
      match self.peek(0) {
        None => return,
        Some(')') => {
          self.pos += 1;
          return;
        }
        Some('\\') if self.valid_escape_at(0) => {
          self.pos += 1;
          self.consume_escape();
        }
        Some(_) => self.pos += 1,
      }
    }
  }

  /// Consume a string token; the cursor is on the opening quote.
  fn consume_string(&mut self, start: usize) -> Token {
    let quote = self.peek(0).expect("on a quote");
    self.pos += 1;
    loop {
      match self.peek(0) {
        None => break,
        Some(c) if c == quote => {
          self.pos += 1;
          break;
        }
        Some(c) if is_newline(c) => {
          return Token::new(TokenType::BadString, self.source, start, self.offset());
        }
        Some('\\') => {
          self.pos += 1;
          match self.peek(0) {
            None => {}
            Some('\r') if self.peek(1) == Some('\n') => self.pos += 2,
            Some(c) if is_newline(c) => self.pos += 1,
            Some(_) => {
              self.consume_escape();
            }
          }
        }
        Some(_) => self.pos += 1,
      }
    }
    Token::new(TokenType::String, self.source, start, self.offset())
  }

  /// Consume the next token.
  fn next_token(&mut self) -> Option<Token> {
    let c = self.peek(0)?;
    let start = self.offset();
    let single = |r: &mut Self, kind: TokenType| {
      r.pos += 1;
      Token::new(kind, r.source, start, r.offset())
    };
    let delim = |r: &mut Self| {
      r.pos += 1;
      let mut token = Token::new(TokenType::Delim, r.source, start, r.offset());
      token.value = c.to_string();
      token
    };
    Some(match c {
      '/' if self.peek(1) == Some('*') => {
        self.pos += 2;
        while self.peek(0).is_some() && !(self.peek(0) == Some('*') && self.peek(1) == Some('/')) {
          self.pos += 1;
        }
        self.pos = (self.pos + 2).min(self.chars.len());
        Token::new(TokenType::Comment, self.source, start, self.offset())
      }
      c if is_whitespace(c) => {
        while self.peek(0).is_some_and(is_whitespace) {
          self.pos += 1;
        }
        Token::new(TokenType::Whitespace, self.source, start, self.offset())
      }
      '"' | '\'' => self.consume_string(start),
      '#' => {
        if self.peek(1).is_some_and(is_ident_char) || self.valid_escape_at(1) {
          self.pos += 1;
          let value = self.consume_ident_sequence();
          let mut token = Token::new(TokenType::Hash, self.source, start, self.offset());
          token.value = value;
          token
        } else {
          delim(self)
        }
      }
      '(' => single(self, TokenType::OpenParen),
      ')' => single(self, TokenType::CloseParen),
      '[' => single(self, TokenType::OpenSquare),
      ']' => single(self, TokenType::CloseSquare),
      '{' => single(self, TokenType::OpenCurly),
      '}' => single(self, TokenType::CloseCurly),
      ',' => single(self, TokenType::Comma),
      ':' => single(self, TokenType::Colon),
      ';' => single(self, TokenType::Semicolon),
      '+' | '.' => {
        if self.starts_number() {
          self.consume_numeric(start)
        } else {
          delim(self)
        }
      }
      '-' => {
        if self.starts_number() {
          self.consume_numeric(start)
        } else if self.peek(1) == Some('-') && self.peek(2) == Some('>') {
          self.pos += 3;
          Token::new(TokenType::Cdc, self.source, start, self.offset())
        } else if self.starts_ident_at(0) {
          self.consume_ident_like(start)
        } else {
          delim(self)
        }
      }
      '<'
        if self.peek(1) == Some('!') && self.peek(2) == Some('-') && self.peek(3) == Some('-') =>
      {
        self.pos += 4;
        Token::new(TokenType::Cdo, self.source, start, self.offset())
      }
      '@' => {
        if self.starts_ident_at(1) {
          self.pos += 1;
          let value = self.consume_ident_sequence();
          let mut token = Token::new(TokenType::AtKeyword, self.source, start, self.offset());
          token.value = value;
          token
        } else {
          delim(self)
        }
      }
      '\\' => {
        if self.valid_escape_at(0) {
          self.consume_ident_like(start)
        } else {
          delim(self)
        }
      }
      c if c.is_ascii_digit() => self.consume_numeric(start),
      c if is_ident_start(c) => self.consume_ident_like(start),
      _ => delim(self),
    })
  }
}

/// JavaScript's `parseFloat` for the text of a CSS number.
fn parse_float(text: &str) -> f64 {
  text.parse().unwrap_or(0.0)
}

/// Tokenize `css` into CSS tokens (without the end-of-file token).
pub fn tokenize(css: &str) -> Vec<Token> {
  let mut reader = Reader {
    source: css,
    chars: css.char_indices().collect(),
    pos: 0,
  };
  let mut tokens = Vec::new();
  while let Some(token) = reader.next_token() {
    tokens.push(token);
  }
  tokens
}

// ---------------------------------------------------------------------------
// Component values
// ---------------------------------------------------------------------------

/// A component value: a token, or a function or simple block holding more.
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentValue {
  /// A preserved token.
  Token(Token),
  /// Consecutive whitespace tokens.
  Whitespace(Vec<Token>),
  /// A comment.
  Comment(Token),
  /// `name( ... )`.
  Function {
    name: Token,
    value: Vec<ComponentValue>,
    end: Option<Token>,
  },
  /// `( ... )`, `[ ... ]` or `{ ... }`.
  SimpleBlock {
    open: Token,
    value: Vec<ComponentValue>,
    end: Option<Token>,
  },
}

impl ComponentValue {
  /// Byte offset where the value starts.
  pub fn start(&self) -> usize {
    match self {
      ComponentValue::Token(t) | ComponentValue::Comment(t) => t.start,
      ComponentValue::Whitespace(ts) => ts.first().map_or(0, |t| t.start),
      ComponentValue::Function { name, .. } => name.start,
      ComponentValue::SimpleBlock { open, .. } => open.start,
    }
  }

  /// Exclusive byte offset where the value ends.
  pub fn end(&self) -> usize {
    match self {
      ComponentValue::Token(t) | ComponentValue::Comment(t) => t.end,
      ComponentValue::Whitespace(ts) => ts.last().map_or(0, |t| t.end),
      ComponentValue::Function { name, value, end } => end
        .as_ref()
        .map(|t| t.end)
        .or_else(|| value.last().map(ComponentValue::end))
        .unwrap_or(name.end),
      ComponentValue::SimpleBlock { open, value, end } => end
        .as_ref()
        .map(|t| t.end)
        .or_else(|| value.last().map(ComponentValue::end))
        .unwrap_or(open.end),
    }
  }

  /// The value as text.
  pub fn to_css(&self) -> String {
    match self {
      ComponentValue::Token(t) | ComponentValue::Comment(t) => t.raw.clone(),
      ComponentValue::Whitespace(ts) => ts.iter().map(|t| t.raw.as_str()).collect(),
      ComponentValue::Function { name, value, end }
      | ComponentValue::SimpleBlock {
        open: name,
        value,
        end,
      } => {
        let inner: String = value.iter().map(ComponentValue::to_css).collect();
        format!(
          "{}{inner}{}",
          name.raw,
          end.as_ref().map_or("", |t| t.raw.as_str())
        )
      }
    }
  }

  /// Whether this is whitespace or a comment.
  pub fn is_whitespace_or_comment(&self) -> bool {
    matches!(
      self,
      ComponentValue::Whitespace(_) | ComponentValue::Comment(_)
    )
  }
}

/// `@csstools/css-parser-algorithms`'s `parseListOfComponentValues`.
pub fn parse_list_of_component_values(tokens: Vec<Token>) -> Vec<ComponentValue> {
  let mut iter = tokens.into_iter().peekable();
  let mut out = Vec::new();
  while iter.peek().is_some() {
    if let Some(value) = consume_component_value(&mut iter) {
      out.push(value);
    }
  }
  out
}

/// Consume one component value.
fn consume_component_value(
  iter: &mut std::iter::Peekable<std::vec::IntoIter<Token>>,
) -> Option<ComponentValue> {
  let token = iter.next()?;
  Some(match token.kind {
    TokenType::Whitespace => {
      let mut run = vec![token];
      while iter.peek().is_some_and(|t| t.kind == TokenType::Whitespace) {
        run.push(iter.next().expect("peeked"));
      }
      ComponentValue::Whitespace(run)
    }
    TokenType::Comment => ComponentValue::Comment(token),
    TokenType::Function => {
      let (value, end) = consume_until(iter, TokenType::CloseParen);
      ComponentValue::Function {
        name: token,
        value,
        end,
      }
    }
    TokenType::OpenParen | TokenType::OpenSquare | TokenType::OpenCurly => {
      let closer = match token.kind {
        TokenType::OpenParen => TokenType::CloseParen,
        TokenType::OpenSquare => TokenType::CloseSquare,
        _ => TokenType::CloseCurly,
      };
      let (value, end) = consume_until(iter, closer);
      ComponentValue::SimpleBlock {
        open: token,
        value,
        end,
      }
    }
    _ => ComponentValue::Token(token),
  })
}

/// Consume component values up to the `closer` token, which is returned
/// separately (`None` when the input ends first).
fn consume_until(
  iter: &mut std::iter::Peekable<std::vec::IntoIter<Token>>,
  closer: TokenType,
) -> (Vec<ComponentValue>, Option<Token>) {
  let mut value = Vec::new();
  loop {
    match iter.peek() {
      None => return (value, None),
      Some(t) if t.kind == closer => return (value, iter.next()),
      Some(_) => {
        if let Some(v) = consume_component_value(iter) {
          value.push(v);
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `(kind, raw)` of every token.
  fn kinds(css: &str) -> Vec<(TokenType, String)> {
    tokenize(css).into_iter().map(|t| (t.kind, t.raw)).collect()
  }

  #[test]
  fn tokenizes_numbers_dimensions_and_operators() {
    use TokenType::*;
    assert_eq!(
      kinds("2px+1px -3 .5% 1e3 1em"),
      vec![
        (Dimension, "2px".into()),
        (Dimension, "+1px".into()),
        (Whitespace, " ".into()),
        (Number, "-3".into()),
        (Whitespace, " ".into()),
        (Percentage, ".5%".into()),
        (Whitespace, " ".into()),
        (Number, "1e3".into()),
        (Whitespace, " ".into()),
        (Dimension, "1em".into()),
      ]
    );
    let dim = &tokenize("1px- 2")[0];
    assert_eq!(dim.unit, "px-");
    assert_eq!(kinds("a+b")[1], (Delim, "+".into()));
  }

  #[test]
  fn tokenizes_idents_functions_escapes_and_urls() {
    use TokenType::*;
    let tokens = tokenize("1\\23 a- calc( url(x y) --x @k #h");
    assert_eq!(tokens[0].kind, Dimension);
    assert_eq!(tokens[0].unit, "#a-");
    assert_eq!(tokens[2].kind, Function);
    assert_eq!(tokens[2].value, "calc");
    assert_eq!(tokens[4].kind, BadUrl);
    assert_eq!(tokens[6].kind, Ident);
    assert_eq!(tokens[8].kind, AtKeyword);
    assert_eq!(tokens[10].kind, Hash);
  }

  #[test]
  fn mutations_reserialise_like_csstools() {
    let mut dim = tokenize("1\\23 -")[0].clone();
    dim.mutate_unit("#");
    assert_eq!(dim.raw, "1\\23 ");
    let mut dim = tokenize("+1.50px-")[0].clone();
    dim.mutate_unit("px");
    assert_eq!(dim.raw, "+1.5px");
    let mut dim = tokenize("2ex-")[0].clone();
    dim.mutate_unit("ex");
    assert_eq!(dim.raw, "2\\65 x");
    let mut ident = tokenize("g-")[0].clone();
    ident.mutate_ident("g");
    assert_eq!(ident.raw, "g");
  }

  #[test]
  fn groups_component_values() {
    let values = parse_list_of_component_values(tokenize("calc(1px + (2px)) [a] x"));
    assert!(matches!(values[0], ComponentValue::Function { .. }));
    assert_eq!(values[0].to_css(), "calc(1px + (2px))");
    assert_eq!((values[0].start(), values[0].end()), (0, 17));
    assert!(matches!(values[2], ComponentValue::SimpleBlock { .. }));
    let roundtrip: String = values.iter().map(ComponentValue::to_css).collect();
    assert_eq!(roundtrip, "calc(1px + (2px)) [a] x");
  }
}
