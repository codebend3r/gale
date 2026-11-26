//! Byte-level scanning helpers the tokenizers and parsers share.

/// Whether `b` is CSS whitespace: space, tab, line feed, carriage return or
/// form feed.  The set every tokenizer ported here (PostCSS's,
/// postcss-selector-parser's, `@csstools/css-tokenizer`'s) treats as space.
pub(crate) fn is_css_space(b: u8) -> bool {
  matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// The first offset at or after `from` holding `target` that is not
/// escaped: preceded by an even run of backslashes (none included).  This
/// is how the upstream tokenizers find a closing quote or `url(` paren.
///
/// The run is counted back from the match without stopping at `from`; the
/// callers all start just past a quote or `(`, so it never reaches further.
pub(crate) fn find_unescaped(bytes: &[u8], from: usize, target: u8) -> Option<usize> {
  let mut search = from;
  loop {
    let found = search + bytes.get(search..)?.iter().position(|&b| b == target)?;
    let backslashes = bytes[..found]
      .iter()
      .rev()
      .take_while(|&&b| b == b'\\')
      .count();
    if backslashes % 2 == 0 {
      return Some(found);
    }
    search = found + 1;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_the_first_unescaped_target() {
    assert_eq!(find_unescaped(b"'a\\'b'", 1, b'\''), Some(5));
    assert_eq!(find_unescaped(b"'a\\\\'b'", 1, b'\''), Some(4));
    assert_eq!(find_unescaped(b"(a\\)", 1, b')'), None);
    assert_eq!(find_unescaped(b"(a)", 3, b')'), None);
    assert_eq!(find_unescaped(b"(a)", 9, b')'), None);
  }

  #[test]
  fn knows_css_whitespace() {
    for b in *b" \t\n\r\x0c" {
      assert!(is_css_space(b));
    }
    assert!(!is_css_space(b'\x0b'));
    assert!(!is_css_space(b'a'));
  }
}
