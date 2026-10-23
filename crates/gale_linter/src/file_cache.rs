//! What every rule linting a file shares, built once per file.
//!
//! Rules read a file through its PostCSS-shaped statement tree
//! ([`PostcssTree`]).  The runner parses it once into a [`FileCache`],
//! before any rule runs and outside the rules' panic guards, so a parser
//! bug is one internal error on the file rather than one per rule; every
//! rule and the disable comments then share it through
//! [`RuleContext`](crate::rule::RuleContext).

use std::rc::Rc;

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;

/// The artifacts shared by every rule linting one file, built from
/// [`Self::source`] parsed as [`Self::syntax`].
pub struct FileCache<'a> {
  /// The text every artifact is built from: the text the rules lint.
  source: &'a str,
  /// The syntax `source` is parsed as.
  syntax: Syntax,
  /// The statements as PostCSS sees them.
  postcss_tree: Rc<PostcssTree<'a>>,
}

impl<'a> FileCache<'a> {
  /// The cache for `source`, parsed as `syntax`.
  pub fn new(source: &'a str, syntax: Syntax) -> Self {
    Self {
      source,
      syntax,
      postcss_tree: Rc::new(PostcssTree::parse(source, syntax)),
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

  /// The file's statements as PostCSS sees them.
  pub fn postcss_tree(&self) -> Rc<PostcssTree<'a>> {
    Rc::clone(&self.postcss_tree)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
