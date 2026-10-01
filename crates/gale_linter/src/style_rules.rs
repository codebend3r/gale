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

/// An at-rule's name and params as written in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAtRule {
  /// The name without the `@`, as written.
  pub name: String,
  /// The params with surrounding whitespace removed.
  pub params: String,
  /// Byte offset of the `@`.
  pub offset: usize,
  /// Byte offset of `params`.
  pub params_offset: usize,
  /// Whether the at-rule has a `{ ... }` block rather than ending with `;`.
  pub has_block: bool,
}

/// The style rules and at-rules of a stylesheet, as written, from one scan
/// of its source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScannedRules {
  /// Every style rule prelude, as [`scan_style_rules`] finds them.
  pub style_rules: Vec<RawStyleRule>,
  /// Every at-rule, as [`scan_at_rules`] finds them.
  pub at_rules: Vec<RawAtRule>,
}

/// Find every style rule prelude and every at-rule in `source` in one pass.
///
/// Rules get this for the file they lint from
/// [`RuleContext::scanned_rules`](crate::rule::RuleContext::scanned_rules),
/// which scans once per file for all of them.
pub fn scan(source: &str, syntax: Syntax) -> ScannedRules {
  let mut scanned = ScannedRules::default();
  scanner(source, syntax).scan_block(
    0,
    source.len(),
    None,
    false,
    &mut scanned.style_rules,
    &mut scanned.at_rules,
  );
  scanned
}

/// Find every style rule prelude in `source`, in document order.
///
/// Keyframe selectors such as `from` and `50%` are not style rules and are
/// left out, as is anything nested under them.
pub fn scan_style_rules(source: &str, syntax: Syntax) -> Vec<RawStyleRule> {
  scan(source, syntax).style_rules
}

/// Find every at-rule in `source` that has a block or ends with `;`, nested
/// ones included, in document order.  Unlike the parsed AST, this sees
/// at-rules nested in plain CSS style rules.
pub fn scan_at_rules(source: &str, syntax: Syntax) -> Vec<RawAtRule> {
  scan(source, syntax).at_rules
}

/// A scanner over `source`.
fn scanner(source: &str, syntax: Syntax) -> BlockScanner<'_> {
  BlockScanner {
    bytes: source.as_bytes(),
    source,
    // Plain CSS has no `//` line comments; every preprocessor syntax does.
    line_comments: !matches!(syntax, Syntax::Css),
  }
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
    at_rules: &mut Vec<RawAtRule>,
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
        b';' => {
          self.record_at_rule(stmt_start, at, false, at_rules);
          pos = at + 1;
        }
        b'}' => return,
        _ => {
          // An opening brace: a rule, an at-rule, or a nested property block.
          let prelude = self.source[stmt_start..at].trim_end();
          let block_start = at + 1;
          let block_end = self.find_block_end(block_start, end);
          if prelude.starts_with('@') {
            self.record_at_rule(stmt_start, at, true, at_rules);
            let nested_keyframes = in_keyframes || at_rule_name(prelude).ends_with("keyframes");
            self.scan_block(
              block_start,
              block_end,
              parent,
              nested_keyframes,
              out,
              at_rules,
            );
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
            self.scan_block(block_start, block_end, Some(index), false, out, at_rules);
          }
          pos = (block_end + 1).min(end);
        }
      }
    }
  }

  /// Record the statement in `[start, end)` if it is an at-rule.
  fn record_at_rule(
    &self,
    start: usize,
    end: usize,
    has_block: bool,
    at_rules: &mut Vec<RawAtRule>,
  ) {
    let Some(text) = self.source.get(start..end) else {
      return;
    };
    let Some(rest) = text.strip_prefix('@') else {
      return;
    };
    let name_len = rest
      .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
      .unwrap_or(rest.len());
    let after = &rest[name_len..];
    let leading = after.len() - after.trim_start().len();
    at_rules.push(RawAtRule {
      name: rest[..name_len].to_string(),
      params: after.trim().to_string(),
      offset: start,
      params_offset: start + 1 + name_len + leading,
      has_block,
    });
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
        // An escaped character, such as the quote in `[a=b\'c]`, is not
        // syntax.
        b'\\' => {
          pos += 2;
          continue;
        }
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
        // An escaped character, such as the quote in `[a=b\'c]`, is not
        // syntax.
        b'\\' => {
          pos += 2;
          continue;
        }
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
  fn escaped_quotes_do_not_open_a_string() {
    assert_eq!(
      preludes("[href=te\\'s\\\"t] { } b {}"),
      vec![
        ("[href=te\\'s\\\"t]".into(), 0, None),
        ("b".into(), 20, None)
      ]
    );
  }

  #[test]
  fn finds_at_rules_nested_anywhere() {
    let found: Vec<(String, String, usize, usize, bool)> = scan_at_rules(
      "@import 'a';\na { @media  (x) { b {} } }\n@MEDIA print{}",
      Syntax::Css,
    )
    .into_iter()
    .map(|r| (r.name, r.params, r.offset, r.params_offset, r.has_block))
    .collect();
    assert_eq!(
      found,
      vec![
        ("import".into(), "'a'".into(), 0, 8, false),
        ("media".into(), "(x)".into(), 17, 25, true),
        ("MEDIA".into(), "print".into(), 40, 47, true),
      ]
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
