//! A small CSS selector AST shared by the selector rules.
//!
//! Gale's selector rules historically scanned raw selector strings. Rules that
//! need to reason about selector *structure*, which compound selector a
//! pseudo-class belongs to, what sits inside `:is()`, whether a combinator
//! follows a pseudo-element, need an AST instead. This module provides one:
//! [`parser`] builds it from text, [`nesting`] resolves `&` against a parent
//! selector, and `Display` turns it back into text.
//!
//! Nodes carry no positions of their own. A [`Selector`] records the extent
//! of the text it was parsed from, and a selector produced by nesting
//! resolution keeps the extent of the nested selector it came from.

use std::fmt;

pub mod nesting;
pub mod parser;

pub use parser::{ParseError, parse_selector_list};

/// A combinator between two compound selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
  /// Whitespace.
  Descendant,
  /// `>`
  Child,
  /// `+`
  NextSibling,
  /// `~`
  SubsequentSibling,
  /// `||`
  Column,
}

impl Combinator {
  /// The combinator as CSS text, with normalised spacing.
  pub fn as_str(self) -> &'static str {
    match self {
      Combinator::Descendant => " ",
      Combinator::Child => " > ",
      Combinator::NextSibling => " + ",
      Combinator::SubsequentSibling => " ~ ",
      Combinator::Column => " || ",
    }
  }
}

/// A single component of a compound selector.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectorNode {
  /// A type selector such as `a` or `svg|path`.
  Tag(String),
  /// `*`
  Universal,
  /// `.foo`
  Class(String),
  /// `#foo`
  Id(String),
  /// `[foo="bar"]`, stored with its brackets.
  Attribute(String),
  /// `:hover`, `::before`, `:is(a, b)`.
  Pseudo(Pseudo),
  /// `&`
  Nesting,
  /// A combinator separating compound selectors.
  Combinator(Combinator),
  /// `/* ... */` inside a selector.
  Comment(String),
}

impl SelectorNode {
  /// The pseudo-class or pseudo-element this node is, if it is one.
  pub fn as_pseudo(&self) -> Option<&Pseudo> {
    match self {
      SelectorNode::Pseudo(pseudo) => Some(pseudo),
      _ => None,
    }
  }

  pub fn is_pseudo_element(&self) -> bool {
    matches!(self, SelectorNode::Pseudo(pseudo) if pseudo.element)
  }

  pub fn is_combinator(&self) -> bool {
    matches!(self, SelectorNode::Combinator(_))
  }
}

/// A pseudo-class or pseudo-element.
#[derive(Debug, Clone, PartialEq)]
pub struct Pseudo {
  /// Whether this is a pseudo-element (`::before`) rather than a
  /// pseudo-class (`:hover`).
  pub element: bool,
  /// The name as written, without colons.
  pub name: String,
  /// What sits between the parentheses, if any.
  pub arg: PseudoArg,
}

impl Pseudo {
  /// The name lower-cased.
  pub fn lower_name(&self) -> String {
    self.name.to_ascii_lowercase()
  }

  /// Whether the name is `name`, ignoring case.
  pub fn is(&self, name: &str) -> bool {
    self.name.eq_ignore_ascii_case(name)
  }

  /// Whether the name is one of `names`, ignoring case.
  pub fn is_one_of(&self, names: &[&str]) -> bool {
    names.iter().any(|name| self.is(name))
  }

  /// The selector list inside the argument, if it holds one.
  pub fn selectors(&self) -> Option<&SelectorList> {
    self.arg.selectors()
  }

  /// `:name` or `::name` as written, which Stylelint calls the node's value.
  pub fn value(&self) -> String {
    format!("{}{}", self.colons(), self.name)
  }

  /// `:name` or `:name()` lower-cased, the form Stylelint prints in reasons.
  pub fn display_name(&self) -> String {
    let parens = if self.arg.is_none() { "" } else { "()" };
    format!("{}{}{parens}", self.colons(), self.lower_name())
  }

  fn colons(&self) -> &'static str {
    if self.element { "::" } else { ":" }
  }
}

/// The argument of a pseudo-class or pseudo-element.
#[derive(Debug, Clone, PartialEq)]
pub enum PseudoArg {
  /// Written without parentheses, e.g. `:hover`.
  None,
  /// A selector list, e.g. `:is(a, b)`.
  Selectors(SelectorList),
  /// An+B notation with an optional `of S` clause, e.g. `:nth-child(2n of .a)`.
  /// `raw` is the whole argument as written.
  Anb {
    raw: String,
    of: Option<SelectorList>,
  },
  /// Anything else, kept as written, e.g. `:lang(en)`.
  Raw(String),
}

impl PseudoArg {
  pub fn is_none(&self) -> bool {
    matches!(self, PseudoArg::None)
  }

  /// The selector list inside the argument, if it holds one.
  pub fn selectors(&self) -> Option<&SelectorList> {
    match self {
      PseudoArg::Selectors(list) => Some(list),
      PseudoArg::Anb { of, .. } => of.as_ref(),
      PseudoArg::None | PseudoArg::Raw(_) => None,
    }
  }

  pub fn selectors_mut(&mut self) -> Option<&mut SelectorList> {
    match self {
      PseudoArg::Selectors(list) => Some(list),
      PseudoArg::Anb { of, .. } => of.as_mut(),
      PseudoArg::None | PseudoArg::Raw(_) => None,
    }
  }
}

/// One selector in a selector list, e.g. the `a > b` of `a > b, .c`.
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
  pub nodes: Vec<SelectorNode>,
  /// Byte offset of the first non-comment node in the source.
  pub offset: usize,
  /// Exclusive byte offset of the last non-comment node, ignoring trailing
  /// combinators.
  pub end: usize,
}

/// A comma-separated list of selectors.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectorList {
  pub selectors: Vec<Selector>,
}

impl SelectorList {
  /// Whether any top-level node of any selector in the list satisfies `pred`.
  pub fn any_node(&self, mut pred: impl FnMut(&SelectorNode) -> bool) -> bool {
    self
      .selectors
      .iter()
      .any(|selector| selector.nodes.iter().any(&mut pred))
  }
}

// ---------------------------------------------------------------------------
// Serialisation: normalised spacing, comments dropped
// ---------------------------------------------------------------------------

impl fmt::Display for SelectorList {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (i, selector) in self.selectors.iter().enumerate() {
      if i > 0 {
        f.write_str(",")?;
      }
      write!(f, "{selector}")?;
    }
    Ok(())
  }
}

impl fmt::Display for Selector {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.nodes.iter().try_for_each(|node| write!(f, "{node}"))
  }
}

impl fmt::Display for SelectorNode {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      SelectorNode::Tag(name) => f.write_str(name),
      SelectorNode::Universal => f.write_str("*"),
      SelectorNode::Class(name) => write!(f, ".{name}"),
      SelectorNode::Id(name) => write!(f, "#{name}"),
      SelectorNode::Attribute(raw) => f.write_str(raw),
      SelectorNode::Pseudo(pseudo) => write!(f, "{pseudo}"),
      SelectorNode::Nesting => f.write_str("&"),
      SelectorNode::Combinator(kind) => f.write_str(kind.as_str()),
      SelectorNode::Comment(_) => Ok(()),
    }
  }
}

impl fmt::Display for Pseudo {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}{}", self.colons(), self.name)?;
    match &self.arg {
      PseudoArg::None => Ok(()),
      PseudoArg::Selectors(list) => write!(f, "({list})"),
      PseudoArg::Anb { raw, .. } | PseudoArg::Raw(raw) => write!(f, "({raw})"),
    }
  }
}

// ---------------------------------------------------------------------------
// Walking
// ---------------------------------------------------------------------------

/// A pseudo-class or pseudo-element found by [`walk_pseudos`], together with
/// where it sits.
pub struct PseudoVisit<'a, 'b> {
  pub pseudo: &'a Pseudo,
  /// The nodes of the selector holding it.
  pub siblings: &'a [SelectorNode],
  /// Its index in `siblings`.
  pub index: usize,
  /// The pseudo-classes whose arguments enclose it, outermost first. A
  /// pseudo-element's argument starts a fresh chain.
  pub ancestors: &'b [&'a Pseudo],
}

/// Visit every pseudo-class and pseudo-element in `nodes`, including those
/// nested inside arguments, in source order. Stops once `visit` returns
/// `false`, and returns whether the walk ran to completion.
pub fn walk_pseudos<'a>(
  nodes: &'a [SelectorNode],
  visit: &mut dyn FnMut(&PseudoVisit<'a, '_>) -> bool,
) -> bool {
  walk_with_ancestors(nodes, &mut Vec::new(), visit)
}

fn walk_with_ancestors<'a>(
  nodes: &'a [SelectorNode],
  ancestors: &mut Vec<&'a Pseudo>,
  visit: &mut dyn FnMut(&PseudoVisit<'a, '_>) -> bool,
) -> bool {
  for (index, node) in nodes.iter().enumerate() {
    let SelectorNode::Pseudo(pseudo) = node else {
      continue;
    };
    let keep_going = visit(&PseudoVisit {
      pseudo,
      siblings: nodes,
      index,
      ancestors: ancestors.as_slice(),
    });
    if !keep_going {
      return false;
    }
    let Some(list) = pseudo.selectors() else {
      continue;
    };
    let outer = if pseudo.element {
      std::mem::take(ancestors)
    } else {
      ancestors.push(pseudo);
      Vec::new()
    };
    let completed = list
      .selectors
      .iter()
      .all(|selector| walk_with_ancestors(&selector.nodes, ancestors, visit));
    if pseudo.element {
      *ancestors = outer;
    } else {
      ancestors.pop();
    }
    if !completed {
      return false;
    }
  }
  true
}

/// Whether any pseudo-class or pseudo-element in `nodes`, at any depth,
/// satisfies `pred`.
pub fn any_pseudo(nodes: &[SelectorNode], mut pred: impl FnMut(&Pseudo) -> bool) -> bool {
  !walk_pseudos(nodes, &mut |visit| !pred(visit.pseudo))
}

/// Whether a selector uses only standard CSS syntax; interpolation and
/// preprocessor placeholders are left alone.
pub fn is_standard_syntax_selector(selector: &str) -> bool {
  let trimmed = selector.trim_start();
  !(trimmed.starts_with('%')
    || selector.contains("#{")
    || selector.contains("@{")
    || selector.contains('$'))
}
