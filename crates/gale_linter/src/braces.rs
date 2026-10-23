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
