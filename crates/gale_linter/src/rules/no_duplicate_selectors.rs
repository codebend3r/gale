use std::collections::HashMap;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, SourceLineIndex, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate selectors within a stylesheet.
///
/// Equivalent to Stylelint's `no-duplicate-selectors` rule.
pub struct NoDuplicateSelectors;

impl Rule for NoDuplicateSelectors {
  fn name(&self) -> &'static str {
    "no-duplicate-selectors"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate selectors within a stylesheet"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let line_index = SourceLineIndex::build(context.source);
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut diagnostics = Vec::new();
    let is_scss = matches!(context.syntax, Syntax::Scss | Syntax::Sass | Syntax::Less);

    // Parse ignoreSelectors option (v17): array of string/regex patterns
    let ignore_selectors: Vec<String> = context
      .secondary_options()
      .and_then(|v| v.get("ignoreSelectors"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|item| item.as_str().map(|s| s.to_string()))
          .collect()
      })
      .unwrap_or_default();

    collect_selectors(
      nodes,
      &[],
      &mut seen,
      &mut diagnostics,
      self,
      &line_index,
      context.source,
      is_scss,
      &ignore_selectors,
    );

    diagnostics
  }
}

/// Split a selector list by commas (respecting parentheses), returning trimmed parts.
/// Matches PostCSS `list.comma` behavior which trims all elements.
fn split_and_trim(selector: &str) -> Vec<String> {
  let mut parts = Vec::new();
  let mut depth: usize = 0;
  let mut start = 0;
  let bytes = selector.as_bytes();
  for (i, &b) in bytes.iter().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => depth = depth.saturating_sub(1),
      b',' if depth == 0 => {
        let part = selector[start..i].trim().to_string();
        if !part.is_empty() {
          parts.push(part);
        }
        start = i + 1;
      }
      _ => {}
    }
  }
  let last = selector[start..].trim().to_string();
  if !last.is_empty() {
    parts.push(last);
  }
  parts
}

/// Expand a SCSS/Less child selector against parent selectors.
/// Uses trimmed child parts (matching PostCSS `list.comma` behavior).
fn expand_selector(parents: &[String], child: &str) -> Vec<String> {
  let child_parts = split_and_trim(child);
  let mut results = Vec::new();
  for child_part in &child_parts {
    if child_part.is_empty() {
      continue;
    }
    if child_part.contains('&') {
      for parent in parents {
        let expanded = child_part.replace('&', parent.as_str());
        results.push(expanded);
      }
    } else {
      for parent in parents {
        results.push(format!("{} {}", parent, child_part));
      }
    }
  }
  results
}

/// Normalize a set of expanded selectors for duplicate comparison.
/// Sorts the selectors and collapses whitespace.
fn normalize_expanded(selectors: &[String]) -> String {
  let mut normalized: Vec<String> = selectors
    .iter()
    .map(|s| collapse_whitespace(s.trim()))
    .collect();
  normalized.sort_unstable();
  normalized.join(", ")
}

/// Collapse runs of whitespace into a single space.
fn collapse_whitespace(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let mut prev_was_space = false;
  for ch in s.chars() {
    if ch.is_whitespace() {
      if !prev_was_space {
        result.push(' ');
        prev_was_space = true;
      }
    } else {
      result.push(ch);
      prev_was_space = false;
    }
  }
  result
}

/// Walk CSS/SCSS/Less nodes recursively, checking for duplicate selectors.
///
/// `parent_selectors`: expanded parent selectors for `&` substitution (SCSS).
/// `seen`: maps normalized expanded selector → first occurrence line number.
/// At-rules create fresh `seen` scopes (matching Stylelint's `nodeContextLookup`).
fn collect_selectors(
  nodes: &[CssNode],
  parent_selectors: &[String],
  seen: &mut HashMap<String, usize>,
  diagnostics: &mut Vec<Diagnostic>,
  rule: &NoDuplicateSelectors,
  line_index: &SourceLineIndex,
  source: &str,
  is_scss: bool,
  ignore_selectors: &[String],
) {
  for node in nodes {
    match node {
      CssNode::Style(style_rule) => {
        let raw = style_rule.selector.trim();

        // Skip selectors with preprocessor interpolation (non-standard).
        // Stylelint's isStandardSyntaxRule returns false for these.
        let is_standard = !raw.contains("#{") && !raw.contains("@{");

        // Compute the expanded selectors for comparison.
        // Non-standard selectors still get placeholder parents for child
        // recursion (children need to know their non-standard parent context).
        let expanded: Vec<String> = if is_scss && !parent_selectors.is_empty() {
          expand_selector(parent_selectors, raw)
        } else {
          // Top-level rule or CSS: use the selector parts as-is.
          split_and_trim(raw)
        };

        // Only check standard selectors (matching Stylelint's isStandardSyntaxRule).
        // Also skip if any expanded selector contains SCSS interpolation — postcss-selector-parser
        // fails to parse such selectors in Stylelint, causing no-duplicate-selectors to skip them.
        let expanded_is_standard = !expanded
          .iter()
          .any(|s| s.contains("#{") || s.contains("@{"));
        // Check ignoreSelectors: skip if any selector part matches an ignore pattern
        let should_ignore = !ignore_selectors.is_empty()
          && expanded.iter().any(|sel| {
            ignore_selectors.iter().any(|pattern| {
              if pattern.starts_with('/') && pattern.ends_with('/') {
                // Regex pattern
                let re_str = &pattern[1..pattern.len() - 1];
                regex::Regex::new(re_str)
                  .map(|re| re.is_match(sel))
                  .unwrap_or(false)
              } else {
                sel == pattern
              }
            })
          });
        if is_standard && expanded_is_standard && !expanded.is_empty() && !should_ignore {
          let normalized_key = normalize_expanded(&expanded);
          let (line, _) = line_index.offset_to_location(style_rule.span.offset);

          if let Some(&first_line) = seen.get(&normalized_key) {
            diagnostics.push(
              Diagnostic::new(
                rule.name(),
                format!(
                  "Unexpected duplicate selector \"{}\", first used at line {}",
                  raw, first_line,
                ),
              )
              .severity(rule.default_severity())
              .span(Span::new(style_rule.span.offset, style_rule.span.length)),
            );
          } else {
            seen.insert(normalized_key, line);
          }
        }

        // Compute parent selectors for child recursion.
        // For SCSS, pass the expanded versions (or the raw selector if
        // at the top level / non-expandable).
        let new_parents: Vec<String> = if is_scss {
          if parent_selectors.is_empty() {
            split_and_trim(raw)
          } else {
            expand_selector(parent_selectors, raw)
          }
        } else {
          vec![]
        };

        // Recurse into nested style rules (SCSS nesting).
        let children_nodes: Vec<CssNode> = style_rule
          .children
          .iter()
          .map(|c| CssNode::Style(c.clone()))
          .collect();
        collect_selectors(
          &children_nodes,
          &new_parents,
          seen,
          diagnostics,
          rule,
          line_index,
          source,
          is_scss,
          ignore_selectors,
        );

        // Recurse into nested at-rules (e.g. `@media` inside a rule).
        collect_selectors(
          &style_rule.nested_at_rules,
          &new_parents,
          seen,
          diagnostics,
          rule,
          line_index,
          source,
          is_scss,
          ignore_selectors,
        );
      }
      CssNode::AtRule(at_rule) => {
        // Each at-rule establishes a fresh duplicate-detection scope,
        // matching Stylelint's `nodeContextLookup` behaviour.
        // Pass the current parent_selectors through (at-rules don't reset
        // the `&` context — only the duplicate-detection scope resets).
        let mut scoped_seen = HashMap::new();
        collect_selectors(
          &at_rule.children,
          parent_selectors,
          &mut scoped_seen,
          diagnostics,
          rule,
          line_index,
          source,
          is_scss,
          ignore_selectors,
        );
      }
      _ => {}
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_duplicate_selectors_with_line_reference() {
    let rule = NoDuplicateSelectors;
    let source = ".foo { color: red; }\n.bar { color: blue; }\n.foo { display: block; }";
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(6, 10),
          important: false,
        }],
        span: ParserSpan::new(0, 20),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(27, 11),
          important: false,
        }],
        span: ParserSpan::new(21, 21),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![Declaration {
          property: "display".to_string(),
          value: "block".to_string(),
          span: ParserSpan::new(49, 14),
          important: false,
        }],
        span: ParserSpan::new(43, 24),
        ..Default::default()
      }),
    ];
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate selector \".foo\", first used at line 1"
    );
  }

  #[test]
  fn ignores_unique_selectors() {
    let rule = NoDuplicateSelectors;
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 10),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".bar".to_string(),
        declarations: vec![],
        span: ParserSpan::new(11, 10),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn scoped_selectors_in_at_rules_dont_clash() {
    let rule = NoDuplicateSelectors;
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 10),
        ..Default::default()
      }),
      CssNode::AtRule(gale_css_parser::AtRule {
        name: "media".to_string(),
        params: "(min-width: 768px)".to_string(),
        span: ParserSpan::new(11, 50),
        children: vec![CssNode::Style(StyleRule {
          selector: ".foo".to_string(),
          declarations: vec![],
          span: ParserSpan::new(30, 10),
          ..Default::default()
        })],
      }),
    ];
    let diags = rule.check_root(&nodes, &make_context());
    assert!(
      diags.is_empty(),
      "same selector in different scopes should not be flagged"
    );
  }

  #[test]
  fn detects_duplicates_in_parsed_scss() {
    let scss = ".foo { color: red; }\n.bar { color: blue; }\n.foo { display: block; }";
    let result = gale_css_parser::parse(scss, Syntax::Scss).expect("should parse SCSS");
    let rule = NoDuplicateSelectors;
    let ctx = RuleContext {
      file_path: "test.scss",
      source: scss,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags = rule.check_root(&result.nodes, &ctx);
    assert_eq!(diags.len(), 1, "should detect duplicate .foo in SCSS");
  }

  #[test]
  fn detects_nested_scss_duplicate_ampersand() {
    let scss = ".parent {\n  & { color: red; }\n  & { color: blue; }\n}";
    let result = gale_css_parser::parse(scss, Syntax::Scss).expect("should parse SCSS");
    let rule = NoDuplicateSelectors;
    let ctx = RuleContext {
      file_path: "test.scss",
      source: scss,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags = rule.check_root(&result.nodes, &ctx);
    // Each `& {}` expands to `.parent`, which duplicates the parent rule.
    // Stylelint reports both as duplicates, so we expect 2 diagnostics.
    assert_eq!(
      diags.len(),
      2,
      "each nested & should be flagged as duplicate of parent"
    );
  }

  #[test]
  fn ignore_selectors_string_match() {
    let rule = NoDuplicateSelectors;
    let source = ".foo { color: red; }\n.bar { color: blue; }\n.foo { display: block; }";
    let opts = serde_json::json!([true, {"ignoreSelectors": [".foo"]}]);
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 20),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".bar".to_string(),
        declarations: vec![],
        span: ParserSpan::new(21, 21),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![],
        span: ParserSpan::new(43, 24),
        ..Default::default()
      }),
    ];
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let diags = rule.check_root(&nodes, &ctx);
    assert!(
      diags.is_empty(),
      "ignoreSelectors should suppress duplicate .foo"
    );
  }

  #[test]
  fn ignore_selectors_regex_match() {
    let rule = NoDuplicateSelectors;
    let source = ".foo { color: red; }\n.foo { display: block; }";
    let opts = serde_json::json!([true, {"ignoreSelectors": ["/^\\.foo$/"]}]);
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 20),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".foo".to_string(),
        declarations: vec![],
        span: ParserSpan::new(21, 24),
        ..Default::default()
      }),
    ];
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let diags = rule.check_root(&nodes, &ctx);
    assert!(
      diags.is_empty(),
      "ignoreSelectors with regex should suppress duplicate .foo"
    );
  }
}
