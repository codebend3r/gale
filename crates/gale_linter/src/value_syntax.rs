//! The value syntax tree css-tree builds for `parse(value, { context:
//! 'value' })`, reduced to what `declaration-block-no-duplicate-properties`
//! compares when it decides whether two values are the same.

/// One node of a parsed value.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
  /// `ident`, with its name.
  Identifier(String),
  /// `name(...)`, with its name and arguments.
  Function(String, Vec<Node>),
  /// A number with a unit, with the unit as written.
  Dimension(String),
  /// A plain number.
  Number,
  /// `n%`.
  Percentage,
  /// A quoted string.
  String,
  /// `url(...)`.
  Url,
  /// `#hash`.
  Hash,
  /// `U+...`.
  UnicodeRange,
  /// `,`, `/`, `*`, `+` or `-`; css-tree keeps no name for these, so all
  /// operators look alike.
  Operator,
  /// `( ... )`.
  Parentheses(Vec<Node>),
  /// `[ ... ]`.
  Brackets(Vec<Node>),
  /// The unparsed fallback of `var(--x, ...)`.
  Raw,
}

impl Node {
  /// css-tree's `name`: identifiers and functions have one.
  pub fn name(&self) -> &str {
    match self {
      Node::Identifier(name) | Node::Function(name, _) => name,
      _ => "",
    }
  }

  /// css-tree's `unit`: dimensions have one.
  pub fn unit(&self) -> &str {
    match self {
      Node::Dimension(unit) => unit,
      _ => "",
    }
  }

  /// css-tree's `children`, for functions, parentheses and brackets.
  pub fn children(&self) -> Option<&[Node]> {
    match self {
      Node::Function(_, children) | Node::Parentheses(children) | Node::Brackets(children) => {
        Some(children)
      }
      _ => None,
    }
  }
}

/// Parse `value`, or `None` where css-tree would throw.
pub fn parse(value: &str) -> Option<Vec<Node>> {
  let mut parser = Parser {
    chars: value.chars().collect(),
    pos: 0,
  };
  let nodes = parser.sequence()?;
  (parser.pos >= parser.chars.len()).then_some(nodes)
}

/// A cursor over the value's characters.
struct Parser {
  chars: Vec<char>,
  pos: usize,
}

impl Parser {
  /// The character `offset` places ahead, or `'\0'` past the end.
  fn peek(&self, offset: usize) -> char {
    self.chars.get(self.pos + offset).copied().unwrap_or('\0')
  }

  /// Skip whitespace and comments.
  fn skip_space_and_comments(&mut self) {
    loop {
      let c = self.peek(0);
      if c.is_whitespace() && c != '\0' {
        self.pos += 1;
      } else if c == '/' && self.peek(1) == '*' {
        self.pos += 2;
        while self.pos < self.chars.len() && !(self.peek(0) == '*' && self.peek(1) == '/') {
          self.pos += 1;
        }
        self.pos = (self.pos + 2).min(self.chars.len());
      } else {
        return;
      }
    }
  }

  /// css-tree's `readSequence`: nodes up to the end, a `)` or a `]`.
  /// `None` on a token the value scope does not recognise.
  fn sequence(&mut self) -> Option<Vec<Node>> {
    let mut nodes = Vec::new();
    loop {
      self.skip_space_and_comments();
      if self.pos >= self.chars.len() || matches!(self.peek(0), ')' | ']') {
        return Some(nodes);
      }
      nodes.push(self.node()?);
    }
  }

  /// Read the node at the cursor.
  fn node(&mut self) -> Option<Node> {
    let c = self.peek(0);
    match c {
      ',' => {
        self.pos += 1;
        Some(Node::Operator)
      }
      '(' => {
        self.pos += 1;
        let children = self.sequence()?;
        self.expect(')')?;
        Some(Node::Parentheses(children))
      }
      '[' => {
        self.pos += 1;
        let children = self.sequence()?;
        self.expect(']')?;
        Some(Node::Brackets(children))
      }
      '"' | '\'' => {
        self.pos += 1;
        while self.pos < self.chars.len() && self.peek(0) != c {
          if self.peek(0) == '\\' {
            self.pos += 1;
          }
          self.pos += 1;
        }
        self.pos = (self.pos + 1).min(self.chars.len());
        Some(Node::String)
      }
      '#' if self.starts_ident_char(1) || self.peek(1).is_ascii_digit() => {
        self.pos += 1;
        self.ident_chars();
        Some(Node::Hash)
      }
      _ if self.starts_number() => {
        self.number();
        if self.peek(0) == '%' {
          self.pos += 1;
          Some(Node::Percentage)
        } else if self.starts_ident(0) {
          let unit = self.ident_chars();
          Some(Node::Dimension(unit))
        } else {
          Some(Node::Number)
        }
      }
      _ if self.starts_ident(0) => {
        let name = self.ident_chars();
        if self.peek(0) != '(' {
          if name.len() >= 2 && name[..2].eq_ignore_ascii_case("u+") {
            return Some(Node::UnicodeRange);
          }
          return Some(Node::Identifier(name));
        }
        self.pos += 1;
        if name.eq_ignore_ascii_case("url") {
          self.skip_to_close();
          return Some(Node::Url);
        }
        if name.eq_ignore_ascii_case("var") {
          return self.var_arguments(name);
        }
        let children = self.sequence()?;
        self.expect(')')?;
        Some(Node::Function(name, children))
      }
      '/' | '*' | '+' | '-' => {
        self.pos += 1;
        Some(Node::Operator)
      }
      _ => None,
    }
  }

  /// css-tree's `var()`: the custom property name, then a comma and the
  /// fallback as one raw node.
  fn var_arguments(&mut self, name: String) -> Option<Node> {
    self.skip_space_and_comments();
    if !self.starts_ident(0) {
      return None;
    }
    let mut children = vec![Node::Identifier(self.ident_chars())];
    self.skip_space_and_comments();
    if self.peek(0) == ',' {
      self.pos += 1;
      children.push(Node::Operator);
      let mut depth = 0usize;
      while self.pos < self.chars.len() {
        match self.peek(0) {
          '(' => depth += 1,
          ')' if depth == 0 => break,
          ')' => depth -= 1,
          _ => {}
        }
        self.pos += 1;
      }
      children.push(Node::Raw);
    }
    self.expect(')')?;
    Some(Node::Function(name, children))
  }

  /// Consume `close`, or fail.
  fn expect(&mut self, close: char) -> Option<()> {
    self.skip_space_and_comments();
    (self.peek(0) == close).then(|| self.pos += 1)
  }

  /// Skip to just past the `)` that closes the current function.
  fn skip_to_close(&mut self) {
    while self.pos < self.chars.len() && self.peek(0) != ')' {
      self.pos += 1;
    }
    self.pos = (self.pos + 1).min(self.chars.len());
  }

  /// Whether the character `offset` ahead can start an identifier body.
  fn starts_ident_char(&self, offset: usize) -> bool {
    let c = self.peek(offset);
    c.is_ascii_alphabetic() || c == '_' || c == '-' || !c.is_ascii() || c == '\\'
  }

  /// CSS's "would start an identifier" at `offset` ahead.
  fn starts_ident(&self, offset: usize) -> bool {
    let c = self.peek(offset);
    if c == '-' {
      let next = self.peek(offset + 1);
      return next == '-'
        || next.is_ascii_alphabetic()
        || next == '_'
        || (!next.is_ascii() && next != '\0')
        || (next == '\\' && self.peek(offset + 2) != '\n');
    }
    c.is_ascii_alphabetic() || c == '_' || (!c.is_ascii() && c != '\0') || c == '\\'
  }

  /// CSS's "would start a number" at the cursor.
  fn starts_number(&self) -> bool {
    let digit_or_dot = |offset: usize| {
      let c = self.peek(offset);
      c.is_ascii_digit() || (c == '.' && self.peek(offset + 1).is_ascii_digit())
    };
    match self.peek(0) {
      '+' | '-' => digit_or_dot(1),
      _ => digit_or_dot(0),
    }
  }

  /// Consume a number: sign, digits, fraction and exponent.
  fn number(&mut self) {
    if matches!(self.peek(0), '+' | '-') {
      self.pos += 1;
    }
    while self.peek(0).is_ascii_digit() {
      self.pos += 1;
    }
    if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
      self.pos += 1;
      while self.peek(0).is_ascii_digit() {
        self.pos += 1;
      }
    }
    if matches!(self.peek(0), 'e' | 'E') {
      let sign = usize::from(matches!(self.peek(1), '+' | '-'));
      if self.peek(1 + sign).is_ascii_digit() {
        self.pos += 1 + sign;
        while self.peek(0).is_ascii_digit() {
          self.pos += 1;
        }
      }
    }
  }

  /// Consume identifier characters (and escapes) and return them.
  fn ident_chars(&mut self) -> String {
    let start = self.pos;
    while self.pos < self.chars.len() {
      let c = self.peek(0);
      if c == '\\' {
        self.pos = (self.pos + 2).min(self.chars.len());
      } else if c.is_ascii_alphanumeric() || c == '_' || c == '-' || !c.is_ascii() {
        self.pos += 1;
      } else {
        break;
      }
    }
    self.chars[start..self.pos].iter().collect()
  }
}
