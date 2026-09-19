//! Style rule preludes recovered from the source text.
//!
//! Gale's CSS parser drops rules whose selectors it cannot parse and
//! re-serialises the ones it keeps. Rules that must see selectors exactly as
//! the author wrote them, including invalid ones, read them from here rather
//! than from the parsed AST.

use gale_css_parser::Syntax;

/// A style rule's prelude as written in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawStyleRule {
  /// The prelude text with surrounding whitespace removed.
  pub prelude: String,
  /// Byte offset of `prelude` in the source.
  pub offset: usize,
  /// Index into the returned list of the enclosing style rule, if nested.
  /// A parent always precedes its children in the list.
  pub parent: Option<usize>,
}

/// Find every style rule prelude in `source`, in document order.
///
/// Keyframe selectors such as `from` and `50%` are not style rules and are
/// left out, as is anything nested under them.
pub fn scan_style_rules(source: &str, syntax: Syntax) -> Vec<RawStyleRule> {
  let scanner = BlockScanner {
    bytes: source.as_bytes(),
    source,
    // Plain CSS has no `//` line comments; every preprocessor syntax does.
    line_comments: !matches!(syntax, Syntax::Css),
  };
  let mut out = Vec::new();
  scanner.scan_block(0, source.len(), None, false, &mut out);
  out
}

/// Walks the byte stream of a stylesheet, tracking braces, strings and
/// comments, to recover rule preludes exactly as written.
struct BlockScanner<'a> {
  bytes: &'a [u8],
  source: &'a str,
  line_comments: bool,
}

impl BlockScanner<'_> {
  /// Scan the statements in `[start, end)`, a block body or the top level.
  fn scan_block(
    &self,
    start: usize,
    end: usize,
    parent: Option<usize>,
    in_keyframes: bool,
    out: &mut Vec<RawStyleRule>,
  ) {
    let mut pos = start;
    loop {
      pos = self.skip_trivia(pos, end);
      if pos >= end || self.bytes[pos] == b'}' {
        return;
      }
      let stmt_start = pos;
      let Some((terminator, at)) = self.find_statement_end(pos, end) else {
        return;
      };
      match terminator {
        b';' => pos = at + 1,
        b'}' => return,
        _ => {
          // An opening brace: a rule, an at-rule, or a nested property block.
          let prelude = self.source[stmt_start..at].trim_end();
          let block_start = at + 1;
          let block_end = self.find_block_end(block_start, end);
          if prelude.starts_with('@') {
            let nested_keyframes = in_keyframes || at_rule_name(prelude).ends_with("keyframes");
            self.scan_block(block_start, block_end, parent, nested_keyframes, out);
          } else if prelude.is_empty() || prelude.ends_with(':') {
            // A Sass nested-property block; its body holds declarations.
          } else if in_keyframes {
            // A keyframe selector; nothing under it is a style rule either.
          } else {
            let index = out.len();
            out.push(RawStyleRule {
              prelude: prelude.to_string(),
              offset: stmt_start,
              parent,
            });
            self.scan_block(block_start, block_end, Some(index), false, out);
          }
          pos = (block_end + 1).min(end);
        }
      }
    }
  }

  /// Skip whitespace and comments starting at `pos`.
  fn skip_trivia(&self, mut pos: usize, end: usize) -> usize {
    while pos < end {
      let b = self.bytes[pos];
      if b.is_ascii_whitespace() {
        pos += 1;
      } else if b == b'/' && pos + 1 < end && self.bytes[pos + 1] == b'*' {
        pos = self.skip_block_comment(pos, end);
      } else if self.line_comments && b == b'/' && pos + 1 < end && self.bytes[pos + 1] == b'/' {
        pos = self.skip_line_comment(pos, end);
      } else {
        break;
      }
    }
    pos
  }

  /// Position just past a `/* ... */` comment beginning at `pos`.
  fn skip_block_comment(&self, pos: usize, end: usize) -> usize {
    let mut i = pos + 2;
    while i + 1 < end {
      if self.bytes[i] == b'*' && self.bytes[i + 1] == b'/' {
        return i + 2;
      }
      i += 1;
    }
    end
  }

  /// Position just past a `//` comment beginning at `pos`.
  fn skip_line_comment(&self, pos: usize, end: usize) -> usize {
    let mut i = pos;
    while i < end && self.bytes[i] != b'\n' {
      i += 1;
    }
    i
  }

  /// Position just past a quoted string beginning at `pos`.
  fn skip_string(&self, pos: usize, end: usize) -> usize {
    let quote = self.bytes[pos];
    let mut i = pos + 1;
    while i < end {
      match self.bytes[i] {
        b'\\' => i += 2,
        b if b == quote => return i + 1,
        b'\n' => return i,
        _ => i += 1,
      }
    }
    end
  }

  /// Position just past an interpolation (`#{...}` / `@{...}`) beginning at
  /// `pos`, balancing any nested braces.
  fn skip_interpolation(&self, pos: usize, end: usize) -> usize {
    let mut depth = 0usize;
    let mut i = pos;
    while i < end {
      match self.bytes[i] {
        b'{' => depth += 1,
        b'}' => {
          depth -= 1;
          if depth == 0 {
            return i + 1;
          }
        }
        b'"' | b'\'' => {
          i = self.skip_string(i, end);
          continue;
        }
        _ => {}
      }
      i += 1;
    }
    end
  }

  /// Find the byte that terminates the statement starting at `pos`: one of
  /// `;`, `{` or `}` outside strings, comments, parentheses and
  /// interpolation. Returns the terminator and its position.
  fn find_statement_end(&self, mut pos: usize, end: usize) -> Option<(u8, usize)> {
    let mut paren_depth = 0usize;
    while pos < end {
      let b = self.bytes[pos];
      let next = if pos + 1 < end {
        self.bytes[pos + 1]
      } else {
        0
      };
      match b {
        b'"' | b'\'' => {
          pos = self.skip_string(pos, end);
          continue;
        }
        b'/' if next == b'*' => {
          pos = self.skip_block_comment(pos, end);
          continue;
        }
        b'/' if self.line_comments && next == b'/' && paren_depth == 0 => {
          pos = self.skip_line_comment(pos, end);
          continue;
        }
        b'#' | b'@' if next == b'{' => {
          pos = self.skip_interpolation(pos + 1, end);
          continue;
        }
        b'(' => paren_depth += 1,
        b')' => paren_depth = paren_depth.saturating_sub(1),
        b';' | b'{' | b'}' => return Some((b, pos)),
        _ => {}
      }
      pos += 1;
    }
    None
  }

  /// Find the `}` closing the block whose body starts at `pos`.
  fn find_block_end(&self, mut pos: usize, end: usize) -> usize {
    let mut depth = 0usize;
    while pos < end {
      let b = self.bytes[pos];
      let next = if pos + 1 < end {
        self.bytes[pos + 1]
      } else {
        0
      };
      match b {
        b'"' | b'\'' => {
          pos = self.skip_string(pos, end);
          continue;
        }
        b'/' if next == b'*' => {
          pos = self.skip_block_comment(pos, end);
          continue;
        }
        b'/' if self.line_comments && next == b'/' => {
          pos = self.skip_line_comment(pos, end);
          continue;
        }
        b'{' => depth += 1,
        b'}' => {
          if depth == 0 {
            return pos;
          }
          depth -= 1;
        }
        _ => {}
      }
      pos += 1;
    }
    end
  }
}

/// The lower-cased name of an at-rule prelude such as `@media (x)`.
fn at_rule_name(prelude: &str) -> String {
  prelude[1..]
    .chars()
    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
    .collect::<String>()
    .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn preludes(src: &str) -> Vec<(String, usize, Option<usize>)> {
    scan_style_rules(src, Syntax::Css)
      .into_iter()
      .map(|r| (r.prelude, r.offset, r.parent))
      .collect()
  }

  #[test]
  fn finds_a_top_level_rule() {
    assert_eq!(preludes("a {}"), vec![("a".into(), 0, None)]);
  }

  #[test]
  fn keeps_invalid_selectors_verbatim() {
    assert_eq!(preludes("a ) b {}"), vec![("a ) b".into(), 0, None)]);
    assert_eq!(
      preludes(":not(::before) {}"),
      vec![(":not(::before)".into(), 0, None)]
    );
  }

  #[test]
  fn finds_nested_rules_with_their_parent() {
    assert_eq!(
      preludes("a { color: red; &:hover {} }"),
      vec![("a".into(), 0, None), ("&:hover".into(), 16, Some(0))]
    );
  }

  #[test]
  fn skips_declarations_but_not_rules_after_them() {
    assert_eq!(preludes("a { color: red }"), vec![("a".into(), 0, None)]);
    assert_eq!(
      preludes("a { color: red; .b {} }"),
      vec![("a".into(), 0, None), (".b".into(), 16, Some(0))]
    );
  }

  #[test]
  fn skips_keyframe_selectors() {
    assert_eq!(preludes("@keyframes foo { from {} 50% {} to {} }"), vec![]);
    assert_eq!(
      preludes("a { @keyframes foo { from {} } .b {} }"),
      vec![("a".into(), 0, None), (".b".into(), 31, Some(0))]
    );
  }

  #[test]
  fn descends_into_other_at_rules_without_a_parent_selector() {
    assert_eq!(
      preludes("@media (x) { a {} }"),
      vec![("a".into(), 13, None)]
    );
  }

  #[test]
  fn ignores_braces_inside_strings() {
    assert_eq!(
      preludes("a[title=\"{\"] {}"),
      vec![("a[title=\"{\"]".into(), 0, None)]
    );
  }

  #[test]
  fn ignores_comments_before_a_prelude() {
    assert_eq!(preludes("/* x */ a {}"), vec![("a".into(), 8, None)]);
  }

  #[test]
  fn keeps_comments_inside_a_prelude() {
    assert_eq!(
      preludes("label/* foo */:enabled {}"),
      vec![("label/* foo */:enabled".into(), 0, None)]
    );
  }

  #[test]
  fn treats_double_slash_as_a_comment_only_for_preprocessors() {
    let scss: Vec<_> = scan_style_rules("// x {\na {}", Syntax::Scss)
      .into_iter()
      .map(|r| (r.prelude, r.offset))
      .collect();
    assert_eq!(scss, vec![("a".to_string(), 7)]);
    assert_eq!(
      preludes("// x {\na {}"),
      vec![("// x".into(), 0, None), ("a".into(), 7, Some(0))]
    );
  }

  #[test]
  fn balances_interpolation_braces() {
    assert_eq!(preludes("#{$foo} {}"), vec![("#{$foo}".into(), 0, None)]);
    assert_eq!(
      preludes("a { #{&}::before {} }"),
      vec![("a".into(), 0, None), ("#{&}::before".into(), 4, Some(0))]
    );
  }

  #[test]
  fn keeps_multi_line_preludes() {
    assert_eq!(preludes("a,\nb {}"), vec![("a,\nb".into(), 0, None)]);
  }

  #[test]
  fn skips_nested_property_declarations() {
    assert_eq!(
      preludes("a { font: { family: x } }"),
      vec![("a".into(), 0, None)]
    );
  }

  #[test]
  fn nests_through_at_rules_inside_rules() {
    assert_eq!(
      preludes("a { @media (x) { .b {} } }"),
      vec![("a".into(), 0, None), (".b".into(), 17, Some(0))]
    );
  }
}
