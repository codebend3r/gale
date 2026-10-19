//! Character classes the source-scanning rules share.
//!
//! An identifier here is what most of gale's scanners read as one: ASCII
//! letters, digits, `-` and `_`, and any non-ASCII character.  The CSS
//! tokenizer and the selector parser follow their specifications' finer
//! classes instead.

/// Whether `c` can start an identifier (after any leading `-`).
pub fn is_ident_start(c: char) -> bool {
  c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

/// Whether `c` can continue an identifier.
pub fn is_ident_char(c: char) -> bool {
  is_ident_start(c) || c.is_ascii_digit() || c == '-'
}

/// [`is_ident_start`] for one byte of UTF-8 text: every byte of a non-ASCII
/// character counts.
pub fn is_ident_start_byte(b: u8) -> bool {
  b.is_ascii_alphabetic() || b == b'_' || !b.is_ascii()
}

/// [`is_ident_char`] for one byte of UTF-8 text.
pub fn is_ident_byte(b: u8) -> bool {
  is_ident_start_byte(b) || b.is_ascii_digit() || b == b'-'
}

/// Whether `b` is a JavaScript regex word character (`\w`).
pub fn is_word_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'_'
}

/// Whether `b` matches `[\w-]`, the class upstream regexes read custom
/// property names with.
pub fn is_word_or_hyphen_byte(b: u8) -> bool {
  is_word_byte(b) || b == b'-'
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn identifiers_take_non_ascii_but_words_do_not() {
    assert!(is_ident_char('é') && is_ident_byte("é".as_bytes()[1]));
    assert!(!is_ident_start('-') && is_ident_char('-'));
    assert!(!is_ident_start('1') && is_ident_char('1'));
    assert!(!is_word_or_hyphen_byte("é".as_bytes()[0]));
    assert!(is_word_or_hyphen_byte(b'-') && !is_word_byte(b'-'));
  }
}
