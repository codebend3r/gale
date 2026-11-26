//! Splitting a rule's selector text into its selectors, and stripping the
//! comments around and inside them, as PostCSS and Stylelint do.

/// PostCSS's `rule.selectors`: the selector split at commas outside
/// parentheses, brackets and strings, each trimmed.
pub fn rule_selectors(selector: &str) -> Vec<&str> {
  split_selectors(selector)
    .into_iter()
    .map(|(_, part)| part.trim())
    .collect()
}

/// The selectors of a list as [`rule_selectors`] splits it, untrimmed and
/// with the offset each starts at.
pub fn split_selectors(selector: &str) -> Vec<(usize, &str)> {
  let bytes = selector.as_bytes();
  let mut parts = Vec::new();
  let mut depth = 0usize;
  let mut quote: Option<u8> = None;
  let mut escaped = false;
  let mut start = 0;
  for (i, &b) in bytes.iter().enumerate() {
    if escaped {
      escaped = false;
      continue;
    }
    match (quote, b) {
      (_, b'\\') => escaped = true,
      (Some(q), _) if b == q => quote = None,
      (Some(_), _) => {}
      (None, b'"' | b'\'') => quote = Some(b),
      (None, b'(' | b'[') => depth += 1,
      (None, b')' | b']') => depth = depth.saturating_sub(1),
      (None, b',') if depth == 0 => {
        parts.push((start, &selector[start..i]));
        start = i + 1;
      }
      _ => {}
    }
  }
  parts.push((start, &selector[start..]));
  parts
}

/// `text` without its `/* ... */` comments.
pub fn strip_comments(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(open) = rest.find("/*") {
    out.push_str(&rest[..open]);
    rest = rest[open + 2..]
      .find("*/")
      .map_or("", |close| &rest[open + 2 + close + 2..]);
  }
  out.push_str(rest);
  out
}

/// `text` with each `/* ... */` comment blanked out with spaces, so that
/// offsets into the result are offsets into `text`.
pub fn blank_comments(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(open) = rest.find("/*") {
    out.push_str(&rest[..open]);
    let len = rest[open + 2..]
      .find("*/")
      .map_or(rest.len() - open, |close| close + 4);
    out.extend(std::iter::repeat_n(' ', len));
    rest = &rest[open + len..];
  }
  out.push_str(rest);
  out
}

/// Stylelint's `getStrippedSelectorSource` for one selector of a list: the
/// selector without the whitespace and comments around it, and where it
/// starts in `part`.
pub fn stripped_selector(part: &str) -> (usize, &str) {
  let mut start = 0;
  let mut end = part.len();
  loop {
    let rest = &part[start..end];
    let trimmed = rest.trim_start();
    start += rest.len() - trimmed.len();
    if let Some(comment) = trimmed.strip_prefix("/*") {
      start = comment
        .find("*/")
        .map_or(end, |close| start + 2 + close + 2);
      continue;
    }
    break;
  }
  loop {
    let trimmed = part[start..end].trim_end();
    end = start + trimmed.len();
    if let Some(comment) = trimmed.strip_suffix("*/")
      && let Some(open) = comment.rfind("/*")
    {
      end = start + open;
      continue;
    }
    break;
  }
  (start, &part[start..end])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn strips_whitespace_and_comments_around_a_selector() {
    assert_eq!(stripped_selector(" /* a */ 50% /* b */ "), (9, "50%"));
    assert_eq!(stripped_selector("from"), (0, "from"));
    assert_eq!(stripped_selector(" /* only */ "), (12, ""));
  }

  #[test]
  fn blanks_comments_in_place() {
    assert_eq!(blank_comments("a /* b */ c /* d"), "a         c     ");
  }

  #[test]
  fn splits_rule_selectors_like_postcss() {
    assert_eq!(
      rule_selectors("a, b:is(c, d) , [e=','] f"),
      vec!["a", "b:is(c, d)", "[e=','] f"]
    );
    assert_eq!(strip_comments("a /* b */ c/**/"), "a  c");
  }
}
