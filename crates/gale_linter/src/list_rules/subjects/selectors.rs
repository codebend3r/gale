//! Rule properties and the pieces of selectors.

use super::*;

/// Every declaration directly in a standard style rule (not in one nested
/// inside it), keyed by the rule's selector:
/// `rule-selector-property-disallowed-list`.
pub fn rule_properties(tree: &PostcssTree, ignore_keyframes: bool, found: &mut Vec<KeyedFound>) {
  for (rule, node, _) in standard_rules(tree) {
    if ignore_keyframes
      && tree
        .node(rule)
        .as_rule()
        .is_some_and(RuleRef::is_keyframe_rule)
    {
      continue;
    }
    for (i, decl) in nodes_of(tree, NodeKind::Decl) {
      if nearest_rule(tree, i) == Some(rule) {
        found.push(KeyedFound::new(
          node.name.as_str(),
          decl.name.as_str(),
          decl.name_span.start,
          decl.name_span.end,
        ));
      }
    }
  }
}

/// The closest style rule enclosing node `i`.
fn nearest_rule(tree: &PostcssTree, i: usize) -> Option<usize> {
  let mut parent = tree.nodes[i].parent;
  while let Some(p) = parent {
    if tree.nodes[p].kind == NodeKind::Rule {
      return Some(p);
    }
    parent = tree.nodes[p].parent;
  }
  None
}

/// What `selector-disallowed-list` skips and how it splits selectors.
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectorOptions {
  /// `ignore: ["inside-block"]`: skip rules nested in another block.
  pub ignore_inside_block: bool,
  /// `ignore: ["keyframe-selectors"]`: skip the selectors in `@keyframes`.
  pub ignore_keyframes: bool,
  /// `splitList`: match each selector of a list on its own.
  pub split_list: bool,
}

/// The selector of every standard style rule, whole or split at its
/// commas: `selector-disallowed-list`.
pub fn selectors(tree: &PostcssTree, options: SelectorOptions, found: &mut Vec<Found>) {
  for (i, node, raw) in standard_rules(tree) {
    if (options.ignore_keyframes
      && tree
        .node(i)
        .as_rule()
        .is_some_and(RuleRef::is_keyframe_rule))
      || (options.ignore_inside_block && node.parent.is_some())
    {
      continue;
    }
    let base = node.name_span.start;
    if !options.split_list {
      found.push(Found::new(raw, i, base, base + raw.len()));
      continue;
    }
    for item in selector::parse(raw, base).unwrap_or_default() {
      let first = item.nodes.iter().find(|n| n.kind != Kind::Comment);
      let last = item.nodes.iter().rev().find(|n| n.kind != Kind::Comment);
      let (start, end) = match (first, last) {
        (Some(first), Some(last)) => (first.start, last.end),
        _ => (item.start, item.start),
      };
      let text = tree.source().get(start..end).unwrap_or("").trim();
      found.push(Found::new(text, i, start, start + text.len()));
    }
  }
}

/// Call `visit` with every node of `selector`, which starts at `base` in
/// the source and belongs to statement `i`.  A selector that does not parse
/// is skipped, as Stylelint skips one postcss-selector-parser rejects.
fn walk_selector(
  i: usize,
  selector: &str,
  base: usize,
  visit: &mut dyn FnMut(usize, &selector::Visit),
) {
  if let Some(parsed) = selector::parse(selector, base) {
    selector::walk(&parsed, &mut |node| visit(i, node));
  }
}

/// Call `visit` with every selector node of the standard style rules whose
/// selector passes `marker` (Stylelint's `mayIncludeRegexes` filter).
fn walk_rule_selectors(
  tree: &PostcssTree,
  marker: fn(&str) -> bool,
  visit: &mut dyn FnMut(usize, &selector::Visit),
) {
  for (i, node, raw) in standard_rules(tree) {
    if marker(raw) {
      walk_selector(i, raw, node.name_span.start, visit);
    }
  }
}

/// Pseudo-class names, without the colon, in style rule selectors and
/// `@page` params: `selector-pseudo-class-*-list`.
pub fn pseudo_classes(tree: &PostcssTree, found: &mut Vec<Found>) {
  let mut visit = |i: usize, visit: &selector::Visit| {
    let node = visit.node;
    if node.kind == Kind::Pseudo && !node.value.starts_with("::") {
      let name = node.value.get(1..).unwrap_or("");
      found.push(Found::new(
        name,
        i,
        node.start,
        node.start + node.value.len(),
      ));
    }
  };
  walk_rule_selectors(tree, |s| s.contains(':'), &mut visit);
  for (i, node) in at_rules_named(tree, "page") {
    let (params, base) = raw_params(tree, node);
    if node.params.contains(':') && tree.is_standard_syntax_at_rule(i) {
      walk_selector(i, params, base, &mut visit);
    }
  }
}

/// Pseudo-element names, without the colons, in style rule selectors; only
/// the `::` form counts: `selector-pseudo-element-*-list`.
pub fn pseudo_elements(tree: &PostcssTree, found: &mut Vec<Found>) {
  walk_rule_selectors(tree, |s| s.contains("::"), &mut |i, visit| {
    let node = visit.node;
    if node.kind == Kind::Pseudo && node.value.as_bytes().get(1) == Some(&b':') {
      let name = node.value.get(2..).unwrap_or("");
      found.push(Found::new(
        name,
        i,
        node.start,
        node.start + node.value.len(),
      ));
    }
  });
}

/// Where the attribute name of `[ns|name ...]` starts: past the `[`, any
/// whitespace and the namespace.
fn attribute_name_start(node: &selector::Node) -> usize {
  let inner = node.value.get(1..).unwrap_or("");
  let mut at = inner.len() - inner.trim_start().len();
  if let Some(namespace) = &node.namespace {
    at += namespace.len() + 1;
  }
  node.start + 1 + at
}

/// Attribute names, with any namespace (`ns|name`), in style rule
/// selectors: `selector-attribute-name-disallowed-list`.
pub fn attribute_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  walk_rule_selectors(tree, |s| s.contains('['), &mut |i, visit| {
    let node = visit.node;
    let Some(attr) = node.attr.as_ref().filter(|_| node.kind == Kind::Attribute) else {
      return;
    };
    let name = match &node.namespace {
      Some(namespace) => format!("{namespace}|{}", attr.name),
      None => attr.name.clone(),
    };
    let start = attribute_name_start(node);
    found.push(Found::new(name, i, start, start + attr.name.len()));
  });
}

/// Attribute operators (`=`, `~=`, `^=`, ...) in style rule selectors:
/// `selector-attribute-operator-*-list`.
pub fn attribute_operators(tree: &PostcssTree, found: &mut Vec<Found>) {
  // `mayIncludeRegexes.attributeSelectorWithOperator`: `/\[.*=/`.
  let has_operator = |s: &str| s.find('[').is_some_and(|open| s[open..].contains('='));
  walk_rule_selectors(tree, has_operator, &mut |i, visit| {
    let node = visit.node;
    let Some(attr) = node.attr.as_ref().filter(|_| node.kind == Kind::Attribute) else {
      return;
    };
    let Some(operator) = &attr.operator else {
      return;
    };
    let after_name = attribute_name_start(node) + attr.name.len() - node.start;
    let offset = node
      .value
      .get(after_name..)
      .and_then(|rest| rest.find(operator.as_str()))
      .unwrap_or(0);
    let start = node.start + after_name + offset;
    found.push(Found::new(
      operator.as_str(),
      i,
      start,
      start + operator.len(),
    ));
  });
}

/// Combinators in style rule selectors, whitespace collapsed to one space
/// (`normalizeCombinator`).  Reference combinators (`/deep/`) and those at
/// the start or end of a selector are skipped (`isStandardSyntaxCombinator`):
/// `selector-combinator-*-list`.
pub fn combinators(tree: &PostcssTree, found: &mut Vec<Found>) {
  walk_rule_selectors(tree, |_| true, &mut |i, visit| {
    let node = visit.node;
    if node.kind != Kind::Combinator
      || node.value.starts_with('/')
      || node.value.ends_with('/')
      || visit.index == 0
      || visit.index + 1 == visit.siblings.len()
    {
      return;
    }
    let mut name = String::new();
    for c in node.value.chars() {
      if !c.is_whitespace() {
        name.push(c);
      } else if !name.ends_with(' ') {
        name.push(' ');
      }
    }
    found.push(Found::new(name, i, node.start, node.end));
  });
}
