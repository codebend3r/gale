//! Stylelint's helpers for comparing selectors across nested rules:
//! `resolveNestedSelectorsForRule`, `normalizeSelector` and
//! `no-descending-specificity`'s `lastCompoundSelectorWithoutPseudoClasses`.

use std::collections::HashMap;

use gale_css_parser::postcss_tree::{NodeRef, RuleRef};
use gale_css_parser::selector::nesting::resolve_nested_list;
use gale_css_parser::selector::{
  Combinator, Pseudo, PseudoArg, Selector, SelectorList, SelectorNode, parse_selector_list,
};

use crate::data::is_known_pseudo_element;

/// Each rule's selectors resolved against its ancestors, as Stylelint's
/// `resolveNestedSelectorsForRule` resolves them, cached by node.
#[derive(Default)]
pub struct SelectorResolver {
  /// The resolved selectors by node, `None` where a selector on the way
  /// does not parse or is not standard syntax.
  resolved: HashMap<usize, Option<SelectorList>>,
}

impl SelectorResolver {
  /// The selectors of `rule` resolved against its ancestors, or `None`
  /// where Stylelint resolves none.
  pub fn resolve(&mut self, rule: RuleRef) -> Option<SelectorList> {
    if let Some(known) = self.resolved.get(&rule.id()) {
      return known.clone();
    }
    let own = parse_selector_list(&rule.raw_selector()).ok();
    let resolved = own.and_then(|own| match self.context_selectors(rule.node()) {
      Err(()) => None,
      Ok(None) => Some(own),
      Ok(Some(parent)) => Some(resolve_nested_list(&own, &parent)),
    });
    self.resolved.insert(rule.id(), resolved.clone());
    resolved
  }

  /// What `&` stands for inside `node`: the resolved selectors of the
  /// nearest enclosing rule, `:where(:scope)` in an `@scope`, or nothing at
  /// the top level; `Err` when a rule on the way is not standard syntax or
  /// does not parse.
  fn context_selectors(&mut self, node: NodeRef) -> Result<Option<SelectorList>, ()> {
    for ancestor in node.ancestors() {
      if let Some(rule) = ancestor.as_rule() {
        if !rule.is_standard_syntax() {
          return Err(());
        }
        return self.resolve(rule).map(Some).ok_or(());
      }
      if let Some(at) = ancestor.as_at_rule()
        && at.name().eq_ignore_ascii_case("scope")
      {
        // The rules above an `@scope` must still be standard syntax.
        self.context_selectors(ancestor)?;
        return parse_selector_list(":where(:scope)")
          .map(Some)
          .map_err(|_| ());
      }
    }
    Ok(None)
  }
}

/// Stylelint's `normalizeSelector` string of one selector: no comments or
/// whitespace around combinators and inside attribute selectors, each
/// compound selector in a canonical order, and the selector lists inside
/// pseudo-classes sorted without repeats.
pub fn normalize_selector(selector: &Selector) -> String {
  // Each node's text, with its sort group: compounds are sorted between
  // combinators and pseudo-elements.
  let mut out = String::new();
  let mut run: Vec<(u8, String)> = Vec::new();
  let flush = |run: &mut Vec<(u8, String)>, out: &mut String| {
    run.sort();
    for (_, text) in run.drain(..) {
      out.push_str(&text);
    }
  };
  for node in &selector.nodes {
    match node {
      SelectorNode::Comment(_) => {}
      SelectorNode::Combinator(kind) => {
        flush(&mut run, &mut out);
        out.push_str(match kind {
          Combinator::Descendant => " ",
          Combinator::Child => ">",
          Combinator::NextSibling => "+",
          Combinator::SubsequentSibling => "~",
          Combinator::Column => "||",
        });
      }
      SelectorNode::Pseudo(pseudo) if pseudo.element => {
        flush(&mut run, &mut out);
        out.push_str(&normalize_pseudo(pseudo));
      }
      SelectorNode::Pseudo(pseudo) => run.push((sort_priority(node), normalize_pseudo(pseudo))),
      SelectorNode::Attribute(raw) => run.push((sort_priority(node), strip_attribute_spaces(raw))),
      _ => run.push((sort_priority(node), node.to_string())),
    }
  }
  flush(&mut run, &mut out);
  out
}

/// A pseudo-class or pseudo-element as [`normalize_selector`] prints it.
fn normalize_pseudo(pseudo: &Pseudo) -> String {
  let colons = if pseudo.element { "::" } else { ":" };
  match &pseudo.arg {
    PseudoArg::Selectors(list) => {
      let mut selectors: Vec<String> = list.selectors.iter().map(normalize_selector).collect();
      selectors.sort();
      selectors.dedup();
      format!("{colons}{}({})", pseudo.name, selectors.join(","))
    }
    _ => pseudo.to_string(),
  }
}

/// An attribute selector without the whitespace outside its strings.
fn strip_attribute_spaces(raw: &str) -> String {
  let mut out = String::with_capacity(raw.len());
  let mut quote = None;
  let mut escaped = false;
  for c in raw.chars() {
    match quote {
      _ if escaped => escaped = false,
      Some(q) if c == q => quote = None,
      Some(_) => {}
      None if c == '"' || c == '\'' => quote = Some(c),
      None if c.is_whitespace() => continue,
      None => {}
    }
    if c == '\\' {
      escaped = true;
    }
    out.push(c);
  }
  out
}

/// Stylelint's `lastCompoundSelectorWithoutPseudoClasses`: the compound
/// selector after the last combinator, in normalised order, without its
/// pseudo-classes; `None` when nothing is left.
pub fn last_compound_without_pseudo_classes(selector: &Selector) -> Option<String> {
  let nodes: Vec<&SelectorNode> = selector
    .nodes
    .iter()
    .filter(|node| !matches!(node, SelectorNode::Comment(_)))
    .collect();
  let start = nodes
    .iter()
    .rposition(|node| node.is_combinator())
    .map_or(0, |i| i + 1);
  // Normalised order: each run between pseudo-elements sorted by kind,
  // then text.
  let mut compound: Vec<(u8, String, &SelectorNode)> = Vec::new();
  let mut run_start = 0;
  for node in &nodes[start..] {
    let text = node.to_string();
    if node.is_pseudo_element() {
      compound[run_start..].sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
      compound.push((u8::MAX, text, node));
      run_start = compound.len();
    } else {
      compound.push((sort_priority(node), text, node));
    }
  }
  compound[run_start..].sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
  let kept: String = compound
    .into_iter()
    .filter(|(_, _, node)| match node {
      SelectorNode::Pseudo(pseudo) => pseudo.element || is_known_pseudo_element(&pseudo.name),
      _ => true,
    })
    .map(|(_, text, _)| text)
    .collect();
  (!kept.is_empty()).then_some(kept)
}

/// Where `normalizeSelector` sorts a node inside a compound selector.
fn sort_priority(node: &SelectorNode) -> u8 {
  match node {
    SelectorNode::Universal => 0,
    SelectorNode::Tag(_) => 1,
    SelectorNode::Nesting => 2,
    SelectorNode::Id(_) => 3,
    SelectorNode::Class(_) => 4,
    SelectorNode::Attribute(_) => 5,
    SelectorNode::Pseudo(_) => 6,
    SelectorNode::Comment(_) | SelectorNode::Combinator(_) => 7,
  }
}
