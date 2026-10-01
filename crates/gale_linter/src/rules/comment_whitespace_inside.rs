use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext};

/// Require or disallow whitespace on the inside of comment markers.
///
/// Equivalent to Stylelint's `comment-whitespace-inside` rule, autofix
/// included, with primary option `"always"` or `"never"`.  Only `/* */`
/// comments that are statements of their own count: `//` comments and
/// comments inside selectors, values or at-rule params are not comment
/// nodes to PostCSS.  `/*! ...` and `/*# ...` (copyright and source map
/// comments) are left alone.
pub struct CommentWhitespaceInside;

/// One of the four problems the rule reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
  ExpectedOpening,
  RejectedOpening,
  ExpectedClosing,
  RejectedClosing,
}

impl Problem {
  /// Stylelint's message for the problem.
  fn message(self) -> &'static str {
    match self {
      Problem::ExpectedOpening => "Expected whitespace after \"/*\"",
      Problem::RejectedOpening => "Expected no whitespace after \"/*\"",
      Problem::ExpectedClosing => "Expected whitespace before \"*/\"",
      Problem::RejectedClosing => "Expected no whitespace before \"*/\"",
    }
  }
}

impl Rule for CommentWhitespaceInside {
  fn name(&self) -> &'static str {
    "comment-whitespace-inside"
  }

  fn description(&self) -> &'static str {
    "Require or disallow whitespace on the inside of comment markers"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks every comment node in the document, in source order.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let never = ctx.primary_option_str() == Some("never");
    let tree = PostcssTree::parse(ctx.source, ctx.syntax);
    let mut diags = Vec::new();
    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if node.kind != NodeKind::Comment || node.inline {
        continue;
      }
      let raw = tree.text(i);
      if disables_rule_from_its_line(&node.name, self.name()) {
        continue;
      }
      for (problem, start, end) in problems(raw, never) {
        let fix = Fix::new(
          "Fix the whitespace inside the comment",
          vec![Edit::new(
            Span::from_range(node.start, node.end),
            fixed_comment(raw, never),
          )],
        );
        diags.push(
          Diagnostic::new(self.name(), problem.message())
            .severity(self.default_severity())
            .span(Span::from_range(node.start + start, node.start + end))
            .fix(fix),
        );
      }
    }
    diags
  }
}

/// Whether the comment text is a `stylelint-disable` (or `gale-disable`)
/// command that turns `rule` off, with every rule or by name.
///
/// Stylelint disables a rule from the command's own line on, so it never
/// reports the command comment itself.  Gale's disabled ranges start after
/// the comment, so the rule skips it instead.  `-line` commands already
/// cover their line, and Stylelint does report `-next-line` ones.
fn disables_rule_from_its_line(text: &str, rule: &str) -> bool {
  ["stylelint-disable", "gale-disable"].iter().any(|command| {
    let Some(rest) = text.strip_prefix(command) else {
      return false;
    };
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
      return false;
    }
    let rules = rest.split("--").next().unwrap_or("").trim();
    rules.is_empty() || rules.split(',').any(|name| name.trim() == rule)
  })
}

/// The problems in the comment `raw` (`/* ... */`), each with the byte range
/// inside `raw` that Stylelint points at, in the order Stylelint reports
/// them.  None for an unclosed comment or a copyright or source map one.
fn problems(raw: &str, never: bool) -> Vec<(Problem, usize, usize)> {
  if raw.len() < 4 || !raw.starts_with("/*") || !raw.ends_with("*/") {
    return Vec::new();
  }
  // `/^\/\*[#!]\s/` on the first four characters.
  let mut first = raw.chars().skip(2);
  if matches!(first.next(), Some('#' | '!')) && first.next().is_some_and(is_js_space) {
    return Vec::new();
  }

  // `/(^\/\*+)(\s*)/`: the opener with all its stars, then whitespace.
  let opener = 1 + raw[1..].bytes().take_while(|&b| b == b'*').count();
  let left_word = js_space_prefix(&raw[opener..]);
  // `/(\s*)(\*+\/)$/`: the stars before the final `/`, then whitespace
  // before them.
  let closer_start = raw.len()
    - 1
    - raw[..raw.len() - 1]
      .bytes()
      .rev()
      .take_while(|&b| b == b'*')
      .count();
  let right_word_start = js_space_suffix_start(&raw[..closer_start]);

  let left_space = raw[opener..opener + left_word].chars().next();
  let right_space = raw[right_word_start..closer_start].chars().next();
  let mut found = Vec::new();
  if never && left_space.is_some() {
    found.push((Problem::RejectedOpening, opener, opener + left_word));
  }
  if !never && !left_space.is_some_and(is_css_space) {
    found.push((Problem::ExpectedOpening, opener, opener));
  }
  if never && right_space.is_some() {
    found.push((Problem::RejectedClosing, right_word_start, closer_start));
  }
  if !never && !right_space.is_some_and(is_css_space) {
    let before_closer = raw[..closer_start]
      .char_indices()
      .next_back()
      .map_or(0, |(at, _)| at);
    found.push((Problem::ExpectedClosing, before_closer, before_closer));
  }
  found
}

/// The comment `raw` after Stylelint's fixes for its problems: PostCSS's
/// `left`/`text`/`right` split, changed the way the rule changes them, then
/// printed back.
fn fixed_comment(raw: &str, never: bool) -> String {
  let inner = &raw[2..raw.len() - 2];
  let (mut left, mut text, mut right) = if inner.chars().all(is_js_space) {
    (inner.to_string(), String::new(), String::new())
  } else {
    let start = js_space_prefix(inner);
    let end = js_space_suffix_start(inner);
    (
      inner[..start].to_string(),
      inner[start..end].to_string(),
      inner[end..].to_string(),
    )
  };
  for (problem, _, _) in problems(raw, never) {
    match problem {
      Problem::RejectedOpening | Problem::RejectedClosing => {
        left.clear();
        right.clear();
        // `.replace(/^(\*+)(\s+)?/, '$1')`
        let stars = text.bytes().take_while(|&b| b == b'*').count();
        if stars > 0 {
          let spaces = js_space_prefix(&text[stars..]);
          text.replace_range(stars..stars + spaces, "");
        }
        // `.replace(/(\s+)?(\*+)$/, '$2')`
        let stars_start = text.len() - text.bytes().rev().take_while(|&b| b == b'*').count();
        if stars_start < text.len() {
          let spaces_start = js_space_suffix_start(&text[..stars_start]);
          text.replace_range(spaces_start..stars_start, "");
        }
      }
      Problem::ExpectedOpening => {
        if text.starts_with('*') {
          let stars = text.bytes().take_while(|&b| b == b'*').count();
          text.insert(stars, ' ');
        } else {
          left = " ".to_string();
        }
      }
      Problem::ExpectedClosing => {
        if text.ends_with('*') {
          let stars_start = text.len() - text.bytes().rev().take_while(|&b| b == b'*').count();
          text.insert(stars_start, ' ');
        } else {
          right = " ".to_string();
        }
      }
    }
  }
  format!("/*{left}{text}{right}*/")
}

/// Stylelint's `isWhitespace`: space, tab, line feed, carriage return or
/// form feed.
fn is_css_space(c: char) -> bool {
  matches!(c, ' ' | '\n' | '\t' | '\r' | '\u{c}')
}

/// JavaScript's `\s`.
fn is_js_space(c: char) -> bool {
  matches!(
    c,
    '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
      ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
  )
}

/// Byte length of the JavaScript whitespace at the start of `text`.
fn js_space_prefix(text: &str) -> usize {
  text
    .char_indices()
    .find(|&(_, c)| !is_js_space(c))
    .map_or(text.len(), |(at, _)| at)
}

/// Byte offset where the JavaScript whitespace at the end of `text` starts.
fn js_space_suffix_start(text: &str) -> usize {
  text
    .char_indices()
    .rev()
    .find(|&(_, c)| !is_js_space(c))
    .map_or(0, |(at, c)| at + c.len_utf8())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::empty_lines::fix_with;
  use gale_css_parser::Syntax;

  /// The messages for `source` in `syntax` with `option`, with their
  /// offsets.
  fn messages(source: &str, syntax: Syntax, option: &str) -> Vec<(String, usize)> {
    let options = serde_json::json!(option);
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax,
      options: Some(&options),
    };
    CommentWhitespaceInside
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| (d.message, d.span.offset))
      .collect()
  }

  #[test]
  fn always_wants_whitespace_on_both_sides() {
    assert!(messages("/* comment */", Syntax::Css, "always").is_empty());
    assert!(messages("/*! copyright */", Syntax::Css, "always").is_empty());
    assert_eq!(
      messages("/*comment*/", Syntax::Css, "always"),
      vec![
        ("Expected whitespace after \"/*\"".to_string(), 2),
        ("Expected whitespace before \"*/\"".to_string(), 8),
      ]
    );
    // The opener takes every star, the closer every star before the `/`.
    assert_eq!(
      messages("/**/", Syntax::Css, "always"),
      vec![
        ("Expected whitespace after \"/*\"".to_string(), 3),
        ("Expected whitespace before \"*/\"".to_string(), 0),
      ]
    );
  }

  #[test]
  fn never_rejects_whitespace_on_either_side() {
    assert!(messages("/*comment*/", Syntax::Css, "never").is_empty());
    assert_eq!(
      messages("/* comment\n*/", Syntax::Css, "never"),
      vec![
        ("Expected no whitespace after \"/*\"".to_string(), 2),
        ("Expected no whitespace before \"*/\"".to_string(), 10),
      ]
    );
  }

  #[test]
  fn only_comment_nodes_count() {
    // CSS comments (which the CSS parser drops) inside blocks count...
    assert_eq!(
      messages("a { color: red; /*x*/ }", Syntax::Css, "always").len(),
      2
    );
    // ...but not comments inside a selector, nor `//` comments.
    assert!(messages("a /*x*/ { }", Syntax::Css, "always").is_empty());
    assert!(messages("a, /*x*/ b { }", Syntax::Css, "always").is_empty());
    assert!(messages("//comment\na {}", Syntax::Scss, "always").is_empty());
    assert!(messages("//comment\na {}", Syntax::Less, "always").is_empty());
  }

  #[test]
  fn leaves_disable_commands_for_this_rule_alone() {
    assert!(messages("/* stylelint-disable */\na {}", Syntax::Css, "never").is_empty());
    assert!(
      messages(
        "/*stylelint-disable comment-whitespace-inside -- why*/\na {}",
        Syntax::Css,
        "always"
      )
      .is_empty()
    );
    // Other rules' disables and next-line disables are reported.
    assert_eq!(
      messages("/* stylelint-disable foo */", Syntax::Css, "never").len(),
      2
    );
    assert_eq!(
      messages(
        "/* stylelint-disable-next-line */\na {}",
        Syntax::Css,
        "never"
      )
      .len(),
      2
    );
  }

  #[test]
  fn fix_follows_stylelint() {
    let fix = |source: &str, option: &str| {
      fix_with(
        "comment-whitespace-inside",
        serde_json::json!(option),
        source,
        Syntax::Css,
      )
    };
    assert_eq!(fix("/*comment*/", "always"), "/* comment */");
    assert_eq!(fix("/**comment **/", "always"), "/** comment **/");
    assert_eq!(fix("/*!copyright */", "always"), "/* !copyright */");
    assert_eq!(fix("/**/", "always"), "/*  */");
    // A leading no-break space is PostCSS's `raws.left`, which the fix replaces.
    assert_eq!(fix("/*\u{a0}x*/", "always"), "/* x */");
    assert_eq!(
      fix("/* comment\n\ncomment*/", "never"),
      "/*comment\n\ncomment*/"
    );
    assert_eq!(fix("/* x **/", "never"), "/*x**/");
    assert_eq!(
      fix("/*comment\n *comment\n *comment\n */", "never"),
      "/*comment\n *comment\n *comment*/"
    );
  }
}
