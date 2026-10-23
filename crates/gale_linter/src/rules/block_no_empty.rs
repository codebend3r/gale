use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::{NodeKind, NodeRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "block-no-empty";

declare_rule! {
  /// Reports when a CSS rule block has no declarations or nested children.
  ///
  /// Equivalent to Stylelint's `block-no-empty` rule.
  pub struct BlockNoEmpty => BlockNoEmptyCheck {
    name: NAME,
    description: "Disallow empty blocks",
    default_severity: Warning,
  }
}

/// `block-no-empty` as configured.
pub struct BlockNoEmptyCheck {
  /// `ignore: ["comments"]`: a block holding only comments is empty.
  ignore_comments: bool,
}

impl BlockNoEmptyCheck {
  /// Stylelint's options: `true`, then `ignore: ["comments"]`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    let ignore = options.keywords("ignore", &["comments"]);
    options.finish()?;
    Ok(Self {
      ignore_comments: ignore.contains(&"comments"),
    })
  }
}

impl Check for BlockNoEmptyCheck {
  /// Flags a rule or at-rule whose block holds nothing. With `ignore:
  /// ["comments"]` a block holding only comments counts as empty.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk()
      .filter(|node| matches!(node.kind, NodeKind::Rule | NodeKind::AtRule))
      .filter_map(|node| self.check_block(node, context))
      .collect()
  }
}

impl BlockNoEmptyCheck {
  /// The report for `node`'s block, if it has one and it is empty.
  fn check_block(&self, node: NodeRef, context: &RuleContext) -> Option<Diagnostic> {
    let open = node.block_open?;
    let end = node.span().end();
    // The text between the braces (the `}` is missing at the end of an
    // unclosed file).
    let body = context.source_slice(open + 1, end)?;
    let body = body.strip_suffix('}').unwrap_or(body);
    let empty = match analyze_block_body(body, context.syntax) {
      BlockContent::HasContent => false,
      BlockContent::Empty => true,
      // Block contains only a disable directive that covers block-no-empty:
      // the report would be suppressed anyway, but it would start at the
      // `{`, before the directive.
      BlockContent::OnlyCoveringDisable => false,
      // Comments count as content unless `ignore: ["comments"]`.
      BlockContent::OnlyRegularComments => self.ignore_comments,
    };
    empty.then(|| Diagnostic::new(NAME, "Unexpected empty block").span(Span::new(open, end - open)))
  }
}

/// Result of analyzing a block body.
enum BlockContent {
  /// Block has meaningful non-comment content.
  HasContent,
  /// Block is empty (only whitespace, possibly with disable directives).
  Empty,
  /// Block contains only regular (non-directive) comments.
  OnlyRegularComments,
  /// Block contains only disable directives that cover block-no-empty.
  OnlyCoveringDisable,
}

/// Check whether a block body (text between `{` and `}`) has meaningful content.
fn analyze_block_body(body: &str, syntax: Syntax) -> BlockContent {
  let bytes = body.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  let mut has_regular_comments = false;
  let mut has_covering_disable = false;

  while i < len {
    let b = bytes[i];

    // Skip whitespace
    if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
      i += 1;
      continue;
    }

    // Check for block comments /* ... */
    if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
      if let Some(end) = find_block_comment_end(bytes, i + 2) {
        let comment_inner = &body[i + 2..end];
        let trimmed = comment_inner.trim();
        if is_any_directive(trimmed) {
          // Disable/enable directive — not regular content
          if is_covering_disable(trimmed) {
            has_covering_disable = true;
          }
        } else {
          has_regular_comments = true;
        }
        i = end + 2; // skip past */
        continue;
      }
      // Unterminated comment
      has_regular_comments = true;
      break;
    }

    // Check for line comments // (SCSS/Less)
    if b == b'/'
      && i + 1 < len
      && bytes[i + 1] == b'/'
      && matches!(syntax, Syntax::Scss | Syntax::Less | Syntax::Sass)
    {
      let mut j = i + 2;
      while j < len && bytes[j] != b'\n' {
        j += 1;
      }
      let comment_inner = &body[i + 2..j];
      let trimmed = comment_inner.trim();
      if is_any_directive(trimmed) {
        if is_covering_disable(trimmed) {
          has_covering_disable = true;
        }
      } else {
        has_regular_comments = true;
      }
      i = if j < len { j + 1 } else { j };
      continue;
    }

    // Any other non-whitespace character is content
    return BlockContent::HasContent;
  }

  if has_regular_comments {
    BlockContent::OnlyRegularComments
  } else if has_covering_disable {
    BlockContent::OnlyCoveringDisable
  } else {
    BlockContent::Empty
  }
}

/// Find the end of a block comment (position of `*` in `*/`).
fn find_block_comment_end(bytes: &[u8], from: usize) -> Option<usize> {
  let mut j = from;
  while j + 1 < bytes.len() {
    if bytes[j] == b'*' && bytes[j + 1] == b'/' {
      return Some(j);
    }
    j += 1;
  }
  None
}

/// Check if comment text is any stylelint/gale directive (disable, enable, etc.).
fn is_any_directive(text: &str) -> bool {
  for prefix in &["stylelint-", "gale-"] {
    if let Some(rest) = text.strip_prefix(prefix)
      && (rest.starts_with("disable") || rest.starts_with("enable"))
    {
      return true;
    }
  }
  false
}

/// Check if comment text is a stylelint/gale disable directive that would
/// cover the `block-no-empty` rule (either disabling all rules or specifically
/// `block-no-empty`).
fn is_covering_disable(text: &str) -> bool {
  for prefix in &["stylelint-", "gale-"] {
    if let Some(rest) = text.strip_prefix(prefix) {
      // disable, disable-line, disable-next-line
      if let Some(after) = rest.strip_prefix("disable") {
        let after = after.strip_prefix("-next-line").or(Some(after));
        let after = after
          .unwrap()
          .strip_prefix("-line")
          .unwrap_or(after.unwrap());
        let rules_text = after.trim();
        // Empty means all rules disabled
        if rules_text.is_empty() {
          return true;
        }
        // Check if block-no-empty is in the comma-separated list
        for rule_name in rules_text.split(',') {
          if rule_name.trim() == "block-no-empty" {
            return true;
          }
        }
      }
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options, lint};

  /// The offsets and lengths reported for `source` in `syntax`.
  fn spans(source: &str, syntax: Syntax) -> Vec<(usize, usize)> {
    configured(&BlockNoEmpty, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| (d.span.offset, d.span.length))
      .collect()
  }

  #[test]
  fn reports_empty_style_rule() {
    let diags = configured(&BlockNoEmpty, None).check_source(&context("a { }", Syntax::Css));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected empty block");
    // From the `{` to the end of the block.
    assert_eq!((diags[0].span.offset, diags[0].span.length), (2, 3));
  }

  #[test]
  fn ignores_non_empty_style_rule() {
    assert!(spans("a { color: red; }", Syntax::Css).is_empty());
  }

  #[test]
  fn block_with_comment_is_non_empty_by_default() {
    assert!(spans("a { /* foo */ }", Syntax::Css).is_empty());
    assert!(spans("a { // foo\n}", Syntax::Scss).is_empty());
  }

  #[test]
  fn block_with_semicolons_is_non_empty() {
    assert!(spans("a { ; }", Syntax::Css).is_empty());
  }

  #[test]
  fn allows_scss_block_with_include() {
    assert!(spans(".btn { @include button-styles; }", Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_truly_empty_scss_block() {
    assert_eq!(spans(".btn {  }", Syntax::Scss).len(), 1);
  }

  #[test]
  fn reports_empty_at_rule_blocks_but_not_blockless_ones() {
    let source = "@import \"x\"; @media print {} a {}";
    assert_eq!(spans(source, Syntax::Css), [(26, 2), (31, 2)]);
  }

  #[test]
  fn reports_nested_blocks_wherever_they_are() {
    let source = "a { @media print { } b { } }";
    assert_eq!(spans(source, Syntax::Scss), [(17, 3), (23, 3)]);
    // `@else` is a sibling of `@if`, so the empty `@if` is reported too.
    let source = "@if $a {} @else { b: c }";
    assert_eq!(spans(source, Syntax::Scss), [(7, 2)]);
  }

  #[test]
  fn a_covering_disable_comment_keeps_the_block_quiet() {
    let source = "a { /* stylelint-disable-line block-no-empty */ }";
    assert!(spans(source, Syntax::Css).is_empty());
    // Other directives are not content.
    let source = "a { /* stylelint-disable color-named */ }";
    assert_eq!(spans(source, Syntax::Css).len(), 1);
  }

  #[test]
  fn ignore_comments_counts_a_commented_block_as_empty() {
    let source = "a { /* note */ }";
    assert!(lint(NAME, serde_json::json!(true), source, Syntax::Css).is_empty());
    let options = serde_json::json!([true, { "ignore": ["comments"] }]);
    assert_eq!(lint(NAME, options, source, Syntax::Css).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, serde_json::json!("yes")),
      ["Invalid option value \"yes\" for rule \"block-no-empty\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignore": ["comments", "blocks"] }])
      ),
      ["Invalid value \"blocks\" for option \"ignore\" of rule \"block-no-empty\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!({ "ignore": "comments", "except": [] })
      ),
      ["Invalid option name \"except\" for rule \"block-no-empty\""]
    );
  }
}
