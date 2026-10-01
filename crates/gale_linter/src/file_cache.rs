//! What several rules derive from the same file, built once per file.
//!
//! Many rules read a file through the same lens: the PostCSS-shaped
//! statement tree ([`PostcssTree`]), say.  Each rule building its own copy
//! means a config with twenty such rules parses every file twenty times.
//! The runner keeps one [`FileCache`] per file instead and hands it to every
//! rule through [`RuleContext`](crate::rule::RuleContext); the first rule
//! to ask for an artifact builds it and the others share it.
//!
//! Some artifacts belong to a node rather than the file: the declarations
//! of the node every rule's `check` is looking at, say.  The runner marks
//! the node it is checking ([`FileCache::enter_node`]) and those artifacts
//! are shared until it moves on to the next one.
//!
//! # Panics in a build
//!
//! Rules run inside [`panic_guard::catch`](crate::panic_guard::catch), and
//! an artifact is built by whichever rule asks for it first, so a build can
//! panic inside one rule's guard.  The cells are [`OnceCell`]s, which are
//! never poisoned: a build that panics leaves its cell empty, the rule that
//! asked reports the internal error, and the next rule to ask builds the
//! artifact again.  Builds are deterministic, so that rule panics the same
//! way, exactly as when every rule built its own copy.  A node's artifacts
//! are built before their `RefCell` is borrowed, so the same holds for
//! them.  The accessors hand out `Rc` clones and hold no borrow while a
//! rule runs, so a rule that panics while using an artifact cannot lock it
//! for the others either.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use gale_css_parser::{CssNode, Syntax};

use crate::postcss_tree::PostcssTree;
use crate::source_text::{self, WrittenDeclaration};
use crate::style_rules::{self, ScannedRules};

/// The artifacts shared by every rule linting one file.
///
/// Each is built on first use from [`Self::source`] parsed as
/// [`Self::syntax`].
pub struct FileCache<'a> {
  /// The text every artifact is built from: the text the rules lint.
  source: &'a str,
  /// The syntax `source` is parsed as.
  syntax: Syntax,
  /// The statements as PostCSS sees them.
  postcss_tree: OnceCell<Rc<PostcssTree<'a>>>,
  /// The style rules and at-rules as written.
  scanned_rules: OnceCell<Rc<ScannedRules>>,
  /// The node the runner is checking, null between nodes.  Only compared,
  /// never dereferenced.
  current_node: Cell<*const CssNode>,
  /// The declarations of `current_node`, once a rule has asked.
  node_declarations: RefCell<Option<Rc<Vec<WrittenDeclaration<'a>>>>>,
}

impl<'a> FileCache<'a> {
  /// An empty cache for `source`, parsed as `syntax`.
  pub fn new(source: &'a str, syntax: Syntax) -> Self {
    Self {
      source,
      syntax,
      postcss_tree: OnceCell::new(),
      scanned_rules: OnceCell::new(),
      current_node: Cell::new(std::ptr::null()),
      node_declarations: RefCell::new(None),
    }
  }

  /// The text the artifacts are built from.
  pub fn source(&self) -> &'a str {
    self.source
  }

  /// The syntax the text is parsed as.
  pub fn syntax(&self) -> Syntax {
    self.syntax
  }

  /// Whether this cache was built for `source` (the very same text, not
  /// just equal text) parsed as `syntax`.
  pub fn is_for(&self, source: &str, syntax: Syntax) -> bool {
    std::ptr::eq(self.source, source) && self.syntax == syntax
  }

  /// The file's statements as PostCSS sees them, parsed on first use.
  pub fn postcss_tree(&self) -> Rc<PostcssTree<'a>> {
    shared(&self.postcss_tree, || {
      PostcssTree::parse(self.source, self.syntax)
    })
  }

  /// The file's style rules and at-rules as written, scanned on first use.
  pub fn scanned_rules(&self) -> Rc<ScannedRules> {
    shared(&self.scanned_rules, || {
      style_rules::scan(self.source, self.syntax)
    })
  }

  /// Start sharing the artifacts of `node`, the node every rule is about to
  /// check, dropping those of the node before.
  ///
  /// The caller must keep `node` where it is until [`Self::leave_node`]:
  /// no other node can then have its address, which is what identifies it.
  pub(crate) fn enter_node(&self, node: &CssNode) {
    self.current_node.set(node);
    self.node_declarations.replace(None);
  }

  /// Stop sharing the artifacts of the node [`Self::enter_node`] marked.
  pub(crate) fn leave_node(&self) {
    self.current_node.set(std::ptr::null());
    self.node_declarations.replace(None);
  }

  /// The declarations `node` holds directly, as written (see
  /// [`source_text::written_declarations`]).  Shared while `node` is the
  /// node the runner is checking, and read afresh for any other node.
  pub fn written_declarations(&self, node: &CssNode) -> Rc<Vec<WrittenDeclaration<'a>>> {
    let read = || {
      Rc::new(source_text::written_declarations(
        self.source,
        node,
        self.syntax,
      ))
    };
    if !std::ptr::eq(node, self.current_node.get()) {
      return read();
    }
    if let Some(found) = self.node_declarations.borrow().as_ref() {
      return Rc::clone(found);
    }
    // Read before borrowing, so that a read that panics leaves nothing
    // cached and nothing borrowed.
    let found = read();
    self.node_declarations.replace(Some(Rc::clone(&found)));
    found
  }

  /// [`Self::postcss_tree`], built by `build` if it is still to be built,
  /// so that tests can make the build panic.
  #[cfg(test)]
  pub(crate) fn postcss_tree_built_by(
    &self,
    build: impl FnOnce() -> PostcssTree<'a>,
  ) -> Rc<PostcssTree<'a>> {
    shared(&self.postcss_tree, build)
  }
}

/// The value in `cell`, built by `build` if the cell is still empty.
///
/// A `build` that panics leaves the cell empty for the next caller to try
/// again; see the [module docs](self).
fn shared<T>(cell: &OnceCell<Rc<T>>, build: impl FnOnce() -> T) -> Rc<T> {
  Rc::clone(cell.get_or_init(|| Rc::new(build())))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::panic_guard;
  use std::cell::Cell;

  /// The first caller builds the value; later callers get the same `Rc`.
  #[test]
  fn an_artifact_is_built_once_and_shared() {
    let cell = OnceCell::new();
    let builds = Cell::new(0);
    let build = || {
      builds.set(builds.get() + 1);
      42
    };
    let first = shared(&cell, build);
    let second = shared(&cell, build);
    assert_eq!((*first, *second), (42, 42));
    assert!(Rc::ptr_eq(&first, &second));
    assert_eq!(builds.get(), 1);
  }

  /// A panicking build caches nothing and poisons nothing: the next caller
  /// builds the value and it is shared from then on.
  #[test]
  fn a_build_that_panics_leaves_the_cell_for_the_next_caller() {
    let cell: OnceCell<Rc<usize>> = OnceCell::new();
    let caught = panic_guard::catch(|| shared(&cell, || panic!("build failed"))).unwrap_err();
    assert_eq!(caught.message, "build failed");
    assert!(cell.get().is_none(), "nothing is cached");

    // The next caller builds it afresh, and from then on it is shared.
    assert_eq!(*shared(&cell, || 7), 7);
    assert_eq!(*shared(&cell, || panic!("not built again")), 7);
  }

  /// Every call returns the one tree parsed from the cache's source.
  #[test]
  fn the_tree_is_parsed_once_per_cache() {
    let source = "a { color: red; }\n/* note */\n";
    let cache = FileCache::new(source, Syntax::Css);
    let tree = cache.postcss_tree();
    assert!(Rc::ptr_eq(&tree, &cache.postcss_tree()));
    assert_eq!(tree.nodes.len(), 3);
    assert_eq!(tree.source(), source);
  }

  /// One scan finds the style rules and the at-rules, and is shared.
  #[test]
  fn the_rules_are_scanned_once_per_cache() {
    let source = "@media x { a {} }\nb { @include y; }\n";
    let cache = FileCache::new(source, Syntax::Scss);
    let scanned = cache.scanned_rules();
    assert!(Rc::ptr_eq(&scanned, &cache.scanned_rules()));
    assert_eq!(*scanned, style_rules::scan(source, Syntax::Scss));
    let preludes: Vec<&str> = scanned
      .style_rules
      .iter()
      .map(|r| r.prelude.as_str())
      .collect();
    assert_eq!(preludes, ["a", "b"]);
    let at_rules: Vec<&str> = scanned.at_rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(at_rules, ["media", "include"]);
  }

  /// The current node's declarations are read once and shared until the
  /// runner leaves it; any other node's are read afresh.
  #[test]
  fn declarations_are_shared_for_the_node_being_checked() {
    let source = "a { color: red; top: 0 }\nb { left: 1px }\n";
    let parsed = gale_css_parser::parse(source, Syntax::Css).unwrap();
    let [a, b] = &parsed.nodes[..] else {
      panic!("two rules: {:?}", parsed.nodes);
    };
    let cache = FileCache::new(source, Syntax::Css);
    let props = |found: &[WrittenDeclaration]| -> Vec<String> {
      found.iter().map(|d| d.prop.to_string()).collect()
    };

    // Outside any node nothing is shared.
    assert!(!Rc::ptr_eq(
      &cache.written_declarations(a),
      &cache.written_declarations(a)
    ));

    cache.enter_node(a);
    let first = cache.written_declarations(a);
    assert_eq!(props(&first), ["color", "top"]);
    assert!(Rc::ptr_eq(&first, &cache.written_declarations(a)));
    let other = cache.written_declarations(b);
    assert_eq!(props(&other), ["left"]);
    assert!(!Rc::ptr_eq(&other, &cache.written_declarations(b)));

    cache.enter_node(b);
    let second = cache.written_declarations(b);
    assert_eq!(props(&second), ["left"]);
    assert!(Rc::ptr_eq(&second, &cache.written_declarations(b)));
    assert_eq!(props(&cache.written_declarations(a)), ["color", "top"]);

    cache.leave_node();
    assert!(!Rc::ptr_eq(&second, &cache.written_declarations(b)));
  }

  /// `is_for` matches the very same text and syntax only.
  #[test]
  fn a_cache_belongs_to_one_text_and_syntax() {
    let source = String::from("a {}");
    let copy = source.clone();
    let cache = FileCache::new(&source, Syntax::Scss);
    assert!(cache.is_for(&source, Syntax::Scss));
    assert!(!cache.is_for(&source, Syntax::Css));
    assert!(!cache.is_for(&copy, Syntax::Scss), "equal text elsewhere");
    assert!(!cache.is_for(&source[..2], Syntax::Scss), "a prefix");
  }
}
