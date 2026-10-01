use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext};

/// Specify keyword or percentage notation for keyframe selectors.
///
/// Equivalent to Stylelint's `keyframe-selector-notation` rule, including
/// its autofix.  Primary options:
///
/// - `"keyword"`: `0%` and `100%` become `from` and `to`;
/// - `"percentage"`: `from` and `to` become `0%` and `100%`;
/// - `"percentage-unless-within-keyword-only-block"`: as `"percentage"`,
///   except in a `@keyframes` block whose selectors are all keywords.
pub struct KeyframeSelectorNotation;

/// The notation the rule enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notation {
  Keyword,
  Percentage,
  PercentageUnlessKeywordOnly,
}

impl Rule for KeyframeSelectorNotation {
  fn name(&self) -> &'static str {
    "keyframe-selector-notation"
  }

  fn description(&self) -> &'static str {
    "Specify keyword or percentage notation for keyframe selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports keyframe selectors in the other notation, with a fix that
  /// swaps `from`/`to` and `0%`/`100%`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let notation = match ctx.primary_option_str() {
      Some("keyword") => Notation::Keyword,
      Some("percentage") => Notation::Percentage,
      Some("percentage-unless-within-keyword-only-block") => Notation::PercentageUnlessKeywordOnly,
      _ => return Vec::new(),
    };
    let tree = PostcssTree::parse(ctx.source, ctx.syntax);
    let mut diags = Vec::new();

    for (index, at_rule) in tree.nodes.iter().enumerate() {
      let is_keyframes = at_rule.kind == NodeKind::AtRule
        && ctx
          .source_slice(at_rule.name_span.start, at_rule.name_span.end)
          .is_some_and(is_keyframes_name);
      if !is_keyframes {
        continue;
      }
      // Every rule inside the block, however deep.
      let rules: Vec<(usize, &str)> = descendants(&tree, index)
        .filter(|&i| tree.nodes[i].kind == NodeKind::Rule)
        .filter_map(|i| {
          let selector = &tree.nodes[i].name_span;
          ctx
            .source_slice(selector.start, selector.end)
            .map(|text| (selector.start, text))
        })
        .collect();
      let keyword_only_block = notation == Notation::PercentageUnlessKeywordOnly
        && rules.iter().all(|(_, selector)| {
          split_list(&strip_comments(selector))
            .iter()
            .all(|s| is_keyword(&s.to_lowercase()))
        });

      for (start, selector) in rules {
        if !may_need_fix(notation, &strip_comments(selector)) {
          continue;
        }
        for (offset, written) in keyframe_selectors(selector) {
          let normalized = written.to_ascii_lowercase();
          let fixed = match notation {
            Notation::Keyword if !is_keyword(&normalized) => keyword_for(&normalized),
            Notation::Percentage if is_keyword(&normalized) => percentage_for(&normalized),
            Notation::PercentageUnlessKeywordOnly
              if is_keyword(&normalized) && !keyword_only_block =>
            {
              percentage_for(&normalized)
            }
            _ => continue,
          };
          let span = Span::new(start + offset, normalized.len());
          diags.push(
            Diagnostic::new(
              self.name(),
              format!("Expected \"{written}\" to be \"{fixed}\""),
            )
            .severity(self.default_severity())
            .span(span)
            .fix(Fix::new(
              format!("Replace \"{written}\" with \"{fixed}\""),
              vec![Edit::new(span, fixed)],
            )),
          );
        }
      }
    }
    diags
  }
}

/// Whether an at-rule name is `keyframes`, optionally vendor-prefixed.
fn is_keyframes_name(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  let unprefixed = ["-o-", "-moz-", "-ms-", "-webkit-"]
    .iter()
    .find_map(|prefix| lower.strip_prefix(prefix))
    .unwrap_or(&lower);
  unprefixed == "keyframes"
}

/// Indices of every node inside the block of node `index`.
fn descendants<'t>(tree: &'t PostcssTree<'_>, index: usize) -> impl Iterator<Item = usize> + 't {
  let mut stack: Vec<usize> = tree
    .children_of(Some(index))
    .iter()
    .rev()
    .copied()
    .collect();
  std::iter::from_fn(move || {
    let next = stack.pop()?;
    stack.extend(tree.children_of(Some(next)).iter().rev().copied());
    Some(next)
  })
}

/// Whether a lowercased selector is `from` or `to`.
fn is_keyword(selector: &str) -> bool {
  matches!(selector, "from" | "to")
}

/// The keyword for `0%` or `100%`.
fn keyword_for(percentage: &str) -> &'static str {
  if percentage == "0%" { "from" } else { "to" }
}

/// The percentage for `from` or `to`.
fn percentage_for(keyword: &str) -> &'static str {
  if keyword == "from" { "0%" } else { "100%" }
}

/// Stylelint's selector prefilter: `keyword` looks for `0%`/`100%` at the
/// start or after a comma or space, `percentage` for the words `from`/`to`.
fn may_need_fix(notation: Notation, selector: &str) -> bool {
  match notation {
    Notation::Keyword => ["0%", "100%"].iter().any(|p| {
      selector.match_indices(p).any(|(i, _)| {
        i == 0 || {
          let prev = selector.as_bytes()[i - 1];
          prev == b',' || prev.is_ascii_whitespace()
        }
      })
    }),
    Notation::Percentage => {
      let lower = selector.to_ascii_lowercase();
      let bytes = lower.as_bytes();
      let is_word = |i: usize| {
        bytes
          .get(i)
          .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
      };
      ["from", "to"].iter().any(|w| {
        lower
          .match_indices(w)
          .any(|(i, _)| (i == 0 || !is_word(i - 1)) && !is_word(i + w.len()))
      })
    }
    Notation::PercentageUnlessKeywordOnly => !selector.is_empty(),
  }
}

/// The keyframe selectors of a rule that are a lone `from`, `to`, `0%` or
/// `100%` (any case), with their offsets in `selector`.  A selector with
/// anything else beside its comments and whitespace is skipped, as
/// Stylelint skips any that is not a single type selector.
fn keyframe_selectors(selector: &str) -> Vec<(usize, &str)> {
  let mut found = Vec::new();
  for (start, end) in top_level_parts(selector) {
    let mut words = Vec::new();
    let mut i = start;
    let bytes = selector.as_bytes();
    while i < end {
      if selector[i..end].starts_with("/*") {
        i = selector[i + 2..end]
          .find("*/")
          .map_or(end, |j| i + 2 + j + 2);
        continue;
      }
      if bytes[i].is_ascii_whitespace() {
        i += 1;
        continue;
      }
      let word_start = i;
      while i < end && !bytes[i].is_ascii_whitespace() && !selector[i..end].starts_with("/*") {
        i += selector[i..].chars().next().map_or(1, char::len_utf8);
      }
      words.push((word_start, &selector[word_start..i]));
    }
    if let [(offset, word)] = words[..] {
      let lower = word.to_ascii_lowercase();
      if matches!(lower.as_str(), "from" | "to" | "0%" | "100%") {
        found.push((offset, word));
      }
    }
  }
  found
}

/// Byte ranges of the comma-separated parts of `selector`, splitting only
/// at commas outside strings, comments, parentheses and brackets.
fn top_level_parts(selector: &str) -> Vec<(usize, usize)> {
  let bytes = selector.as_bytes();
  let mut parts = Vec::new();
  let mut depth = 0usize;
  let mut start = 0;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = selector[i + 2..]
          .find("*/")
          .map_or(bytes.len(), |j| i + 2 + j + 2);
        continue;
      }
      quote @ (b'"' | b'\'') => {
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
          i += if bytes[i] == b'\\' { 2 } else { 1 };
        }
      }
      b'(' | b'[' => depth += 1,
      b')' | b']' => depth = depth.saturating_sub(1),
      b',' if depth == 0 => {
        parts.push((start, i));
        start = i + 1;
      }
      _ => {}
    }
    i += 1;
  }
  parts.push((start, bytes.len().max(start)));
  parts
}

/// `text` with its `/* */` comments removed.
fn strip_comments(text: &str) -> String {
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

/// PostCSS's `list.comma`: the trimmed, non-empty comma-separated parts.
fn split_list(text: &str) -> Vec<&str> {
  top_level_parts(text)
    .into_iter()
    .map(|(start, end)| text[start..end].trim())
    .filter(|part| !part.is_empty())
    .collect()
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::fix_testing::{fix, warnings};

  const RULE: &str = "keyframe-selector-notation";

  #[test]
  fn fixes_to_keywords() {
    assert_eq!(
      fix(
        RULE,
        json!(["keyword"]),
        "@keyframes foo { 0% {} 50% {} 100% {} }",
        Syntax::Css
      ),
      "@keyframes foo { from {} 50% {} to {} }"
    );
  }

  #[test]
  fn fixes_to_percentages_keeping_comments() {
    assert_eq!(
      fix(
        RULE,
        json!(["percentage"]),
        "@-webkit-keyframes foo { FROM, /* c */ to {} }",
        Syntax::Css
      ),
      "@-webkit-keyframes foo { 0%, /* c */ 100% {} }"
    );
  }

  #[test]
  fn keyword_only_blocks_may_keep_keywords() {
    let unless = || json!(["percentage-unless-within-keyword-only-block"]);
    assert!(
      warnings(
        RULE,
        unless(),
        "@keyframes foo { from {} to {} }",
        Syntax::Css
      )
      .is_empty()
    );
    assert_eq!(
      fix(RULE, unless(), "@keyframes foo { 0%,TO {} }", Syntax::Css),
      "@keyframes foo { 0%,100% {} }"
    );
  }

  #[test]
  fn ignores_rules_outside_keyframes() {
    assert!(
      warnings(
        RULE,
        json!(["percentage"]),
        "from { color: red }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["keyword", { "disableFix": true }]);
    let source = "@keyframes foo { 0% {} }";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(warnings(RULE, options, source, Syntax::Css).len(), 1);
  }
}
