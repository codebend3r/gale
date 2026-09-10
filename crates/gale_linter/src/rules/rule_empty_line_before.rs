use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow an empty line before rules.
///
/// Equivalent to Stylelint's `rule-empty-line-before` rule.
/// Supports primary options: "always", "never", "always-multi-line", "never-multi-line".
/// Supports secondary options: `except` and `ignore`.
pub struct RuleEmptyLineBefore;

impl Rule for RuleEmptyLineBefore {
  fn name(&self) -> &'static str {
    "rule-empty-line-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow an empty line before rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let opts = Options::from_ctx(ctx);
    let mut diags = Vec::new();
    check_nodes(self, nodes, ctx, &opts, true, &mut diags);
    diags
  }
}

/// Parsed options for rule-empty-line-before.
struct Options {
  primary: PrimaryOption,
  except_first_nested: bool,
  except_after_single_line_comment: bool,
  except_after_rule: bool,
  except_inside_block_and_after_rule: bool,
  except_inside_block: bool,
  ignore_after_comment: bool,
  ignore_first_nested: bool,
  ignore_inside_block: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PrimaryOption {
  Always,
  Never,
  AlwaysMultiLine,
  NeverMultiLine,
}

impl Options {
  fn from_ctx(ctx: &RuleContext) -> Self {
    let mut opts = Options {
      primary: PrimaryOption::Always,
      except_first_nested: false,
      except_after_single_line_comment: false,
      except_after_rule: false,
      except_inside_block_and_after_rule: false,
      except_inside_block: false,
      ignore_after_comment: false,
      ignore_first_nested: false,
      ignore_inside_block: false,
    };

    let Some(value) = ctx.options else {
      return opts;
    };

    // Options can be:
    // - A string: "always" / "never" / "always-multi-line" / "never-multi-line"
    // - An array: ["always", { except: [...], ignore: [...] }]
    match value {
      serde_json::Value::String(s) => {
        opts.primary = parse_primary(s);
      }
      serde_json::Value::Array(arr) => {
        if let Some(primary_str) = arr.first().and_then(|v| v.as_str()) {
          opts.primary = parse_primary(primary_str);
        }
        if let Some(secondary) = arr.get(1) {
          parse_secondary(&mut opts, secondary);
        }
      }
      _ => {}
    }

    opts
  }
}

fn parse_primary(s: &str) -> PrimaryOption {
  match s {
    "never" => PrimaryOption::Never,
    "always-multi-line" => PrimaryOption::AlwaysMultiLine,
    "never-multi-line" => PrimaryOption::NeverMultiLine,
    _ => PrimaryOption::Always,
  }
}

fn parse_secondary(opts: &mut Options, value: &serde_json::Value) {
  if let Some(except) = value.get("except").and_then(|v| v.as_array()) {
    for item in except {
      if let Some(s) = item.as_str() {
        match s {
          "first-nested" => opts.except_first_nested = true,
          "after-single-line-comment" => opts.except_after_single_line_comment = true,
          "after-rule" => opts.except_after_rule = true,
          "inside-block-and-after-rule" => opts.except_inside_block_and_after_rule = true,
          "inside-block" => opts.except_inside_block = true,
          _ => {}
        }
      }
    }
  }
  if let Some(ignore) = value.get("ignore").and_then(|v| v.as_array()) {
    for item in ignore {
      if let Some(s) = item.as_str() {
        match s {
          "after-comment" => opts.ignore_after_comment = true,
          "first-nested" => opts.ignore_first_nested = true,
          "inside-block" => opts.ignore_inside_block = true,
          _ => {}
        }
      }
    }
  }
}

/// Check if the text before a node has an empty line (double newline).
///
/// An "empty line" is a line containing only whitespace.  For example,
/// `"  ;\n  \n  &"` has an empty line before `&` even though the blank
/// line contains spaces.
fn has_empty_line_before(source: &str, offset: usize) -> bool {
  if offset == 0 || offset > source.len() {
    return false;
  }
  let before = &source[..offset];

  // Find the start of the current line (skip back past the indentation
  // leading up to the node).  We want to look at complete lines only.
  let last_newline = before.rfind('\n');
  let Some(nl_pos) = last_newline else {
    return false; // No newline before → first line → no empty line.
  };

  // Now look at lines before `nl_pos` for a blank line.
  let before_lines = &before[..nl_pos];
  let mut found_blank = false;
  for line in before_lines.rsplit('\n') {
    let stripped = line.trim_matches(|c: char| c == ' ' || c == '\t' || c == '\r');
    if stripped.is_empty() {
      found_blank = true;
    } else {
      return found_blank;
    }
  }
  // If we exhausted all lines and they were all blank, there's no prior
  // content to separate from → no empty line in the meaningful sense.
  false
}

/// Check if a node is the first meaningful (non-comment) child in a block.
fn is_first_nested_in_list(nodes: &[CssNode], index: usize) -> bool {
  if index == 0 {
    return true;
  }
  nodes[..index]
    .iter()
    .all(|n| matches!(n, CssNode::Comment(_)))
}

/// Check if a Style node at `index` is the first `CssNode::Style` in the list.
/// Declarations, at-rules, and comments that appear before it are ignored.
fn is_first_style_in_list(nodes: &[CssNode], index: usize) -> bool {
  nodes[..index]
    .iter()
    .all(|n| !matches!(n, CssNode::Style(_)))
}

/// Check if a node is first-nested by looking at the source (follows an opening brace).
///
/// Matches Stylelint's `isFirstNested` semantics: a node is first-nested only
/// if it is literally the first child of its parent block.  If a comment appears
/// between the opening `{` and this node, the node is NOT first-nested (the
/// comment is the first child).
fn is_first_nested_by_source(source: &str, offset: usize) -> bool {
  if offset == 0 || offset > source.len() {
    return false;
  }
  let before = &source[..offset];
  let trimmed = before.trim_end();
  trimmed.ends_with('{')
}

/// Check if the previous sibling is a single-line comment.
///
/// Matches Stylelint's `isAfterSingleLineComment()` which considers ANY
/// comment that occupies a single line — both `//` line comments and
/// `/* … */` block comments that start and end on the same line.
///
/// An extra adjacency check verifies that the comment's span actually ends
/// near the current rule's offset.  This guards against SCSS `//` comments
/// that the parser may hoist out of nested blocks into the root node list,
/// which would otherwise make a root-level rule incorrectly appear to follow
/// a comment.
fn prev_is_single_line_comment(
  nodes: &[CssNode],
  index: usize,
  source: &str,
  rule_offset: usize,
) -> bool {
  if index == 0 {
    return false;
  }
  match &nodes[index - 1] {
    CssNode::Comment(c) => {
      // Adjacency check: the comment must end on or just before the
      // rule in the source.  Allow up to a few hundred bytes of
      // whitespace/newlines between them (generous to accommodate
      // indentation), but reject comments that are far away (inside
      // a different block).
      let comment_end = c.span.offset + c.span.length;
      if rule_offset > comment_end + 200 {
        return false;
      }
      // Also verify the text between the comment end and the rule
      // offset contains only whitespace.
      if comment_end <= rule_offset && comment_end <= source.len() {
        let between = &source[comment_end..rule_offset.min(source.len())];
        if between.chars().any(|ch| !ch.is_whitespace()) {
          return false;
        }
      }

      // SCSS // comments are always single-line.
      if c.is_line {
        return true;
      }
      // For block comments, check if they occupy a single line in source.
      let start = c.span.offset;
      let end = (start + c.span.length).min(source.len());
      if start < source.len() {
        !source[start..end].contains('\n')
      } else {
        false
      }
    }
    _ => false,
  }
}

/// Check if the previous sibling is any comment (AST-based).
fn prev_is_comment(nodes: &[CssNode], index: usize) -> bool {
  if index == 0 {
    return false;
  }
  matches!(&nodes[index - 1], CssNode::Comment(_))
}

/// Source-based check: is the previous non-whitespace content before `offset` a comment?
/// This catches both single-line and multi-line block comments (`/** ... */`),
/// as well as SCSS `//` comments that may not be in the AST.
fn prev_line_is_comment(source: &str, offset: usize) -> bool {
  if offset == 0 || offset > source.len() {
    return false;
  }
  let before = &source[..offset];
  let bytes = before.as_bytes();
  let mut pos = before.len();

  // Walk backwards past whitespace
  while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t' | b'\n' | b'\r') {
    pos -= 1;
  }
  if pos == 0 {
    return false;
  }

  // `pos` is a byte offset into ASCII whitespace, so it's always a valid
  // char boundary.  However, the content *before* the whitespace may contain
  // multi-byte characters.  Use the trimmed slice for all subsequent checks.
  let trimmed = &before[..pos];

  // Check if the previous content ends with `*/` (block comment)
  if trimmed.ends_with("*/") {
    // Find the matching `/*`
    if trimmed[..trimmed.len() - 2].rfind("/*").is_some() {
      return true;
    }
  }

  // Check if the previous line starts with `//` (SCSS line comment)
  let line_start = trimmed.rfind('\n').map(|p| p + 1).unwrap_or(0);
  let line = trimmed[line_start..].trim();
  line.starts_with("//")
}

/// Source-based check: is the non-empty line immediately before `offset` a
/// single-line comment?  This catches SCSS `//` comments that may not
/// appear in the AST.  Matches Stylelint's `isAfterSingleLineComment()`
/// which considers both `//` comments and single-line `/* … */` block
/// comments.
fn prev_line_is_single_line_comment(source: &str, offset: usize) -> bool {
  if offset == 0 || offset > source.len() {
    return false;
  }
  let before = &source[..offset];
  let trimmed = before.trim_end_matches([' ', '\t']);
  let trimmed = trimmed.strip_suffix('\n').unwrap_or(trimmed);
  let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
  let line_start = trimmed.rfind('\n').map(|p| p + 1).unwrap_or(0);
  let line = trimmed[line_start..].trim();
  line.starts_with("//") || (line.starts_with("/*") && line.ends_with("*/"))
}

/// Source-based check: is there a multi-line comment (e.g. `/** ... */`)
/// immediately before the offset?  This is used to distinguish multi-line
/// from single-line comments for `except: ["after-single-line-comment"]`.
fn prev_is_multi_line_comment_by_source(source: &str, offset: usize) -> bool {
  if offset < 4 || offset > source.len() {
    return false;
  }
  let before = &source[..offset];
  // Walk backwards past whitespace
  let bytes = before.as_bytes();
  let mut pos = before.len();
  while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t' | b'\n' | b'\r') {
    pos -= 1;
  }
  if pos < 2 {
    return false;
  }
  // Check for `*/`
  if &before[pos - 2..pos] == "*/" {
    // Find the matching `/*`
    if let Some(open) = before[..pos - 2].rfind("/*") {
      // Multi-line if contains a newline between /* and */
      return before[open..pos].contains('\n');
    }
  }
  false
}

/// Check whether a selector contains preprocessor interpolation (`#{`, `@{`)
/// or other non-standard-syntax patterns that Stylelint would skip.
fn has_interpolation(selector: &str) -> bool {
  selector.contains("#{") || selector.contains("@{")
}

/// Check if the previous sibling is a style rule.
fn prev_is_rule(nodes: &[CssNode], index: usize) -> bool {
  if index == 0 {
    return false;
  }
  matches!(&nodes[index - 1], CssNode::Style(_))
}

/// Source-based check: is the previous meaningful content before `offset`
/// a closing brace `}` (indicating the rule follows another rule/block)?
fn prev_is_rule_by_source(source: &str, offset: usize) -> bool {
  if offset == 0 || offset > source.len() {
    return false;
  }
  let before = &source[..offset];
  // Walk backwards past whitespace and comments
  let bytes = before.as_bytes();
  let mut pos = before.len();
  loop {
    while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t' | b'\n' | b'\r') {
      pos -= 1;
    }
    if pos == 0 {
      return false;
    }
    // Skip block comments
    if pos >= 2 && &before[pos - 2..pos] == "*/" {
      if let Some(open) = before[..pos - 2].rfind("/*") {
        pos = open;
        continue;
      }
      return false;
    }
    // Check for `}`
    return bytes[pos - 1] == b'}';
  }
}

/// Check if a style rule is multi-line by looking at the source.
fn is_rule_multi_line(source: &str, span: &gale_css_parser::Span) -> bool {
  let start = span.offset;
  let end = (start + span.length).min(source.len());
  if start >= source.len() {
    return false;
  }
  source[start..end].contains('\n')
}

fn check_nodes(
  rule_impl: &RuleEmptyLineBefore,
  nodes: &[CssNode],
  ctx: &RuleContext,
  opts: &Options,
  is_root: bool,
  diags: &mut Vec<Diagnostic>,
) {
  for (i, node) in nodes.iter().enumerate() {
    if let CssNode::Style(style) = node {
      // Stylelint skips rules whose selectors contain SCSS/Less
      // interpolation (e.g. `.#{$prefix}--foo`) because they are not
      // "standard syntax".  Match that behavior.
      if has_interpolation(&style.selector) {
        check_children(rule_impl, style, ctx, opts, diags);
        continue;
      }

      // Stylelint skips SCSS placeholder selectors (`%placeholder`).
      // These are non-standard syntax used with `@extend`.
      if style.selector.trim_start().starts_with('%') {
        check_children(rule_impl, style, ctx, opts, diags);
        continue;
      }

      let offset = style.span.offset;

      // Determine if this is first-nested: the first meaningful content after
      // the opening `{` of the parent block.
      let first_nested = if is_root {
        is_first_nested_in_list(nodes, i)
      } else {
        is_first_nested_by_source(ctx.source, offset)
      };

      // At the document root, the very first rule (or first after only comments)
      // is never checked — there's nothing before it to separate from.
      if is_root && first_nested {
        check_children(rule_impl, style, ctx, opts, diags);
        continue;
      }

      // ignore: ["first-nested"] — skip entirely
      if opts.ignore_first_nested && first_nested {
        check_children(rule_impl, style, ctx, opts, diags);
        continue;
      }

      // ignore: ["after-comment"] — skip if preceded by a comment.
      if opts.ignore_after_comment
        && (prev_is_comment(nodes, i) || prev_line_is_comment(ctx.source, offset))
      {
        check_children(rule_impl, style, ctx, opts, diags);
        continue;
      }

      // ignore: ["inside-block"] — skip all rules that are inside a block (not root level)
      if opts.ignore_inside_block && !is_root {
        check_children(rule_impl, style, ctx, opts, diags);
        continue;
      }

      let has_empty = has_empty_line_before(ctx.source, offset);

      // Determine the expectation based on primary option.
      // For *-multi-line variants, skip single-line rules entirely.
      let is_multi = is_rule_multi_line(ctx.source, &style.span);

      let expects_empty = match opts.primary {
        PrimaryOption::Always => true,
        PrimaryOption::Never => false,
        PrimaryOption::AlwaysMultiLine => {
          if !is_multi {
            check_children(rule_impl, style, ctx, opts, diags);
            continue;
          }
          true
        }
        PrimaryOption::NeverMultiLine => {
          if !is_multi {
            check_children(rule_impl, style, ctx, opts, diags);
            continue;
          }
          false
        }
      };

      // Apply exceptions — flip the expectation at most once.
      // Stylelint evaluates exceptions in order and stops after the
      // first match (PR #2920).
      let mut expectation = expects_empty;

      // Determine if this rule is after another rule — use both AST and source checks.
      let after_rule = prev_is_rule(nodes, i) || prev_is_rule_by_source(ctx.source, offset);

      let exception_matched = if opts.except_first_nested && first_nested {
        true
      } else if opts.except_after_single_line_comment {
        // For after-single-line-comment, we must NOT match multi-line comments.
        let is_after_single = prev_is_single_line_comment(nodes, i, ctx.source, offset)
          || prev_line_is_single_line_comment(ctx.source, offset);
        let is_after_multi = prev_is_multi_line_comment_by_source(ctx.source, offset);
        // Only match if it's after a single-line comment but NOT a multi-line one.
        is_after_single && !is_after_multi
      } else if opts.except_inside_block_and_after_rule && !is_root && after_rule {
        true
      } else if opts.except_inside_block && !is_root {
        true
      } else {
        opts.except_after_rule && after_rule
      };

      if exception_matched {
        expectation = !expectation;
      }

      // Check and report
      if expectation && !has_empty {
        diags.push(
          Diagnostic::new(
            rule_impl.name(),
            "Expected empty line before rule".to_string(),
          )
          .severity(rule_impl.default_severity())
          .span(Span::new(style.span.offset, style.span.length)),
        );
      } else if !expectation && has_empty {
        diags.push(
          Diagnostic::new(
            rule_impl.name(),
            "Unexpected empty line before rule".to_string(),
          )
          .severity(rule_impl.default_severity())
          .span(Span::new(style.span.offset, style.span.length)),
        );
      }

      // Recurse into nested rules within this style rule
      check_children(rule_impl, style, ctx, opts, diags);
    }

    // Recurse into at-rules (including @keyframes — Stylelint checks
    // rule-empty-line-before inside keyframe blocks via root.walkRules()).
    if let CssNode::AtRule(at_rule) = node {
      check_nodes(rule_impl, &at_rule.children, ctx, opts, false, diags);
    }
  }
}

fn check_children(
  rule_impl: &RuleEmptyLineBefore,
  style: &gale_css_parser::StyleRule,
  ctx: &RuleContext,
  opts: &Options,
  diags: &mut Vec<Diagnostic>,
) {
  if !style.children.is_empty() {
    let child_nodes: Vec<CssNode> = style
      .children
      .iter()
      .map(|sr| CssNode::Style(sr.clone()))
      .collect();
    check_nodes(rule_impl, &child_nodes, ctx, opts, false, diags);
  }
  // Also recurse into nested at-rules (e.g. @include, @media inside a
  // style rule) which may contain style rules that need checking.
  for at_node in &style.nested_at_rules {
    if let CssNode::AtRule(at_rule) = at_node {
      check_nodes(rule_impl, &at_rule.children, ctx, opts, false, diags);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn make_ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn make_ctx_with_options<'a>(source: &'a str, options: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(options),
    }
  }

  #[test]
  fn reports_missing_empty_line_before_rule() {
    // Default: "always" with no options
    let src = "a { color: red; }\nb { color: blue; }";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 10),
          important: false,
        }],
        span: ParserSpan::new(0, 17),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: "b".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(b_offset + 4, 11),
          important: false,
        }],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }),
    ];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected empty line before rule"));
  }

  #[test]
  fn allows_empty_line_before_rule() {
    let src = "a { color: red; }\n\nb { color: blue; }";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 10),
          important: false,
        }],
        span: ParserSpan::new(0, 17),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: "b".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(b_offset + 4, 11),
          important: false,
        }],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }),
    ];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn except_first_nested_allows_no_empty_line() {
    // With except: ["first-nested"], the first rule inside a block
    // should NOT require an empty line (flipped from "always")
    let opts = serde_json::json!(["always", {"except": ["first-nested"]}]);
    let src = "a {\n  b { color: red; }\n}";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      children: vec![StyleRule {
        selector: "b".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(b_offset + 4, 10),
          important: false,
        }],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }],
      span: ParserSpan::new(0, src.len()),

      nested_at_rules: Vec::new(),
    })];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx_with_options(src, &opts));
    assert!(
      d.is_empty(),
      "first-nested exception should suppress diagnostic"
    );
  }

  #[test]
  fn except_after_single_line_comment_flips() {
    // With except: ["after-single-line-comment"], a rule after a // comment
    // should NOT require an empty line (flipped from "always")
    let opts = serde_json::json!(["always", {"except": ["after-single-line-comment"]}]);
    let src = "a { color: red; }\n// comment\nb { color: blue; }";
    let b_offset = src.find("b {").unwrap();
    let comment_offset = src.find("//").unwrap();
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 17),
        ..Default::default()
      }),
      CssNode::Comment(gale_css_parser::Comment {
        is_line: true,
        text: " comment".to_string(),
        span: ParserSpan::new(comment_offset, 10),
      }),
      CssNode::Style(StyleRule {
        selector: "b".to_string(),
        declarations: vec![],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }),
    ];
    let ctx = RuleContext {
      file_path: "t.scss",
      source: src,
      syntax: Syntax::Scss,
      options: Some(&opts),
    };
    let d = RuleEmptyLineBefore.check_root(&nodes, &ctx);
    assert!(
      d.is_empty(),
      "after-single-line-comment exception should suppress diagnostic"
    );
  }

  #[test]
  fn ignore_after_comment_skips() {
    // With ignore: ["after-comment"], a rule after a comment should be ignored entirely
    let opts = serde_json::json!(["always", {"ignore": ["after-comment"]}]);
    let src = "/* comment */\nb { color: blue; }";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![
      CssNode::Comment(gale_css_parser::Comment {
        is_line: false,
        text: " comment ".to_string(),
        span: ParserSpan::new(0, 13),
      }),
      CssNode::Style(StyleRule {
        selector: "b".to_string(),
        declarations: vec![],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }),
    ];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx_with_options(src, &opts));
    assert!(d.is_empty(), "ignore after-comment should skip the check");
  }

  #[test]
  fn ignore_after_multi_line_block_comment_skips() {
    // With ignore: ["after-comment"], a rule after a multi-line block comment
    // (e.g. /** ... */) should be ignored — this was previously broken because
    // prev_line_is_comment only checked the single line immediately before.
    let opts = serde_json::json!(["always", {"ignore": ["after-comment"]}]);
    let src = "/**\n * Multi-line comment\n */\nb { color: blue; }";
    let b_offset = src.find("b {").unwrap();
    let comment_offset = 0;
    let comment_len = src.find("*/").unwrap() + 2;
    let nodes = vec![
      CssNode::Comment(gale_css_parser::Comment {
        is_line: false,
        text: "*\n * Multi-line comment\n ".to_string(),
        span: ParserSpan::new(comment_offset, comment_len),
      }),
      CssNode::Style(StyleRule {
        selector: "b".to_string(),
        declarations: vec![],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }),
    ];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx_with_options(src, &opts));
    assert!(
      d.is_empty(),
      "ignore after-comment should skip rules after multi-line block comments"
    );
  }

  #[test]
  fn ignore_after_multi_line_comment_source_based() {
    // Source-based detection: even without the comment in the AST,
    // prev_line_is_comment should detect multi-line block comments.
    let opts = serde_json::json!(["always", {"ignore": ["after-comment"]}]);
    let src = "a { color: red; }\n/**\n * docs\n */\nb { color: blue; }";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 17),
        ..Default::default()
      }),
      // No Comment node in AST — rely on source-based detection
      CssNode::Style(StyleRule {
        selector: "b".to_string(),
        declarations: vec![],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }),
    ];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx_with_options(src, &opts));
    assert!(
      d.is_empty(),
      "source-based detection should handle multi-line block comments for ignore after-comment"
    );
  }

  #[test]
  fn is_first_nested_not_when_comment_between_brace_and_rule() {
    // A rule that follows a block comment after `{` is NOT first-nested
    // because the comment is the actual first child. Stylelint's
    // `isFirstNested` only returns true if the node is the literal first
    // child — no intervening comments.
    let opts = serde_json::json!(["always", {"except": ["first-nested"]}]);
    let src = "a {\n  /** comment */\n  b { color: red; }\n}";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      children: vec![StyleRule {
        selector: "b".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(b_offset + 4, 10),
          important: false,
        }],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }],
      span: ParserSpan::new(0, src.len()),

      nested_at_rules: Vec::new(),
    })];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx_with_options(src, &opts));
    assert!(
      !d.is_empty(),
      "first-nested exception should NOT apply when a comment sits between {{ and the rule"
    );
  }

  #[test]
  fn is_first_nested_not_when_multi_line_comment_between_brace_and_rule() {
    // A rule that follows a multi-line block comment after `{` is NOT
    // first-nested — the comment is the first child.
    let opts = serde_json::json!(["always", {"except": ["first-nested"]}]);
    let src = "a {\n  /**\n   * docs\n   */\n  b { color: red; }\n}";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      children: vec![StyleRule {
        selector: "b".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(b_offset + 4, 10),
          important: false,
        }],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }],
      span: ParserSpan::new(0, src.len()),

      nested_at_rules: Vec::new(),
    })];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx_with_options(src, &opts));
    assert!(
      !d.is_empty(),
      "first-nested exception should NOT apply with multi-line block comment between {{ and rule"
    );
  }

  #[test]
  fn prev_line_is_comment_detects_multi_line() {
    // Unit test for the prev_line_is_comment helper
    let src = "/**\n * Multi-line\n */\n.foo {}";
    let offset = src.find(".foo").unwrap();
    assert!(
      prev_line_is_comment(src, offset),
      "should detect multi-line block comment"
    );
  }

  #[test]
  fn prev_line_is_comment_detects_single_line_block() {
    let src = "/* single line */\n.foo {}";
    let offset = src.find(".foo").unwrap();
    assert!(
      prev_line_is_comment(src, offset),
      "should detect single-line block comment"
    );
  }

  #[test]
  fn prev_line_is_comment_detects_scss_line_comment() {
    let src = "// scss comment\n.foo {}";
    let offset = src.find(".foo").unwrap();
    assert!(
      prev_line_is_comment(src, offset),
      "should detect SCSS line comment"
    );
  }

  #[test]
  fn never_mode_reports_empty_line() {
    let opts = serde_json::json!(["never"]);
    let src = "a { color: red; }\n\nb { color: blue; }";
    let b_offset = src.find("b {").unwrap();
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 17),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: "b".to_string(),
        declarations: vec![],
        span: ParserSpan::new(b_offset, 18),
        ..Default::default()
      }),
    ];
    let d = RuleEmptyLineBefore.check_root(&nodes, &make_ctx_with_options(src, &opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
  }

  #[test]
  fn keyframes_second_rule_requires_empty_line() {
    use gale_css_parser::{Syntax, parse};
    let source =
      "@keyframes foo {\n  0% {\n    opacity: 0;\n  }\n  100% {\n    opacity: 1;\n  }\n}";
    let opts = serde_json::json!(["always", {"except": ["first-nested"]}]);
    let parsed = parse(source, Syntax::Scss).expect("parse");
    let ctx = make_ctx_with_options(source, &opts);
    let d = RuleEmptyLineBefore.check_root(&parsed.nodes, &ctx);
    assert_eq!(
      d.len(),
      1,
      "Expected 1 diagnostic for 100% without empty line. Got: {:?}",
      d
    );
  }
}
