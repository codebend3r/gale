//! postcss-selector-parser's escaping: the `cssesc` port it serialises
//! attribute values with, and its `unesc` for reading them back.

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

/// Resolve CSS escapes the way postcss-selector-parser's `unesc` does:
/// `\` and up to six hex digits (and one following space) for a code point,
/// `\` and any other character for that character.
pub(super) fn unescape(text: &str) -> String {
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

#[cfg(test)]
mod tests {
  use super::*;

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
