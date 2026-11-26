//! The strings stylelint-scss's `calculationOperatorSpaceChecker` searches
//! for Sass operators, and the operators [`find_operators`] finds in them.
//!
//! `scss/operator-no-unspaced`, `scss/operator-no-newline-after` and
//! `scss/operator-no-newline-before` all run that checker, so they agree on
//! where operators are and differ only in what they report about each one.

use std::sync::LazyLock;

use gale_css_parser::postcss_tree::{Node, NodeKind, PostcssTree};
use gale_diagnostics::Span;
use regex::Regex;

use super::{Operator, find_operators};

/// One operator, with the string it was found in.
#[derive(Debug, Clone, Copy)]
pub struct OperatorSite<'t> {
  /// The string upstream hands its checker: a declaration value, at-rule
  /// params, a media feature value, or one `#{...}` interpolation.
  pub string: &'t str,
  /// Offset of [`Self::string`] in the source.
  pub offset: usize,
  /// Where the operator is in [`Self::string`].
  pub operator: Operator,
}

impl<'t> OperatorSite<'t> {
  /// The operator as written: `+`, `==` and so on.
  pub fn symbol(&self) -> &'t str {
    &self.string[self.operator.start..self.operator.end]
  }

  /// Where the operator is in the source.
  pub fn span(&self) -> Span {
    Span::new(
      self.offset + self.operator.start,
      self.operator.end - self.operator.start,
    )
  }

  /// The bytes of [`Self::string`] before the operator.
  pub fn before(&self) -> &'t [u8] {
    &self.string.as_bytes()[..self.operator.start]
  }

  /// The bytes of [`Self::string`] after the operator.
  pub fn after(&self) -> &'t [u8] {
    &self.string.as_bytes()[self.operator.end..]
  }
}

/// Upstream's `isWhitespace`.
pub fn is_whitespace(byte: u8) -> bool {
  matches!(byte, b' ' | b'\n' | b'\t' | b'\r' | b'\x0c')
}

/// Every operator `calculationOperatorSpaceChecker` hands its checker, in
/// the order it finds them: node by node, then in comments.
pub fn operator_sites<'t>(tree: &'t PostcssTree) -> Vec<OperatorSite<'t>> {
  tree
    .nodes
    .iter()
    .flat_map(|node| node_strings(tree, node))
    .chain(comment_interpolations(tree.source()))
    .flat_map(|scanned| {
      find_operators(scanned.text, scanned.is_after_colon)
        .into_iter()
        .map(move |operator| OperatorSite {
          string: scanned.text,
          offset: scanned.offset,
          operator,
        })
    })
    .collect()
}

/// A string to search for operators.
#[derive(Clone, Copy)]
struct Scanned<'t> {
  text: &'t str,
  /// Offset of `text` in the source.
  offset: usize,
  /// `findOperators`'s `isAfterColon`.
  is_after_colon: bool,
}

/// The at-rules upstream leaves alone: Sass module loading, `@at-root`,
/// `@scope` and Tailwind's directives.
const SKIPPED_AT_RULES: &[&str] = &[
  "forward",
  "use",
  "at-root",
  "scope",
  "apply",
  "custom-variant",
  "reference",
  "source",
  "theme",
  "utility",
  "variant",
];

/// A declaration holding a `data:` URI is skipped whole.
static DATA_URI: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"url\(\s*['"]?data:"#).expect("valid regex"));

/// Upstream's `/#\{(.*?)\}/g`; JavaScript's `.` stops at any line terminator,
/// so an interpolation found this way never spans a line.
static INTERPOLATION: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"#\{[^\n\r\u{2028}\u{2029}]*?\}").expect("valid regex"));

/// The strings searched in one node.
fn node_strings<'t>(tree: &'t PostcssTree, node: &'t Node) -> Vec<Scanned<'t>> {
  match node.kind {
    NodeKind::Decl => {
      if node.name == "unicode-range" || DATA_URI.is_match(&node.value) {
        return vec![];
      }
      let value = tree.declaration_value_index(node).map(|index| Scanned {
        text: &node.value,
        offset: node.start + index,
        // Sass variable values are where `+` and `/` most often do math.
        is_after_colon: node.name.starts_with('$'),
      });
      value
        .into_iter()
        .chain(interpolations(&node.name, node.start))
        .collect()
    }
    NodeKind::Rule => interpolations(&node.name, node.start).collect(),
    NodeKind::AtRule => at_rule_strings(tree, node),
    NodeKind::Comment => vec![],
  }
}

/// The strings searched in an at-rule's params.
fn at_rule_strings<'t>(tree: &'t PostcssTree, node: &'t Node) -> Vec<Scanned<'t>> {
  if SKIPPED_AT_RULES.contains(&node.name.as_str()) {
    return vec![];
  }
  let params_start = node.start + tree.at_rule_param_index(node);
  if node.name != "media" && node.name != "import" {
    return vec![Scanned {
      text: &node.params,
      offset: params_start,
      is_after_colon: true,
    }];
  }
  // `postcss-media-query-parser` splits the params into nodes: values and
  // `url()` arguments are searched as SassScript, and the rest (media
  // types, keywords, feature names) only for interpolation.
  let parts = media_query_parts(&node.params);
  let in_part = |offset: usize| {
    parts
      .iter()
      .any(|part| (part.offset..part.offset + part.text.len()).contains(&offset))
  };
  let others: Vec<_> = interpolations(&node.params, params_start)
    .filter(|scanned| !in_part(scanned.offset - params_start))
    .collect();
  parts
    .iter()
    .filter(|part| part.is_sass_script)
    .map(|part| Scanned {
      text: part.text,
      offset: params_start + part.offset,
      is_after_colon: true,
    })
    .chain(others)
    .collect()
}

/// Each `#{...}` in `text`, which starts at `offset` in the source.
fn interpolations(text: &str, offset: usize) -> impl Iterator<Item = Scanned<'_>> {
  INTERPOLATION.find_iter(text).map(move |found| Scanned {
    text: found.as_str(),
    offset: offset + found.start(),
    is_after_colon: false,
  })
}

/// A media feature value or `url()` argument in `@media` or `@import`
/// params.
struct MediaPart<'p> {
  /// Offset in the params.
  offset: usize,
  text: &'p str,
  /// Whether upstream searches it for operators: every feature value, and
  /// a `url()` argument only when it is quoted or holds whitespace (Sass
  /// parses nothing else in `url()` as SassScript).
  is_sass_script: bool,
}

/// The feature values and `url()` arguments in `@media` or `@import`
/// params, which upstream gets from `postcss-media-query-parser`.
fn media_query_parts(params: &str) -> Vec<MediaPart<'_>> {
  let bytes = params.as_bytes();
  let mut parts = Vec::new();
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => i = skip_string(bytes, i, quote),
      b'(' => {
        let close = matching_paren(bytes, i);
        let inner = &params[i + 1..close];
        if params[..i].to_ascii_lowercase().ends_with("url") {
          let trimmed = inner.trim();
          parts.push(MediaPart {
            offset: i + 1 + inner.find(trimmed).unwrap_or(0),
            text: trimmed,
            is_sass_script: trimmed.starts_with(['"', '\''])
              || trimmed.contains(char::is_whitespace),
          });
        } else if let Some(colon) = feature_colon(inner) {
          let rest = &inner[colon + 1..];
          let leading = rest.len() - rest.trim_start().len();
          parts.push(MediaPart {
            offset: i + 1 + colon + 1 + leading,
            text: rest.trim(),
            is_sass_script: true,
          });
        }
        i = close + 1;
      }
      _ => i += 1,
    }
  }
  parts
}

/// Offset just past the string opened by `quote` at `start`.
fn skip_string(bytes: &[u8], start: usize, quote: u8) -> usize {
  let mut i = start + 1;
  while i < bytes.len() && bytes[i] != quote {
    if bytes[i] == b'\\' {
      i += 1;
    }
    i += 1;
  }
  (i + 1).min(bytes.len())
}

/// Offset of the `)` closing the `(` at `open`, or the end of the text.
fn matching_paren(bytes: &[u8], open: usize) -> usize {
  let mut depth = 0usize;
  let mut i = open;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => {
        i = skip_string(bytes, i, quote);
        continue;
      }
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 {
          return i;
        }
      }
      _ => {}
    }
    i += 1;
  }
  bytes.len()
}

/// Offset of the `:` separating a media feature from its value: the first
/// one outside strings and `{...}`.
fn feature_colon(feature: &str) -> Option<usize> {
  let bytes = feature.as_bytes();
  let mut braces = 0usize;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => {
        i = skip_string(bytes, i, quote);
        continue;
      }
      b'{' => braces += 1,
      b'}' => braces = braces.saturating_sub(1),
      b':' if braces == 0 => return Some(i),
      _ => {}
    }
    i += 1;
  }
  None
}

/// The interpolation in each `/* ... */` comment of the source, found the
/// way upstream's `findCommentsInRaws` finds comments (PostCSS drops those
/// inside selectors and values), so a comment inside a string, a `//`
/// comment or an unquoted `url(//...)` does not count.
///
/// Upstream shifts each comment's offset by two for every `//` comment
/// before it (it compensates for the `*/` postcss-scss appends, though it
/// reads the untransformed source); gale reports at the operator instead.
fn comment_interpolations(source: &str) -> Vec<Scanned<'_>> {
  css_comment_texts(source)
    .into_iter()
    .flat_map(|(offset, text)| interpolations(text, offset).collect::<Vec<_>>())
    .collect()
}

/// Where `findCommentsInRaws` is: inside a string (with its quote), a
/// comment, `url(...)`, or other parentheses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
  String(u8),
  BlockComment,
  LineComment,
  Url,
  Parens,
}

/// The text of each `/* ... */` comment, after the opening `/*` (and any
/// extra `*`, `!` or `#`) and the whitespace after it, with its offset.
fn css_comment_texts(source: &str) -> Vec<(usize, &str)> {
  let bytes = source.as_bytes();
  let mut modes: Vec<Mode> = Vec::new();
  let mut comment_start = 0;
  let mut texts = Vec::new();
  let mut i = 0;
  while i < bytes.len() {
    let mode = modes.last().copied();
    let is_comment = matches!(mode, Some(Mode::BlockComment | Mode::LineComment));
    let is_string = matches!(mode, Some(Mode::String(_)));
    match bytes[i] {
      quote @ (b'"' | b'\'') if !is_comment => {
        if mode == Some(Mode::String(quote)) && (i == 0 || bytes[i - 1] != b'\\') {
          modes.pop();
        } else {
          modes.push(Mode::String(quote));
        }
      }
      b'(' if !is_comment && !is_string => {
        modes.push(if function_name_before(&source[..i]) == Some("url") {
          Mode::Url
        } else {
          Mode::Parens
        });
      }
      b')' if !is_comment && !is_string => {
        modes.pop();
      }
      b'/' if !is_comment && !is_string => match bytes.get(i + 1) {
        Some(b'*') => {
          modes.push(Mode::BlockComment);
          comment_start = i;
          i += 1;
        }
        Some(b'/') if mode != Some(Mode::Url) => {
          modes.push(Mode::LineComment);
          i += 1;
        }
        _ => {}
      },
      b'*' if mode == Some(Mode::BlockComment) && bytes.get(i + 1) == Some(&b'/') => {
        modes.pop();
        texts.push(block_comment_text(source, comment_start, i + 2));
        i += 1;
      }
      b'\n' if mode == Some(Mode::LineComment) => {
        modes.pop();
      }
      _ => {}
    }
    i += 1;
  }
  texts
}

/// The name of the function whose `(` ends `before`, if a name ends it
/// after the start, a line break, or one of the characters upstream's
/// `/(?:^|[\n\r]|\s-|[:\s,.(){}*+/%])([\w-]*)$/` allows.
fn function_name_before(before: &str) -> Option<&str> {
  let name_start = before
    .bytes()
    .rposition(|b| !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .map_or(0, |i| i + 1);
  let name = &before[name_start..];
  let is_delimited = name_start == 0
    || before.as_bytes()[name_start - 1].is_ascii_whitespace()
    || b":,.(){}*+/%".contains(&before.as_bytes()[name_start - 1]);
  is_delimited.then_some(name)
}

/// The text of the comment `source[start..end]` without its `/*` (and
/// extra `*`, `!` or `#`), the whitespace after it, and its closing `*/`.
fn block_comment_text(source: &str, start: usize, end: usize) -> (usize, &str) {
  let raw = &source[start..end];
  let stars = raw[1..].bytes().take_while(|&b| b == b'*').count();
  let mut token = 1 + stars;
  if matches!(raw.as_bytes().get(token), Some(b'!' | b'#')) {
    token += 1;
  }
  let body_end = (raw.len() - 2).max(token);
  let body = &raw[token.min(body_end)..body_end];
  let left = body.len() - body.trim_start().len();
  (start + token.min(body_end) + left, &body[left..])
}

#[cfg(test)]
mod tests {
  use super::css_comment_texts;

  fn texts(source: &str) -> Vec<&str> {
    css_comment_texts(source)
      .into_iter()
      .map(|(_, text)| text)
      .collect()
  }

  #[test]
  fn finds_block_comment_text_after_the_opening_token() {
    let source = "a { /** #{1+1} */ }";
    let found = css_comment_texts(source);
    assert_eq!(found, [(8, "#{1+1} ")]);
    assert_eq!(&source[8..14], "#{1+1}");
  }

  #[test]
  fn skips_comment_markers_in_strings_line_comments_and_urls() {
    assert_eq!(texts("a { content: '/* x */'; }"), Vec::<&str>::new());
    assert_eq!(texts("// a /* x */\n/* y */"), ["y "]);
    // Upstream keeps only `//` from starting a comment inside `url()`.
    assert_eq!(texts("a { b: url(//x/*y*/); } /* z */"), ["y", "z "]);
  }
}
