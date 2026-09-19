//! Resolution of the nesting selector `&` against a parent selector list,
//! following the CSS Nesting specification the way
//! `@csstools/selector-resolve-nested` does.

use super::{Combinator, Pseudo, PseudoArg, Selector, SelectorList, SelectorNode};

/// Resolve every selector of `child` against `parent`.
pub fn resolve_nested_list(child: &SelectorList, parent: &SelectorList) -> SelectorList {
  SelectorList {
    selectors: child
      .selectors
      .iter()
      .map(|selector| resolve_nested(selector, parent))
      .collect(),
  }
}

/// Resolve one nested selector against its parent's resolved selector list.
///
/// A selector without `&` is treated as `& <selector>`, and one that starts
/// with a combinator gets `&` prepended. Each `&` is then replaced by the
/// parent, inline when the parent is a single compound selector and wrapped
/// in `:is()` otherwise, and every compound selector that changed is put
/// back into canonical order. The result keeps the extent of `child`.
pub fn resolve_nested(child: &Selector, parent: &SelectorList) -> Selector {
  let mut nodes = child.nodes.clone();
  if !contains_nesting(&nodes) {
    nodes.insert(0, SelectorNode::Combinator(Combinator::Descendant));
    nodes.insert(0, SelectorNode::Nesting);
  } else if matches!(nodes.first(), Some(SelectorNode::Combinator(_))) {
    nodes.insert(0, SelectorNode::Nesting);
  }
  Selector {
    nodes: replace_nesting(nodes, parent, false),
    offset: child.offset,
    end: child.end,
  }
}

/// Whether a selector contains `&` at any depth.
fn contains_nesting(nodes: &[SelectorNode]) -> bool {
  nodes.iter().any(|node| match node {
    SelectorNode::Nesting => true,
    SelectorNode::Pseudo(pseudo) => pseudo.selectors().is_some_and(|list| {
      list
        .selectors
        .iter()
        .any(|selector| contains_nesting(&selector.nodes))
    }),
    _ => false,
  })
}

/// Replace every `&` with the parent selector, then sort the compound
/// selectors of any selector that changed. Inside `:has()` the parent is
/// always wrapped in `:is()`.
fn replace_nesting(
  nodes: Vec<SelectorNode>,
  parent: &SelectorList,
  in_has: bool,
) -> Vec<SelectorNode> {
  let mut result = Vec::with_capacity(nodes.len());
  let mut replaced = false;
  for node in nodes {
    match node {
      SelectorNode::Nesting => {
        result.extend(parent_stand_in(parent, in_has));
        replaced = true;
      }
      SelectorNode::Pseudo(mut pseudo) => {
        let in_has = !pseudo.element && pseudo.is("has");
        if let Some(list) = pseudo.arg.selectors_mut() {
          *list = replace_nesting_in_list(std::mem::take(list), parent, in_has);
        }
        result.push(SelectorNode::Pseudo(pseudo));
      }
      other => result.push(other),
    }
  }
  if replaced {
    sort_compound_selectors(result)
  } else {
    result
  }
}

fn replace_nesting_in_list(
  list: SelectorList,
  parent: &SelectorList,
  in_has: bool,
) -> SelectorList {
  SelectorList {
    selectors: list
      .selectors
      .into_iter()
      .map(|selector| Selector {
        nodes: replace_nesting(selector.nodes, parent, in_has),
        offset: selector.offset,
        end: selector.end,
      })
      .collect(),
  }
}

/// The nodes that stand in for `&`: the parent inline when it is a single
/// compound selector, otherwise wrapped in `:is()`.
fn parent_stand_in(parent: &SelectorList, force_is: bool) -> Vec<SelectorNode> {
  if force_is || !is_compound_selector_list(parent) {
    return vec![is_pseudo_class(parent.clone())];
  }
  parent.selectors[0].nodes.clone()
}

/// An `:is()` wrapping `list`.
fn is_pseudo_class(list: SelectorList) -> SelectorNode {
  SelectorNode::Pseudo(Pseudo {
    element: false,
    name: "is".to_string(),
    arg: PseudoArg::Selectors(list),
  })
}

/// Whether a selector list is exactly one compound selector.
fn is_compound_selector_list(list: &SelectorList) -> bool {
  list.selectors.len() == 1
    && !list.selectors[0]
      .nodes
      .iter()
      .any(|node| node.is_combinator() || node.is_pseudo_element())
}

/// Put each compound selector back into canonical order after substitution.
/// A compound can hold only one universal and one type selector, so a second
/// universal is dropped and a second type selector is wrapped in `:is()`.
fn sort_compound_selectors(nodes: Vec<SelectorNode>) -> Vec<SelectorNode> {
  let mut groups: Vec<Vec<SelectorNode>> = Vec::new();
  let mut current: Vec<SelectorNode> = Vec::new();

  for node in nodes {
    match node {
      SelectorNode::Combinator(_) => {
        groups.push(std::mem::take(&mut current));
        groups.push(vec![node]);
      }
      SelectorNode::Pseudo(_) if node.is_pseudo_element() => {
        groups.push(std::mem::take(&mut current));
        current.push(node);
      }
      SelectorNode::Universal if current.contains(&SelectorNode::Universal) => {}
      SelectorNode::Tag(_) if current.iter().any(|n| matches!(n, SelectorNode::Tag(_))) => {
        // A synthesised selector, so it has no extent in the source.
        let wrapped = SelectorList {
          selectors: vec![Selector {
            nodes: vec![node],
            offset: 0,
            end: 0,
          }],
        };
        current.push(is_pseudo_class(wrapped));
      }
      _ => current.push(node),
    }
  }
  groups.push(current);

  groups
    .into_iter()
    .flat_map(|mut group| {
      group.sort_by_key(node_order);
      group
    })
    .collect()
}

fn node_order(node: &SelectorNode) -> u8 {
  match node {
    SelectorNode::Universal => 0,
    SelectorNode::Tag(_) => 1,
    SelectorNode::Pseudo(pseudo) if pseudo.element => 2,
    SelectorNode::Nesting => 3,
    SelectorNode::Id(_) => 4,
    SelectorNode::Class(_) => 5,
    SelectorNode::Attribute(_) => 6,
    SelectorNode::Pseudo(_) => 7,
    SelectorNode::Comment(_) => 8,
    SelectorNode::Combinator(_) => 9,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::selector::parse_selector_list;

  fn resolve(child: &str, parent: &str) -> String {
    let child = parse_selector_list(child).expect("child should parse");
    let parent = parse_selector_list(parent).expect("parent should parse");
    resolve_nested_list(&child, &parent).to_string()
  }

  #[test]
  fn treats_a_selector_without_nesting_as_a_descendant() {
    assert_eq!(resolve(".foo", "a"), "a .foo");
  }

  #[test]
  fn inlines_a_compound_parent() {
    assert_eq!(resolve("&:hover", "a.b"), "a.b:hover");
  }

  #[test]
  fn wraps_a_complex_parent_in_is() {
    assert_eq!(resolve("&:hover", "a b"), ":is(a b):hover");
    assert_eq!(resolve("&:hover", "a::before"), ":is(a::before):hover");
  }

  #[test]
  fn wraps_a_parent_list_in_is() {
    assert_eq!(resolve("&:hover", "a, b"), ":is(a,b):hover");
  }

  #[test]
  fn prepends_nesting_before_a_leading_combinator() {
    assert_eq!(resolve("> :is(&)", "a"), "a > :is(a)");
  }

  #[test]
  fn resolves_an_explicit_nesting_selector_in_place() {
    assert_eq!(resolve("& > b", "a"), "a > b");
  }

  #[test]
  fn resolves_nesting_inside_arguments() {
    assert_eq!(resolve(":not(&)", "a"), ":not(a)");
  }

  #[test]
  fn wraps_the_parent_in_is_inside_has() {
    assert_eq!(resolve(":has(&)", "a"), ":has(:is(a))");
  }

  #[test]
  fn sorts_the_compound_after_substitution() {
    assert_eq!(resolve(".foo&", "a"), "a.foo");
  }

  #[test]
  fn wraps_a_second_type_selector_in_is() {
    assert_eq!(resolve("b&", "a"), "b:is(a)");
  }

  #[test]
  fn drops_a_second_universal_selector() {
    assert_eq!(resolve("*&", "*.a"), "*.a");
  }

  #[test]
  fn keeps_the_extent_of_the_nested_selector() {
    let child = parse_selector_list("  .foo").expect("child should parse");
    let parent = parse_selector_list("a").expect("parent should parse");
    let resolved = resolve_nested(&child.selectors[0], &parent);
    assert_eq!((resolved.offset, resolved.end), (2, 6));
  }
}
