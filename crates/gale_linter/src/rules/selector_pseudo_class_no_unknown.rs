use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::data::is_known_pseudo_class;
use crate::rule::{Rule, RuleContext};

/// Known pseudo-classes that are only valid in a `@page` context.
/// These should NOT be recognized as valid pseudo-classes in regular selectors.
const PAGE_PSEUDO_CLASSES: &[&str] = &["blank", "first", "left", "nth", "right", "recto", "verso"];

/// Pseudo-classes that are only valid after vendor-prefixed pseudo-elements
/// (e.g., `::-webkit-scrollbar-thumb:window-inactive`).
const WEBKIT_SCROLLBAR_PSEUDO_CLASSES: &[&str] = &[
  "corner-present",
  "decrement",
  "double-button",
  "end",
  "horizontal",
  "increment",
  "no-button",
  "single-button",
  "start",
  "vertical",
  "window-inactive",
];

fn is_page_pseudo_class(name: &str) -> bool {
  PAGE_PSEUDO_CLASSES
    .iter()
    .any(|p| p.eq_ignore_ascii_case(name))
}

fn is_webkit_scrollbar_pseudo_class(name: &str) -> bool {
  WEBKIT_SCROLLBAR_PSEUDO_CLASSES
    .iter()
    .any(|p| p.eq_ignore_ascii_case(name))
}

pub struct SelectorPseudoClassNoUnknown;

impl Rule for SelectorPseudoClassNoUnknown {
  fn name(&self) -> &'static str {
    "selector-pseudo-class-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown pseudo-class selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore_list = parse_ignore_list(ctx.options);

    match node {
      CssNode::Style(rule) => {
        if rule.selector.contains("#{") || rule.selector.contains("@{") {
          return vec![];
        }
        let mut diags = Vec::new();
        let entries = extract_pseudo_classes_with_context(&rule.selector);
        for entry in &entries {
          if entry.name.starts_with('-') {
            continue;
          }
          if matches!(
            entry.name.as_str(),
            "before" | "after" | "first-line" | "first-letter"
          ) {
            continue;
          }
          // Webkit scrollbar pseudo-classes are valid after pseudo-elements
          if entry.after_pseudo_element && is_webkit_scrollbar_pseudo_class(&entry.name) {
            continue;
          }
          // Page pseudo-classes are NOT valid in regular selectors
          if is_page_pseudo_class(&entry.name) {
            let span = find_pseudo_class_span(ctx.source, rule.span.offset, entry);
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Unexpected unknown pseudo-class selector \":{}\"",
                  entry.name
                ),
              )
              .severity(self.default_severity())
              .span(span),
            );
            continue;
          }
          if is_ignored(&entry.name, &ignore_list) {
            continue;
          }
          if !is_known_pseudo_class(&entry.name) {
            let span = find_pseudo_class_span(ctx.source, rule.span.offset, entry);
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Unexpected unknown pseudo-class selector \":{}\"",
                  entry.name
                ),
              )
              .severity(self.default_severity())
              .span(span),
            );
          }
        }
        diags
      }
      CssNode::AtRule(at) => {
        if at.name != "page" {
          return vec![];
        }
        let params = at.params.trim();
        if params.is_empty() {
          return vec![];
        }

        let mut diags = Vec::new();
        let entries = extract_pseudo_classes_with_context(params);
        for entry in &entries {
          if entry.name.starts_with('-') {
            continue;
          }
          if is_page_pseudo_class(&entry.name) {
            continue;
          }
          if entry.name == "nth" {
            continue;
          }
          if is_ignored(&entry.name, &ignore_list) {
            continue;
          }
          if !is_known_pseudo_class(&entry.name) {
            let span = find_page_pseudo_span(ctx.source, at.span.offset, entry);
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Unexpected unknown pseudo-class selector \":{}\"",
                  entry.name
                ),
              )
              .severity(self.default_severity())
              .span(span),
            );
          }
        }
        diags
      }
      _ => vec![],
    }
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore_list = parse_ignore_list(ctx.options);
    let mut diags = Vec::new();
    let source = ctx.source;

    for entry in scan_source_for_unparsed_pseudo_classes(source, nodes) {
      if entry.name.starts_with('-') {
        continue;
      }
      if matches!(
        entry.name.as_str(),
        "before" | "after" | "first-line" | "first-letter"
      ) {
        continue;
      }
      if entry.after_pseudo_element && is_webkit_scrollbar_pseudo_class(&entry.name) {
        continue;
      }
      if is_ignored(&entry.name, &ignore_list) {
        continue;
      }
      if !is_known_pseudo_class(&entry.name) && !is_page_pseudo_class(&entry.name) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Unexpected unknown pseudo-class selector \":{}\"",
              entry.name
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(entry.byte_offset_in_source, entry.name.len() + 1)),
        );
      }
    }

    diags
  }
}

struct PseudoClassEntry {
  name: String,
  #[allow(dead_code)]
  char_offset_in_selector: usize,
  after_pseudo_element: bool,
  byte_offset_in_source: usize,
}

/// Compute the span for a pseudo-class diagnostic, pointing at the colon.
fn find_pseudo_class_span(source: &str, rule_offset: usize, entry: &PseudoClassEntry) -> Span {
  let selector_source = &source[rule_offset..];
  let search_pattern = format!(":{}", entry.name);
  let search_pattern_lower = search_pattern.to_ascii_lowercase();

  let mut search_start = 0;
  while search_start < selector_source.len() {
    let remaining = &selector_source[search_start..];
    if let Some(pos) = remaining.to_ascii_lowercase().find(&search_pattern_lower) {
      let abs_pos = rule_offset + search_start + pos;
      // Make sure it's not a `::` pseudo-element
      if pos > 0 && remaining.as_bytes().get(pos.wrapping_sub(1)) == Some(&b':') {
        search_start += pos + search_pattern.len();
        continue;
      }
      return Span::new(abs_pos, search_pattern.len());
    } else {
      break;
    }
  }

  Span::new(rule_offset, entry.name.len() + 1)
}

/// Compute span for a pseudo-class in an @page params.
fn find_page_pseudo_span(source: &str, at_offset: usize, entry: &PseudoClassEntry) -> Span {
  let search_pattern = format!(":{}", entry.name);
  let search_pattern_lower = search_pattern.to_ascii_lowercase();

  let at_source = &source[at_offset..];
  if let Some(pos) = at_source.to_ascii_lowercase().find(&search_pattern_lower) {
    if pos > 0 && at_source.as_bytes().get(pos - 1) == Some(&b':') {
      return Span::new(at_offset, search_pattern.len());
    }
    return Span::new(at_offset + pos, search_pattern.len());
  }

  Span::new(at_offset, search_pattern.len())
}

/// Extract pseudo-class names from a selector string, with context.
fn extract_pseudo_classes_with_context(selector: &str) -> Vec<PseudoClassEntry> {
  let mut classes = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut after_pseudo_element = false;

  while i < len {
    if i + 1 < len && chars[i] == '#' && chars[i + 1] == '{' {
      i += 2;
      let mut depth = 1;
      while i < len && depth > 0 {
        if chars[i] == '{' {
          depth += 1;
        } else if chars[i] == '}' {
          depth -= 1;
        }
        i += 1;
      }
      continue;
    }

    if chars[i] == '[' {
      let mut depth = 1;
      i += 1;
      while i < len && depth > 0 {
        match chars[i] {
          '[' => depth += 1,
          ']' => depth -= 1,
          '"' | '\'' => {
            let quote = chars[i];
            i += 1;
            while i < len && chars[i] != quote {
              if chars[i] == '\\' {
                i += 1;
              }
              i += 1;
            }
          }
          _ => {}
        }
        i += 1;
      }
      continue;
    }

    if chars[i] == ',' || chars[i] == ' ' || chars[i] == '\n' || chars[i] == '\t' {
      if chars[i] == ',' {
        after_pseudo_element = false;
      }
      i += 1;
      continue;
    }

    if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      after_pseudo_element = true;
      continue;
    }

    if chars[i] == ':' {
      let colon_pos = i;
      i += 1;
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        classes.push(PseudoClassEntry {
          name,
          char_offset_in_selector: colon_pos,
          after_pseudo_element,
          byte_offset_in_source: 0,
        });
      }
      if i < len && chars[i] == '(' {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if chars[i] == '(' {
            depth += 1;
          } else if chars[i] == ')' {
            depth -= 1;
          }
          i += 1;
        }
      }
    } else {
      i += 1;
    }
  }

  classes
}

/// Scan the full source text for pseudo-classes in selectors that the parser
/// might have dropped (e.g., webkit scrollbar selectors).
fn scan_source_for_unparsed_pseudo_classes(
  source: &str,
  nodes: &[CssNode],
) -> Vec<PseudoClassEntry> {
  let mut covered_ranges: Vec<(usize, usize)> = Vec::new();
  collect_node_ranges(nodes, &mut covered_ranges);
  let covered_ranges = merge_ranges(covered_ranges);

  let mut results = Vec::new();
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if let Some(end) = covered_range_end(i, &covered_ranges) {
      i = end;
      continue;
    }

    // Skip comments
    if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
      i += 2;
      while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
      }
      if i + 1 < len {
        i += 2;
      }
      continue;
    }

    // Look for :: pseudo-element followed by : pseudo-classes
    if i + 1 < len && bytes[i] == b':' && bytes[i + 1] == b':' {
      i += 2;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
        i += 1;
      }
      while i < len && bytes[i] == b':' && (i + 1 >= len || bytes[i + 1] != b':') {
        let colon_offset = i;
        i += 1;
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
          i += 1;
        }
        if i > name_start {
          let name = String::from_utf8_lossy(&bytes[name_start..i]).to_string();
          results.push(PseudoClassEntry {
            name,
            char_offset_in_selector: 0,
            after_pseudo_element: true,
            byte_offset_in_source: colon_offset,
          });
        }
        if i < len && bytes[i] == b'(' {
          let mut depth = 1;
          i += 1;
          while i < len && depth > 0 {
            if bytes[i] == b'(' {
              depth += 1;
            } else if bytes[i] == b')' {
              depth -= 1;
            }
            i += 1;
          }
        }
      }
      continue;
    }

    // Look for @page pseudo-classes that the parser dropped
    if i + 5 < len && &source[i..i + 5] == "@page" {
      // Skip the @page keyword
      let page_start = i;
      i += 5;
      // Skip whitespace
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }
      // Find end of @page selector (opening brace)
      let selector_start = i;
      let mut end = i;
      while end < len && bytes[end] != b'{' {
        end += 1;
      }
      if end < len && !is_in_covered_range(page_start, &covered_ranges) {
        let selector = &source[selector_start..end].trim();
        if !selector.is_empty() {
          let entries = extract_pseudo_classes_with_context(selector);
          for entry in entries {
            let byte_offset = selector_start
              + selector
                .char_indices()
                .nth(entry.char_offset_in_selector)
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            results.push(PseudoClassEntry {
              name: entry.name,
              char_offset_in_selector: entry.char_offset_in_selector,
              after_pseudo_element: entry.after_pseudo_element,
              byte_offset_in_source: byte_offset,
            });
          }
        }
      }
      i = if end < len { end + 1 } else { end };
      continue;
    }

    // Look for standalone pseudo-classes at selector boundaries
    if bytes[i] == b':' && (i + 1 >= len || bytes[i + 1] != b':') {
      let before = if i > 0 {
        let mut j = i - 1;
        while j > 0 && bytes[j].is_ascii_whitespace() {
          j -= 1;
        }
        bytes[j]
      } else {
        b'\n'
      };

      let is_selector_start =
        before == b'}' || before == b';' || before == b'\n' || before == b'\r' || i == 0;

      if is_selector_start && !is_in_covered_range(i, &covered_ranges) {
        let selector_start = i;
        let mut end = i;
        while end < len && bytes[end] != b'{' {
          end += 1;
        }
        if end < len {
          let selector_text = &source[selector_start..end];
          let entries = extract_pseudo_classes_with_context(selector_text);
          for entry in entries {
            let byte_offset = selector_start
              + selector_text
                .char_indices()
                .nth(entry.char_offset_in_selector)
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            results.push(PseudoClassEntry {
              name: entry.name,
              char_offset_in_selector: entry.char_offset_in_selector,
              after_pseudo_element: entry.after_pseudo_element,
              byte_offset_in_source: byte_offset,
            });
          }
        }
        i = if end < len { end + 1 } else { end };
        continue;
      }
    }

    i += 1;
  }

  results
}

fn collect_node_ranges(nodes: &[CssNode], ranges: &mut Vec<(usize, usize)>) {
  for node in nodes {
    match node {
      CssNode::Style(rule) => {
        if rule.span.length > 0 {
          ranges.push((rule.span.offset, rule.span.offset + rule.span.length));
        }
        collect_style_children(&rule.children, ranges);
      }
      CssNode::AtRule(at) => {
        if at.span.length > 0 {
          ranges.push((at.span.offset, at.span.offset + at.span.length));
        }
        collect_node_ranges(&at.children, ranges);
      }
      CssNode::Comment(c) => {
        if c.span.length > 0 {
          ranges.push((c.span.offset, c.span.offset + c.span.length));
        }
      }
      CssNode::Declaration(d) => {
        if d.span.length > 0 {
          ranges.push((d.span.offset, d.span.offset + d.span.length));
        }
      }
    }
  }
}

fn collect_style_children(
  children: &[gale_css_parser::StyleRule],
  ranges: &mut Vec<(usize, usize)>,
) {
  for child in children {
    if child.span.length > 0 {
      ranges.push((child.span.offset, child.span.offset + child.span.length));
    }
    collect_style_children(&child.children, ranges);
  }
}

/// Sort ranges and merge any that overlap or nest (a rule's span contains
/// its declarations' spans) into disjoint intervals, so lookups can binary
/// search instead of scanning every node range for every byte.
fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
  ranges.sort_unstable();
  let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
  for (start, end) in ranges {
    match merged.last_mut() {
      Some(last) if start <= last.1 => last.1 = last.1.max(end),
      _ => merged.push((start, end)),
    }
  }
  merged
}

/// If `offset` falls inside one of the merged ranges, return that range's end.
fn covered_range_end(offset: usize, ranges: &[(usize, usize)]) -> Option<usize> {
  let idx = ranges.partition_point(|&(start, _)| start <= offset);
  match idx.checked_sub(1) {
    Some(i) if offset < ranges[i].1 => Some(ranges[i].1),
    _ => None,
  }
}

fn is_in_covered_range(offset: usize, ranges: &[(usize, usize)]) -> bool {
  covered_range_end(offset, ranges).is_some()
}

/// Parse `ignorePseudoClasses` from the secondary options.
fn parse_ignore_list(options: Option<&serde_json::Value>) -> Vec<String> {
  let Some(opts) = options else {
    return vec![];
  };
  let obj = match opts {
    serde_json::Value::Object(o) => o,
    serde_json::Value::Array(arr) => {
      for item in arr {
        if let serde_json::Value::Object(o) = item
          && let Some(serde_json::Value::Array(names)) = o.get("ignorePseudoClasses")
        {
          return names
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        }
      }
      return vec![];
    }
    _ => return vec![],
  };
  if let Some(serde_json::Value::Array(names)) = obj.get("ignorePseudoClasses") {
    names
      .iter()
      .filter_map(|v| v.as_str().map(String::from))
      .collect()
  } else {
    vec![]
  }
}

fn is_ignored(name: &str, ignore_list: &[String]) -> bool {
  for pattern in ignore_list {
    if let Some(re) = parse_regex_pattern(pattern) {
      if re.is_match(name) {
        return true;
      }
    } else if pattern == name {
      return true;
    }
  }
  false
}

fn parse_regex_pattern(s: &str) -> Option<Regex> {
  if let Some(rest) = s.strip_prefix('/') {
    if let Some(end) = rest.rfind('/') {
      let pattern = &rest[..end];
      let flags = &rest[end + 1..];
      let full_pattern = if flags.contains('i') {
        format!("(?i){pattern}")
      } else {
        pattern.to_string()
      };
      Regex::new(&full_pattern).ok()
    } else {
      None
    }
  } else {
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{CssNode, Declaration, Span as ParserSpan, StyleRule, Syntax};

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
  fn reports_unknown_pseudo_class() {
    let d = SelectorPseudoClassNoUnknown.check(&style_with_selector("a:hoverr"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(":hoverr"));
  }

  #[test]
  fn allows_known_pseudo_class() {
    assert!(
      SelectorPseudoClassNoUnknown
        .check(&style_with_selector("a:hover"), &ctx())
        .is_empty()
    );
    assert!(
      SelectorPseudoClassNoUnknown
        .check(&style_with_selector("a:nth-child(2)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn does_not_confuse_with_pseudo_elements() {
    assert!(
      SelectorPseudoClassNoUnknown
        .check(&style_with_selector("a::before"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_legacy_single_colon_pseudo_elements() {
    for sel in ["p:before", "p:after", "p:first-line", "p:first-letter"] {
      assert!(
        SelectorPseudoClassNoUnknown
          .check(&style_with_selector(sel), &ctx())
          .is_empty(),
        "should not flag legacy pseudo-element in selector \"{sel}\"",
      );
    }
  }

  #[test]
  fn rejects_first_in_regular_selector() {
    let d = SelectorPseudoClassNoUnknown.check(&style_with_selector(":first"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(":first"));
  }
}
