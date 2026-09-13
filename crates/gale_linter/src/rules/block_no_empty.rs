use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports when a CSS rule block has no declarations or nested children.
///
/// Equivalent to Stylelint's `block-no-empty` rule.
pub struct BlockNoEmpty;

impl Rule for BlockNoEmpty {
  fn name(&self) -> &'static str {
    "block-no-empty"
  }

  fn description(&self) -> &'static str {
    "Disallow empty blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    // Read the `ignore` secondary option.
    // Options may be stored as:
    //   - {"ignore": ["comments"]} directly (when config is [true, {...}])
    //   - ["never", {"ignore": ["comments"]}] (when config is ["never", {...}])
    let secondary = context.secondary_options().or_else(|| {
      // When config is [true/false, {secondary}], the config loader
      // stores only the secondary object directly.
      context.options.filter(|v| v.is_object())
    });
    let ignore_comments = secondary
      .and_then(|obj| obj.get("ignore"))
      .and_then(|v| v.as_array())
      .map(|arr| arr.iter().any(|v| v.as_str() == Some("comments")))
      .unwrap_or(false);

    match node {
      CssNode::Style(rule) => {
        // If the AST recorded declarations or children, it's not empty.
        if !rule.declarations.is_empty() || !rule.children.is_empty() {
          return vec![];
        }
        check_block_source(
          self,
          context,
          rule.span.offset,
          rule.span.offset + rule.span.length,
          ignore_comments,
        )
      }
      CssNode::AtRule(at_rule) => {
        // Only check at-rules that have a block body (e.g., @media, @supports).
        // At-rules without children in the AST need source-level checking.
        // Skip at-rules that don't have blocks (like @import).
        if !at_rule.children.is_empty() {
          return vec![];
        }
        let start = at_rule.span.offset;
        let end = start + at_rule.span.length;
        if end > context.source.len() {
          return vec![];
        }
        let block_src = &context.source[start..end];
        // Only check at-rules that have a block (contain `{`)
        if !block_src.contains('{') {
          return vec![];
        }
        check_block_source(self, context, start, end, ignore_comments)
      }
      _ => vec![],
    }
  }
}

/// Check the source text between `{` and `}` for a block at `[span_start..span_end]`.
/// Returns a diagnostic if the block is empty.
fn check_block_source(
  rule: &BlockNoEmpty,
  context: &RuleContext,
  span_start: usize,
  span_end: usize,
  ignore_comments: bool,
) -> Vec<Diagnostic> {
  if span_end > context.source.len() || span_start >= span_end {
    return vec![];
  }

  let block_src = &context.source[span_start..span_end];

  // Find the opening `{`
  let Some(open_brace_rel) = block_src.find('{') else {
    return vec![];
  };

  // Body is between `{` and the last `}`
  let body = &block_src[open_brace_rel + 1..];
  let body = body.strip_suffix('}').unwrap_or(body);

  let make_diag = || {
    let brace_offset = span_start + open_brace_rel;
    vec![
      Diagnostic::new(rule.name(), "Unexpected empty block")
        .severity(rule.default_severity())
        .span(Span::new(brace_offset, span_end - brace_offset)),
    ]
  };

  match analyze_block_body(body, context.syntax) {
    BlockContent::HasContent => vec![],
    BlockContent::Empty => make_diag(),
    BlockContent::OnlyCoveringDisable => {
      // Block contains only a disable directive that covers block-no-empty.
      // Don't emit the diagnostic — it would be suppressed anyway but the
      // span is at the `{` which is before the disable comment.
      vec![]
    }
    BlockContent::OnlyRegularComments => {
      if ignore_comments {
        // `ignore: ["comments"]` — comments don't count as content
        make_diag()
      } else {
        // Default — regular comments count as content
        vec![]
      }
    }
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn make_context_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_empty_style_rule() {
    let rule = BlockNoEmpty;
    let source = "a { }";
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = make_context_with_source(source);
    let diags = rule.check(&node, &ctx);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected empty block");
    // Should point to the `{`
    assert_eq!(diags[0].span.offset, 2);
  }

  #[test]
  fn ignores_non_empty_style_rule() {
    let rule = BlockNoEmpty;
    let source = "a { color: red; }";
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(4, 10),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = make_context_with_source(source);
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn block_with_comment_is_non_empty_by_default() {
    let rule = BlockNoEmpty;
    let source = "a { /* foo */ }";
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = make_context_with_source(source);
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn block_with_semicolons_is_non_empty() {
    let rule = BlockNoEmpty;
    let source = "a { ; }";
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = make_context_with_source(source);
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_scss_block_with_include() {
    let rule = BlockNoEmpty;
    let source = ".btn { @include button-styles; }";
    let node = CssNode::Style(StyleRule {
      selector: ".btn".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = RuleContext {
      file_path: "test.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn reports_truly_empty_scss_block() {
    let rule = BlockNoEmpty;
    let source = ".btn {  }";
    let node = CssNode::Style(StyleRule {
      selector: ".btn".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = RuleContext {
      file_path: "test.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags = rule.check(&node, &ctx);
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn ignores_non_style_rule_nodes() {
    let rule = BlockNoEmpty;
    let node = CssNode::Comment(gale_css_parser::Comment {
      text: "/* hi */".to_string(),
      span: ParserSpan::new(0, 8),
      is_line: false,
    });
    let ctx = make_context_with_source("/* hi */");
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());
  }
}
