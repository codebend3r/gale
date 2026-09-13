use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow qualifying a selector by type (e.g. `a.foo`, `div#bar`).
///
/// Equivalent to Stylelint's `selector-no-qualifying-type` rule.
pub struct SelectorNoQualifyingType;

impl Rule for SelectorNoQualifyingType {
  fn name(&self) -> &'static str {
    "selector-no-qualifying-type"
  }

  fn description(&self) -> &'static str {
    "Disallow qualifying a selector by type"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore_class = has_ignore_option(ctx, "class");
    let ignore_attribute = has_ignore_option(ctx, "attribute");
    let ignore_id = has_ignore_option(ctx, "id");

    let is_scss = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    );

    let mut diags = Vec::new();
    walk_nodes(
      self,
      nodes,
      ctx,
      is_scss,
      false, // ancestor_has_interpolation
      false, // ancestor_is_type
      ignore_class,
      ignore_attribute,
      ignore_id,
      &mut diags,
    );
    diags
  }
}

/// Recursively walk nodes, tracking whether any ancestor selector contains
/// SCSS interpolation. Stylelint skips rules whose resolved (flattened)
/// selector is non-standard, which includes any rule nested inside a parent
/// whose selector contains `#{...}`.
fn walk_nodes(
  rule_impl: &SelectorNoQualifyingType,
  nodes: &[CssNode],
  ctx: &RuleContext,
  is_scss: bool,
  ancestor_has_interpolation: bool,
  ancestor_is_type: bool,
  ignore_class: bool,
  ignore_attribute: bool,
  ignore_id: bool,
  diags: &mut Vec<Diagnostic>,
) {
  for node in nodes {
    match node {
      CssNode::Style(style) => {
        let self_has_interpolation = is_scss && style.selector.contains("#{");
        let any_interpolation = ancestor_has_interpolation || self_has_interpolation;

        // Only check this selector if neither it nor any ancestor
        // contains SCSS interpolation (matching Stylelint's
        // `isStandardSyntaxRule` skip).
        if !any_interpolation {
          check_selector_list(
            rule_impl,
            &style.selector,
            style.span.offset,
            ancestor_is_type,
            ignore_class,
            ignore_attribute,
            ignore_id,
            diags,
          );
        }

        let self_is_type = selector_ends_with_type(&style.selector);

        // Recurse into children, propagating interpolation flag.
        walk_style_children(
          rule_impl,
          style,
          ctx,
          is_scss,
          any_interpolation,
          self_is_type,
          ignore_class,
          ignore_attribute,
          ignore_id,
          diags,
        );
      }
      CssNode::AtRule(at_rule) => {
        walk_nodes(
          rule_impl,
          &at_rule.children,
          ctx,
          is_scss,
          ancestor_has_interpolation,
          ancestor_is_type,
          ignore_class,
          ignore_attribute,
          ignore_id,
          diags,
        );
      }
      _ => {}
    }
  }
}

fn walk_style_children(
  rule_impl: &SelectorNoQualifyingType,
  style: &gale_css_parser::StyleRule,
  ctx: &RuleContext,
  is_scss: bool,
  ancestor_has_interpolation: bool,
  ancestor_is_type: bool,
  ignore_class: bool,
  ignore_attribute: bool,
  ignore_id: bool,
  diags: &mut Vec<Diagnostic>,
) {
  for child in &style.children {
    let self_has_interpolation = is_scss && child.selector.contains("#{");
    let any_interpolation = ancestor_has_interpolation || self_has_interpolation;

    if !any_interpolation {
      check_selector_list(
        rule_impl,
        &child.selector,
        child.span.offset,
        ancestor_is_type,
        ignore_class,
        ignore_attribute,
        ignore_id,
        diags,
      );
    }

    let self_is_type = selector_ends_with_type(&child.selector);

    walk_style_children(
      rule_impl,
      child,
      ctx,
      is_scss,
      any_interpolation,
      self_is_type || ancestor_is_type,
      ignore_class,
      ignore_attribute,
      ignore_id,
      diags,
    );
  }
}

/// Split a selector string by commas at the top level (not inside parens).
fn split_selector_list(selector: &str) -> Vec<(usize, &str)> {
  let mut parts = Vec::new();
  let bytes = selector.as_bytes();
  let len = bytes.len();
  let mut depth = 0;
  let mut start = 0;

  for i in 0..len {
    match bytes[i] {
      b'(' => depth += 1,
      b')' if depth > 0 => depth -= 1,
      b',' if depth == 0 => {
        parts.push((start, &selector[start..i]));
        start = i + 1;
      }
      _ => {}
    }
  }
  parts.push((start, &selector[start..]));
  parts
}

/// Check a comma-separated selector list, emitting diagnostics with correct
/// byte offsets pointing to each compound selector that violates.
fn check_selector_list(
  rule_impl: &SelectorNoQualifyingType,
  selector_list: &str,
  base_offset: usize,
  ancestor_is_type: bool,
  ignore_class: bool,
  ignore_attribute: bool,
  ignore_id: bool,
  diags: &mut Vec<Diagnostic>,
) {
  for (byte_pos, sel_part) in split_selector_list(selector_list) {
    let trimmed = sel_part.trim();
    if trimmed.is_empty() {
      continue;
    }
    let leading_ws = sel_part.len() - sel_part.trim_start().len();
    let sel_byte_offset = byte_pos + leading_ws;

    if let Some(compound_offset) = find_qualifying_type_offset(
      trimmed,
      ancestor_is_type,
      ignore_class,
      ignore_attribute,
      ignore_id,
    ) {
      let violation_offset = base_offset + sel_byte_offset + compound_offset;
      diags.push(
        Diagnostic::new(
          rule_impl.name(),
          format!("Unexpected qualifying type selector in \"{trimmed}\""),
        )
        .severity(rule_impl.default_severity())
        .span(Span::new(violation_offset, trimmed.len() - compound_offset)),
      );
    }
  }
}

/// Check if the `ignore` secondary option contains a given value.
///
/// Handles both forms of options:
/// - Direct object: `{"ignore": ["id"]}` (from `[true, {...}]` or `["error", {...}]` configs)
/// - Array-wrapped: `[primary, {"ignore": ["id"]}]` (from `["always", {...}]` configs)
fn has_ignore_option(ctx: &RuleContext, value: &str) -> bool {
  let opts = match ctx.options {
    Some(v) => v,
    None => return false,
  };

  // Try direct object access (for [true, {...}] or ["error", {...}] configs
  // where the config resolver stores just the secondary options object)
  if let Some(arr) = opts.get("ignore").and_then(|v| v.as_array())
    && arr.iter().any(|v| v.as_str() == Some(value))
  {
    return true;
  }

  // Then try via secondary_options (for ["always", {...}] configs where
  // the entire array is stored as options)
  if let Some(secondary) = ctx.secondary_options()
    && let Some(arr) = secondary.get("ignore").and_then(|v| v.as_array())
    && arr.iter().any(|v| v.as_str() == Some(value))
  {
    return true;
  }

  false
}

/// Find the byte offset within a single (already comma-split) selector where
/// a qualifying type violation occurs. Returns `None` if no violation.
///
/// Handles:
/// - `div.class`, `div#id`, `div[attr]`
/// - `div:hover#thing` (type + pseudo-class + qualifier)
/// - Nesting `&` with ancestor type context (e.g. `a { &.class {} }`)
/// - `:is()`/`:not()`/`:has()`/`:where()` inner selectors
fn find_qualifying_type_offset(
  selector: &str,
  ancestor_is_type: bool,
  ignore_class: bool,
  ignore_attribute: bool,
  ignore_id: bool,
) -> Option<usize> {
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    // Skip whitespace and combinators
    if chars[i].is_ascii_whitespace() || chars[i] == '>' || chars[i] == '+' || chars[i] == '~' {
      i += 1;
      continue;
    }

    // Start of a compound selector
    let compound_start = i;
    let mut saw_type = false;

    // Check for type selector at start of compound
    if is_type_selector_start(&chars, i) {
      saw_type = true;
      while i < len && is_ident_char(chars[i]) {
        i += 1;
      }
    } else if chars[i] == '%' {
      // SCSS placeholder selector (%foo) — skip like a class
      i += 1;
      while i < len && is_ident_char(chars[i]) {
        i += 1;
      }
    } else if chars[i] == '&' {
      // Nesting selector — if parent is a type selector, treat as type
      i += 1;
      // Consume any trailing ident chars (BEM-style `&--modifier`, `&__element`)
      while i < len && is_ident_char(chars[i]) {
        i += 1;
      }
      if ancestor_is_type {
        saw_type = true;
      }
    }

    // Track whether compound has non-type simple selectors (class, id, attr)
    // for detecting qualifying inside :is()/:not() etc.
    let mut has_class = false;
    let mut has_id = false;
    let mut has_attribute = false;

    // Walk through the rest of the compound selector
    loop {
      if i >= len {
        break;
      }

      // Check for qualifying selector after a type
      if saw_type {
        if chars[i] == '.' && !ignore_class {
          return Some(compound_start);
        }
        if chars[i] == '#' && !ignore_id {
          return Some(compound_start);
        }
        if chars[i] == '[' && !ignore_attribute {
          return Some(compound_start);
        }
      }

      // Skip CSS comments `/* ... */`
      if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2; // skip */
        }
        continue;
      }

      if chars[i] == '.' {
        // Class selector
        has_class = true;
        i += 1;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
      } else if chars[i] == '#' {
        // ID selector
        has_id = true;
        i += 1;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
      } else if chars[i] == '[' {
        // Attribute selector — skip to closing bracket
        has_attribute = true;
        i += 1;
        let mut bracket_depth = 1;
        while i < len && bracket_depth > 0 {
          if chars[i] == '[' {
            bracket_depth += 1;
          } else if chars[i] == ']' {
            bracket_depth -= 1;
          }
          i += 1;
        }
      } else if chars[i] == ':' {
        // Pseudo-class or pseudo-element
        i += 1;
        let is_pseudo_element = i < len && chars[i] == ':';
        if is_pseudo_element {
          i += 1;
        }
        // Consume pseudo name
        let pseudo_start = i;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
        let pseudo_name: String = chars[pseudo_start..i].iter().collect();
        let pseudo_lower = pseudo_name.to_ascii_lowercase();

        if i < len && chars[i] == '(' {
          // Functional pseudo-class
          let is_selector_pseudo = !is_pseudo_element
            && matches!(
              pseudo_lower.as_str(),
              "is" | "not" | "has" | "where" | "matches" | "any"
            );

          if is_selector_pseudo {
            // Parse the args
            let paren_idx = i;
            i += 1;
            let args_start = i;
            let mut depth = 1;
            while i < len && depth > 0 {
              if chars[i] == '(' {
                depth += 1;
              } else if chars[i] == ')' {
                depth -= 1;
              }
              i += 1;
            }
            let args_end = if i > 0 { i - 1 } else { i };
            let inner: String = chars[args_start..args_end].iter().collect();

            // 1) Check inner selector list for standalone qualifying types
            for (inner_pos, inner_part) in split_selector_list(&inner) {
              let inner_trimmed = inner_part.trim();
              if inner_trimmed.is_empty() {
                continue;
              }
              if let Some(inner_off) = find_qualifying_type_offset(
                inner_trimmed,
                false,
                ignore_class,
                ignore_attribute,
                ignore_id,
              ) {
                let leading = inner_part.len() - inner_part.trim_start().len();
                let byte_off = char_offset_to_byte_offset(selector, paren_idx + 1)
                  + inner_pos
                  + leading
                  + inner_off;
                return Some(byte_off);
              }
            }

            // 2) Check if any inner arg is a bare type selector
            //    AND the compound has non-type selectors qualifying it.
            //    e.g. [attribute]:is(a) or .class:is(a)
            let has_qualifier = (has_class && !ignore_class)
              || (has_id && !ignore_id)
              || (has_attribute && !ignore_attribute);
            if has_qualifier {
              for (inner_pos, inner_part) in split_selector_list(&inner) {
                let inner_trimmed = inner_part.trim();
                if inner_trimmed.is_empty() {
                  continue;
                }
                if is_bare_type_selector(inner_trimmed) {
                  let leading = inner_part.len() - inner_part.trim_start().len();
                  let byte_off =
                    char_offset_to_byte_offset(selector, paren_idx + 1) + inner_pos + leading;
                  return Some(byte_off);
                }
              }
            }
          } else {
            // Non-selector functional pseudo — skip args
            i += 1;
            let mut depth = 1;
            while i < len && depth > 0 {
              if chars[i] == '(' {
                depth += 1;
              } else if chars[i] == ')' {
                depth -= 1;
              }
              i += 1;
            }
          }
        }
        // After pseudo, the type still qualifies what follows
      } else if chars[i] == '*' {
        i += 1;
      } else if chars[i] == '&' {
        i += 1;
        // Consume trailing ident chars (BEM-style &--modifier)
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
      } else {
        // Unknown char (e.g. Less `@{var}`) — break out of compound
        break;
      }
    }

    // Safety: ensure `i` always advances to avoid infinite loops
    // with unrecognised characters (Less interpolation, etc.)
    if i == compound_start {
      i += 1;
    }
  }
  None
}

/// Convert a char offset to a byte offset in the given string.
fn char_offset_to_byte_offset(s: &str, char_offset: usize) -> usize {
  s.char_indices()
    .nth(char_offset)
    .map(|(byte_idx, _)| byte_idx)
    .unwrap_or(s.len())
}

/// Check if the last compound selector in a selector string ends with (or is)
/// a type selector. Used to determine if `&` in nested rules resolves to a type.
fn selector_ends_with_type(selector: &str) -> bool {
  let trimmed = selector.trim();
  if trimmed.is_empty() {
    return false;
  }
  // Find the last compound selector (after last combinator at top level)
  let parts = split_selector_list(trimmed);
  // Use the last comma-separated selector
  if let Some((_pos, last_sel)) = parts.last() {
    let last_sel = last_sel.trim();
    if last_sel.is_empty() {
      return false;
    }
    // Find the last compound selector within this (after last space/combinator)
    let chars: Vec<char> = last_sel.chars().collect();
    let len = chars.len();
    let mut last_compound_start = 0;
    let mut i = 0;
    let mut depth = 0;
    while i < len {
      match chars[i] {
        '(' => depth += 1,
        ')' if depth > 0 => depth -= 1,
        ' ' | '>' | '+' | '~' if depth == 0 => {
          // Skip whitespace/combinators
          while i < len
            && (chars[i].is_ascii_whitespace()
              || chars[i] == '>'
              || chars[i] == '+'
              || chars[i] == '~')
          {
            i += 1;
          }
          if i < len {
            last_compound_start = i;
          }
          continue;
        }
        _ => {}
      }
      i += 1;
    }
    // Check if the last compound starts with a type selector
    if last_compound_start < len {
      return is_type_selector_start(&chars, last_compound_start);
    }
  }
  false
}

/// Check if a selector string is a bare type selector (e.g. `a`, `div`).
/// Used to detect when `:is(a)` contains a type that is qualified by outer selectors.
fn is_bare_type_selector(selector: &str) -> bool {
  let trimmed = selector.trim();
  if trimmed.is_empty() {
    return false;
  }
  let chars: Vec<char> = trimmed.chars().collect();
  if !is_type_selector_start(&chars, 0) {
    return false;
  }
  // All characters should be ident chars
  chars.iter().all(|c| is_ident_char(*c))
}

fn is_type_selector_start(chars: &[char], i: usize) -> bool {
  let ch = chars[i];
  if !ch.is_ascii_alphabetic() && ch.is_ascii() {
    return false;
  }
  // Must not be preceded by '.', '#', or ':' (which would mean it's part of a class/id/pseudo)
  if i > 0 {
    let prev = chars[i - 1];
    if prev == '.' || prev == '#' || prev == ':' {
      return false;
    }
  }
  true
}

fn is_ident_char(ch: char) -> bool {
  ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii()
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_type_with_class() {
    let node = style_with_selector("a.foo");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("a.foo"));
  }

  #[test]
  fn reports_type_with_id() {
    let node = style_with_selector("div#bar");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_class_only() {
    let node = style_with_selector(".foo");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_type_only() {
    let node = style_with_selector("div");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_type_with_pseudo_then_id() {
    // div:hover#thing should be reported
    let node = style_with_selector("div:hover#thing");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx());
    assert_eq!(d.len(), 1, "should report div:hover#thing as qualifying");
  }

  #[test]
  fn ignore_option_id() {
    let ctx_with_ignore = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&serde_json::json!({"ignore": ["id"]})),
    };
    let node = style_with_selector("div#bar");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx_with_ignore);
    assert!(
      d.is_empty(),
      "should ignore id qualifying when ignore: [\"id\"]"
    );
  }

  #[test]
  fn ignore_option_class() {
    let ctx_with_ignore = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&serde_json::json!({"ignore": ["class"]})),
    };
    let node = style_with_selector("ul.list");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx_with_ignore);
    assert!(
      d.is_empty(),
      "should ignore class qualifying when ignore: [\"class\"]"
    );
  }

  #[test]
  fn reports_nesting_ampersand_with_class() {
    // a { &.class {} } — the & resolves to the parent type `a`
    let parent = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      children: vec![StyleRule {
        selector: "&.class".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 15),
          important: false,
        }],
        span: ParserSpan::new(4, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 20),

      nested_at_rules: Vec::new(),
    });
    let d = SelectorNoQualifyingType.check_root(&[parent], &ctx());
    assert_eq!(
      d.len(),
      1,
      "should report &.class when parent is type selector"
    );
  }

  #[test]
  fn no_infinite_loop_on_is() {
    // Ensure :is(a, b) does not hang
    let node = style_with_selector(":is(a, b):is(c, d)");
    let d = SelectorNoQualifyingType.check_root(&[node], &ctx());
    // No qualifying type here
    assert!(d.is_empty());
  }

  #[test]
  fn split_respects_parens() {
    let parts = split_selector_list("a, :is(b, c), d");
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0].1, "a");
    assert_eq!(parts[1].1, " :is(b, c)");
    assert_eq!(parts[2].1, " d");
  }

  #[test]
  fn skips_nested_in_scss_interpolation_parent() {
    // a.foo nested inside .#{$var} should be skipped in SCSS mode
    let scss_ctx = RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    };
    let parent = CssNode::Style(StyleRule {
      selector: ".#{$var}".to_string(),
      declarations: vec![],
      children: vec![StyleRule {
        selector: "a.foo".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        }],
        span: ParserSpan::new(0, 0),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 0),

      nested_at_rules: Vec::new(),
    });
    let d = SelectorNoQualifyingType.check_root(&[parent], &scss_ctx);
    assert!(
      d.is_empty(),
      "should skip rules nested inside SCSS interpolation parent"
    );
  }
}
