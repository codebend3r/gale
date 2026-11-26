//! Finding the braces of a block in source text, for the rules that scan
//! the source rather than the parsed tree, and checking that parentheses
//! pair up.

/// Whether the block whose closing brace is at `closing_brace_pos` opened on
/// the same line: no newline between the brace that opens it and this one.
pub fn is_block_single_line(source: &str, closing_brace_pos: usize) -> bool {
  let bytes = source.as_bytes();
  let mut depth = 1;
  let mut j = closing_brace_pos;
  while j > 0 {
    j -= 1;
    if bytes[j] == b'}' {
      depth += 1;
    } else if bytes[j] == b'{' {
      depth -= 1;
      if depth == 0 {
        let segment = &source[j..closing_brace_pos];
        return !segment.contains('\n');
      }
    }
  }
  true
}

/// Offset of the `}` closing the block that opens at the first `{` at or
/// after `start`, reading SCSS: `#{...}` interpolation, strings and
/// comments do not count.
pub fn scss_block_close(source: &str, start: usize) -> Option<usize> {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = start;

  // Find opening brace (skip `#{` interpolation openers)
  loop {
    if i >= len {
      return None;
    }
    if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
      i += 2;
      skip_interpolation(bytes, len, &mut i);
      continue;
    }
    if bytes[i] == b'{' {
      break;
    }
    i += 1;
  }

  let mut depth = 1;
  i += 1;
  while i < len && depth > 0 {
    if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
      i += 2;
      skip_interpolation(bytes, len, &mut i);
      continue;
    }
    match bytes[i] {
      b'{' => depth += 1,
      b'}' => {
        depth -= 1;
        if depth == 0 {
          return Some(i);
        }
      }
      b'\'' | b'"' => {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      b'/' if i + 1 < len => {
        if bytes[i + 1] == b'/' {
          i += 2;
          while i < len && bytes[i] != b'\n' {
            i += 1;
          }
        } else if bytes[i + 1] == b'*' {
          i += 2;
          while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
            i += 1;
          }
          if i + 1 < len {
            i += 1;
          }
        }
      }
      _ => {}
    }
    i += 1;
  }

  None
}

/// Offset of the `}` closing the block that opens at the first `{` at or
/// after `start`, counting every brace: one inside an interpolation, a
/// string or a comment counts too.  The `scss/at-else-*` rules have always
/// read blocks this way, unlike [`scss_block_close`].
pub fn counted_block_close(source: &str, start: usize) -> Option<usize> {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = start;

  while i < len && bytes[i] != b'{' {
    i += 1;
  }
  if i >= len {
    return None;
  }

  let mut depth = 1;
  i += 1;
  while i < len && depth > 0 {
    match bytes[i] {
      b'{' => depth += 1,
      b'}' => depth -= 1,
      _ => {}
    }
    if depth > 0 {
      i += 1;
    }
  }

  if depth == 0 { Some(i) } else { None }
}

/// The head of one link in an SCSS `@if` / `@else` chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainHead {
  /// `@if`.
  If,
  /// `@else if`.
  ElseIf,
  /// A plain `@else`.
  Else,
}

/// How a family of chain rules reads the source.  The two families have
/// always differed, and each keeps its own reading so their reports stay
/// the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainReading {
  /// The `scss/at-if-*` rules: a head is `@if` or `@else if` followed by a
  /// single space or `(`, and blocks close by [`scss_block_close`].
  IfRules,
  /// The `scss/at-else-*` rules: a head is any `@else`, an `@else if` when
  /// spaces or tabs and then `if ` follow, and blocks close by
  /// [`counted_block_close`].
  ElseRules,
}

impl ChainReading {
  /// The chain head that starts at `at`, an `@`, if there is one.
  fn head_at(self, source: &str, at: usize) -> Option<ChainHead> {
    let rest = &source[at..];
    match self {
      Self::IfRules => {
        if rest.starts_with("@if ") || rest.starts_with("@if(") {
          Some(ChainHead::If)
        } else if rest.starts_with("@else if ") || rest.starts_with("@else if(") {
          Some(ChainHead::ElseIf)
        } else {
          None
        }
      }
      Self::ElseRules => {
        let after_else = rest.strip_prefix("@else")?;
        let condition = after_else.trim_start_matches([' ', '\t']);
        if condition.starts_with("if ") {
          Some(ChainHead::ElseIf)
        } else {
          Some(ChainHead::Else)
        }
      }
    }
  }

  /// The `}` closing the block of the head at `at`.
  fn block_close(self, source: &str, at: usize) -> Option<usize> {
    match self {
      Self::IfRules => scss_block_close(source, at),
      Self::ElseRules => counted_block_close(source, at),
    }
  }
}

/// The closing brace of one chain link and what follows it on its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainClose {
  /// Offset of the `}`.
  pub close: usize,
  /// How many spaces (not tabs) directly follow the `}`.
  pub spaces: usize,
  /// Whether `@else` comes right after those spaces.
  pub followed_by_else: bool,
  /// Whether `@else` comes after the spaces and tabs that follow the `}`.
  pub else_after_blanks: bool,
  /// Whether the spaces and tabs that follow the `}` run to a line break or
  /// the end of the source.
  pub line_ends_after: bool,
}

/// The closing braces of the chain links in `source` whose head is one of
/// `heads`, read as `reading` reads them.  The scan resumes after each
/// closing brace it reports, so a chain nested in a reported block is not
/// seen, as the rules have always behaved.
pub fn scss_chain_closes(
  source: &str,
  reading: ChainReading,
  heads: &[ChainHead],
) -> Vec<ChainClose> {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut closes = Vec::new();
  let mut i = 0;
  while i < len {
    let close = (bytes[i] == b'@')
      .then(|| reading.head_at(source, i))
      .flatten()
      .filter(|head| heads.contains(head))
      .and_then(|_| reading.block_close(source, i));
    let Some(close) = close else {
      i += 1;
      continue;
    };
    let after = close + 1;
    let spaces = bytes[after..].iter().take_while(|&&b| b == b' ').count();
    let blanks = bytes[after..]
      .iter()
      .take_while(|&&b| b == b' ' || b == b'\t')
      .count();
    closes.push(ChainClose {
      close,
      spaces,
      followed_by_else: source[after + spaces..].starts_with("@else"),
      else_after_blanks: source[after + blanks..].starts_with("@else"),
      line_ends_after: matches!(bytes.get(after + blanks), None | Some(b'\n' | b'\r')),
    });
    i = after;
  }
  closes
}

/// Advances `i` past a `#{...}` block, tracking nested braces.
fn skip_interpolation(bytes: &[u8], len: usize, i: &mut usize) {
  let mut depth = 1;
  while *i < len && depth > 0 {
    match bytes[*i] {
      b'{' => depth += 1,
      b'}' => depth -= 1,
      _ => {}
    }
    *i += 1;
  }
}

/// Whether every `)` in `params` closes an earlier `(` and every `(` is
/// closed.
pub fn parens_balance(params: &str) -> bool {
  let mut depth: i32 = 0;
  for ch in params.chars() {
    match ch {
      '(' => depth += 1,
      ')' => {
        depth -= 1;
        if depth < 0 {
          return false;
        }
      }
      _ => {}
    }
  }
  depth == 0
}
