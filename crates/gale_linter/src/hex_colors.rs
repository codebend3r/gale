//! Hex colors in declaration values.

/// Each `#` followed by hex digits in `value`, with its offset.
pub fn find_hex_colors(value: &str) -> Vec<(usize, String)> {
  let mut colors = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i < len {
    if bytes[i] == b'#' {
      let start = i;
      i += 1;
      while i < len && (bytes[i] as char).is_ascii_hexdigit() {
        i += 1;
      }
      if i > start + 1 {
        colors.push((start, value[start..i].to_string()));
      }
    } else {
      i += 1;
    }
  }
  colors
}

/// Whether `word` is a hex color: `#` and 3, 4, 6 or 8 hex digits.
pub fn is_valid_hex(word: &str) -> bool {
  word.strip_prefix('#').is_some_and(|digits| {
    matches!(digits.len(), 3 | 4 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit())
  })
}
