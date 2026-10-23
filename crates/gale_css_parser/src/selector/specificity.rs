//! Selector specificity, computed the way `@csstools/selector-specificity`
//! (which Stylelint's specificity rules call) computes it, over the
//! [`postcss`](super::postcss) selector tree.

use std::ops::{Add, AddAssign, Sub};

use super::postcss::{self, Kind, Node, Selector};

/// A specificity: `a` counts ids, `b` classes, attributes and
/// pseudo-classes, `c` types and pseudo-elements.  The derived order compares
/// `a`, then `b`, then `c`, as the package's `compare` does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Specificity {
  pub a: u32,
  pub b: u32,
  pub c: u32,
}

impl Specificity {
  /// The specificity `(a, b, c)`.
  pub const fn new(a: u32, b: u32, c: u32) -> Self {
    Self { a, b, c }
  }
}

impl Add for Specificity {
  type Output = Self;

  fn add(self, other: Self) -> Self {
    Self::new(self.a + other.a, self.b + other.b, self.c + other.c)
  }
}

impl AddAssign for Specificity {
  fn add_assign(&mut self, other: Self) {
    *self = *self + other;
  }
}

impl Sub for Specificity {
  type Output = Self;

  fn sub(self, other: Self) -> Self {
    Self::new(
      self.a.saturating_sub(other.a),
      self.b.saturating_sub(other.b),
      self.c.saturating_sub(other.c),
    )
  }
}

/// The package's `customSpecificity` option: a specificity to use for a
/// node instead of computing it, or `None` to compute it.
pub type Custom<'a> = &'a dyn Fn(&Node) -> Option<Specificity>;

/// The specificity of the selector `text`, or `None` where
/// postcss-selector-parser cannot parse it.  A list sums its selectors, as
/// the package does for a root node.
pub fn of_text(text: &str) -> Option<Specificity> {
  of_text_with(text, None)
}

/// [`of_text`] with a `customSpecificity` callback.
pub fn of_text_with(text: &str, custom: Option<Custom>) -> Option<Specificity> {
  let selectors = postcss::parse(text, 0)?;
  Some(
    selectors
      .iter()
      .fold(Specificity::default(), |sum, selector| {
        sum + of_selector(selector, custom)
      }),
  )
}

/// The specificity of one selector: the sum of its nodes'.
pub fn of_selector(selector: &Selector, custom: Option<Custom>) -> Specificity {
  of_nodes(&selector.nodes, custom)
}

/// The sum of the specificities of `nodes`.
fn of_nodes(nodes: &[Node], custom: Option<Custom>) -> Specificity {
  nodes.iter().fold(Specificity::default(), |sum, node| {
    sum + of_node(node, custom)
  })
}

/// The highest specificity among `selectors` (`specificityOfMostSpecificListItem`).
pub fn most_specific(selectors: &[Selector], custom: Option<Custom>) -> Specificity {
  selectors
    .iter()
    .map(|selector| of_selector(selector, custom))
    .max()
    .unwrap_or_default()
}

/// The specificity of one node (`selectorSpecificity`).
pub fn of_node(node: &Node, custom: Option<Custom>) -> Specificity {
  if let Some(specificity) = custom.and_then(|custom| custom(node)) {
    return specificity;
  }
  match node.kind {
    Kind::Id => Specificity::new(1, 0, 0),
    Kind::Class | Kind::Attribute => Specificity::new(0, 1, 0),
    Kind::Tag => Specificity::new(0, 0, 1),
    Kind::Pseudo if is_pseudo_element(node) => pseudo_element(node, custom),
    Kind::Pseudo => pseudo_class(node, custom),
    Kind::Universal | Kind::Nesting | Kind::Combinator | Kind::Comment | Kind::String => {
      Specificity::default()
    }
  }
}

/// Whether the pseudo is an element (postcss-selector-parser's
/// `isPseudoElement`): `::name`, or one of the four CSS 2 pseudo-elements
/// written with one colon.
pub fn is_pseudo_element(node: &Node) -> bool {
  node.value.starts_with("::")
    || [":before", ":after", ":first-letter", ":first-line"]
      .iter()
      .any(|legacy| node.value.eq_ignore_ascii_case(legacy))
}

/// The argument selectors of a pseudo, empty without parentheses.
fn args(node: &Node) -> &[Selector] {
  node.args.as_ref().map_or(&[], |args| &args.selectors)
}

/// A pseudo-element's specificity.
fn pseudo_element(node: &Node, custom: Option<Custom>) -> Specificity {
  match node.value.to_ascii_lowercase().as_str() {
    "::slotted" => Specificity::new(0, 0, 1) + most_specific(args(node), custom),
    "::view-transition-group"
    | "::view-transition-image-pair"
    | "::view-transition-old"
    | "::view-transition-new" => match args(node) {
      [only] if holds_nothing_or_only_universal(only) => Specificity::default(),
      _ => Specificity::new(0, 0, 1),
    },
    _ => Specificity::new(0, 0, 1),
  }
}

/// A pseudo-class's specificity.
fn pseudo_class(node: &Node, custom: Option<Custom>) -> Specificity {
  let class = Specificity::new(0, 1, 0);
  match node.value.to_ascii_lowercase().as_str() {
    ":-moz-any" | ":has" | ":is" | ":matches" | ":not" => most_specific(args(node), custom),
    ":where" => Specificity::default(),
    ":nth-child" | ":nth-last-child" => class + nth_child_of(args(node), custom),
    ":local" | ":global" => args(node)
      .iter()
      .fold(Specificity::default(), |sum, selector| {
        sum + of_selector(selector, custom)
      }),
    ":host" | ":host-context" => class + most_specific(args(node), custom),
    ":active-view-transition" | ":active-view-transition-type" => class,
    _ => class,
  }
}

/// The most specific selector of the `of S` part of an `:nth-child()`
/// argument, or nothing without one.
fn nth_child_of(selectors: &[Selector], custom: Option<Custom>) -> Specificity {
  let Some((first, rest)) = selectors.split_first() else {
    return Specificity::default();
  };
  let Some(of) = first
    .nodes
    .iter()
    .position(|node| node.kind == Kind::Tag && node.value.eq_ignore_ascii_case("of"))
  else {
    return Specificity::default();
  };
  let of_part = Specificity::default().max(of_nodes(&first.nodes[of + 1..], custom));
  rest
    .iter()
    .map(|selector| of_selector(selector, custom))
    .fold(of_part, Specificity::max)
}

/// Whether a selector holds nothing but comments, or a lone `*`.
fn holds_nothing_or_only_universal(selector: &Selector) -> bool {
  let mut nodes = selector
    .nodes
    .iter()
    .filter(|node| node.kind != Kind::Comment);
  match (nodes.next(), nodes.next()) {
    (None, _) => true,
    (Some(node), None) => node.kind == Kind::Universal,
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn spec(text: &str) -> (u32, u32, u32) {
    let s = of_text(text).unwrap();
    (s.a, s.b, s.c)
  }

  #[test]
  fn counts_ids_classes_and_types() {
    assert_eq!(spec("a"), (0, 0, 1));
    assert_eq!(spec("#id .class a[href]"), (1, 2, 1));
    assert_eq!(spec("*"), (0, 0, 0));
    assert_eq!(spec("a:hover::before"), (0, 1, 2));
    // CSS 2 pseudo-elements count as types with one colon too.
    assert_eq!(spec("a:BEFORE"), (0, 0, 2));
  }

  #[test]
  fn functional_pseudo_classes_take_their_arguments() {
    assert_eq!(spec(":where(#a .b)"), (0, 0, 0));
    assert_eq!(spec(":is(a, #b, .c)"), (1, 0, 0));
    assert_eq!(spec(":NOT(.a .b)"), (0, 2, 0));
    assert_eq!(spec(":nth-child(2n+1)"), (0, 1, 0));
    assert_eq!(spec(":nth-child(2n of .a, #b)"), (1, 1, 0));
    assert_eq!(spec(":host(.a)"), (0, 2, 0));
    assert_eq!(spec(":global(.a .b)"), (0, 2, 0));
    assert_eq!(spec("::slotted(.a)"), (0, 1, 1));
    assert_eq!(spec("::view-transition-old(*)"), (0, 0, 0));
    assert_eq!(spec("::view-transition-old(x)"), (0, 0, 1));
    assert_eq!(spec(":-webkit-any(#a)"), (0, 1, 0));
  }

  #[test]
  fn compares_a_then_b_then_c() {
    assert!(Specificity::new(1, 0, 0) > Specificity::new(0, 9, 9));
    assert!(Specificity::new(0, 1, 0) > Specificity::new(0, 0, 9));
  }

  #[test]
  fn custom_specificity_replaces_a_node() {
    let zero_classes = |node: &Node| (node.kind == Kind::Class).then(Specificity::default);
    assert_eq!(
      of_text_with("a.b.c", Some(&zero_classes)),
      Some(Specificity::new(0, 0, 1))
    );
  }
}
