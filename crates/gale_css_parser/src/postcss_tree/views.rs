//! Typed views of the nodes of a [`PostcssTree`], for rules to read the
//! tree the way a Stylelint rule reads PostCSS's.
//!
//! A [`NodeRef`] is a node together with its tree, so that it can answer
//! for its relatives (`parent()`, `children()`, `prev()`, `walk()`) and
//! its text.  [`RuleRef`], [`AtRuleRef`], [`DeclRef`] and [`CommentRef`]
//! narrow it to one kind and name the parts PostCSS names: a rule's
//! `selector`, an at-rule's `name` and `params`, a declaration's `prop`,
//! `value` and `important`, a comment's `text`.  Every view is two words
//! and `Copy`, and dereferences to the less specific one, down to the
//! [`Node`] and its fields.
//!
//! The walks follow PostCSS: [`PostcssTree::walk`] visits every node in
//! document order (a parent before its children), and `walk_rules`,
//! `walk_at_rules`, `walk_decls` and `walk_comments` keep the nodes of one
//! kind.  The same walks on a [`NodeRef`] visit its descendants only.
//!
//! Text comes in two readings where PostCSS has two.  `selector()`,
//! `params()` and `value()` are what PostCSS stores (`rule.selector`,
//! `atRule.params`, `decl.value`), with the comments it cleans out removed;
//! `raw_selector()`, `raw_params()` and `raw_value()` are the text as
//! written, as Stylelint's `getRuleSelector`, `getAtRuleParams` and
//! `getDeclarationValue` read it, and the matching `*_start()` is where
//! that text starts in the source.

use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;

use gale_diagnostics::Span;

use super::{Node, NodeKind, PostcssTree};

/// A node of a [`PostcssTree`], with the tree it belongs to.
///
/// Dereferences to the [`Node`], whose fields hold indexes (`parent`,
/// `children`) where this view's methods of the same names give views.
#[derive(Clone, Copy)]
pub struct NodeRef<'t> {
  tree: &'t PostcssTree<'t>,
  id: usize,
}

impl<'a> PostcssTree<'a> {
  /// The view of node `id`.
  pub fn node<'t>(&'t self, id: usize) -> NodeRef<'t>
  where
    'a: 't,
  {
    NodeRef { tree: self, id }
  }

  /// Every node, in document order: a parent before its children
  /// (PostCSS's `root.walk()`).
  pub fn walk<'t>(&'t self) -> impl Iterator<Item = NodeRef<'t>>
  where
    'a: 't,
  {
    (0..self.nodes.len()).map(move |id| self.node(id))
  }

  /// The top-level nodes (PostCSS's `root.nodes`).
  pub fn root_nodes<'t>(&'t self) -> impl Iterator<Item = NodeRef<'t>>
  where
    'a: 't,
  {
    self.root.iter().map(move |&id| self.node(id))
  }

  /// Every style rule, in document order (`root.walkRules()`).
  pub fn walk_rules<'t>(&'t self) -> impl Iterator<Item = RuleRef<'t>>
  where
    'a: 't,
  {
    self.walk().filter_map(NodeRef::as_rule)
  }

  /// Every at-rule, in document order (`root.walkAtRules()`).
  pub fn walk_at_rules<'t>(&'t self) -> impl Iterator<Item = AtRuleRef<'t>>
  where
    'a: 't,
  {
    self.walk().filter_map(NodeRef::as_at_rule)
  }

  /// Every declaration, in document order (`root.walkDecls()`).
  pub fn walk_decls<'t>(&'t self) -> impl Iterator<Item = DeclRef<'t>>
  where
    'a: 't,
  {
    self.walk().filter_map(NodeRef::as_decl)
  }

  /// Every comment, in document order (`root.walkComments()`).
  pub fn walk_comments<'t>(&'t self) -> impl Iterator<Item = CommentRef<'t>>
  where
    'a: 't,
  {
    self.walk().filter_map(NodeRef::as_comment)
  }
}

impl<'t> NodeRef<'t> {
  /// The node's index in [`PostcssTree::nodes`], which the tree's
  /// index-based helpers take.
  pub fn id(self) -> usize {
    self.id
  }

  /// The tree the node belongs to.
  pub fn tree(self) -> &'t PostcssTree<'t> {
    self.tree
  }

  /// The node's data.
  pub fn data(self) -> &'t Node {
    &self.tree.nodes[self.id]
  }

  /// The node as a style rule, if it is one.
  pub fn as_rule(self) -> Option<RuleRef<'t>> {
    (self.kind == NodeKind::Rule).then_some(RuleRef(self))
  }

  /// The node as an at-rule, if it is one.
  pub fn as_at_rule(self) -> Option<AtRuleRef<'t>> {
    (self.kind == NodeKind::AtRule).then_some(AtRuleRef(self))
  }

  /// The node as a declaration, if it is one.
  pub fn as_decl(self) -> Option<DeclRef<'t>> {
    (self.kind == NodeKind::Decl).then_some(DeclRef(self))
  }

  /// The node as a comment, if it is one.
  pub fn as_comment(self) -> Option<CommentRef<'t>> {
    (self.kind == NodeKind::Comment).then_some(CommentRef(self))
  }

  /// The enclosing node, `None` at the top level (PostCSS's `parent`,
  /// with the root left out).
  pub fn parent(self) -> Option<NodeRef<'t>> {
    self.data().parent.map(|id| self.tree.node(id))
  }

  /// The enclosing nodes, innermost first.
  pub fn ancestors(self) -> impl Iterator<Item = NodeRef<'t>> {
    std::iter::successors(self.parent(), |node| node.parent())
  }

  /// Whether the node has a block (PostCSS's `nodes !== undefined`).
  pub fn has_block(self) -> bool {
    self.data().children.is_some()
  }

  /// The nodes directly inside the block, none when there is no block.
  pub fn children(self) -> impl DoubleEndedIterator<Item = NodeRef<'t>> + ExactSizeIterator {
    let tree = self.tree;
    let children = self.data().children.as_deref().unwrap_or(&[]);
    children.iter().map(move |&id| tree.node(id))
  }

  /// The declarations directly inside the block.
  pub fn child_decls(self) -> impl Iterator<Item = DeclRef<'t>> {
    self.children().filter_map(NodeRef::as_decl)
  }

  /// The sibling before the node (PostCSS's `prev()`).
  pub fn prev(self) -> Option<NodeRef<'t>> {
    self.tree.prev(self.id).map(|id| self.tree.node(id))
  }

  /// The sibling after the node (PostCSS's `next()`).
  pub fn next(self) -> Option<NodeRef<'t>> {
    self.tree.next(self.id).map(|id| self.tree.node(id))
  }

  /// Whether the node sits directly in a style rule's block.
  pub fn parent_is_rule(self) -> bool {
    self
      .parent()
      .is_some_and(|parent| parent.as_rule().is_some())
  }

  /// Whether the node sits directly in a style rule's block, counting an
  /// `@nest` at-rule (the nesting draft's style rule) as one.
  pub fn in_style_rule(self) -> bool {
    self.parent_is_rule()
      || self
        .parent()
        .and_then(NodeRef::as_at_rule)
        .is_some_and(|at| at.name().eq_ignore_ascii_case("nest"))
  }

  /// Whether the node sits in a `@keyframes` block (vendor-prefixed or
  /// not), as keyframe selectors and anything nested under them do.
  pub fn in_keyframes(self) -> bool {
    self.ancestors().any(|node| {
      node.as_at_rule().is_some_and(|at| {
        let name = at.name().as_bytes();
        name.len() >= 9 && name[name.len() - 9..].eq_ignore_ascii_case(b"keyframes")
      })
    })
  }

  /// Every node inside this one, in document order (PostCSS's
  /// `node.walk()`).  A subtree is one run of [`PostcssTree::nodes`], from
  /// the node to its last descendant.
  pub fn walk(self) -> impl Iterator<Item = NodeRef<'t>> {
    let tree = self.tree;
    let mut last = self.id;
    while let Some(&child) = tree.nodes[last].children.as_ref().and_then(|c| c.last()) {
      last = child;
    }
    (self.id + 1..=last).map(move |id| tree.node(id))
  }

  /// Every style rule inside this node (`node.walkRules()`).
  pub fn walk_rules(self) -> impl Iterator<Item = RuleRef<'t>> {
    self.walk().filter_map(NodeRef::as_rule)
  }

  /// Every at-rule inside this node (`node.walkAtRules()`).
  pub fn walk_at_rules(self) -> impl Iterator<Item = AtRuleRef<'t>> {
    self.walk().filter_map(NodeRef::as_at_rule)
  }

  /// Every declaration inside this node (`node.walkDecls()`).
  pub fn walk_decls(self) -> impl Iterator<Item = DeclRef<'t>> {
    self.walk().filter_map(NodeRef::as_decl)
  }

  /// Every comment inside this node (`node.walkComments()`).
  pub fn walk_comments(self) -> impl Iterator<Item = CommentRef<'t>> {
    self.walk().filter_map(NodeRef::as_comment)
  }

  /// The node's source span, from its first character to just past its
  /// last (a declaration's `;` and a block's `}` included).
  pub fn span(self) -> Span {
    let node = self.data();
    Span::from_range(node.start, node.end.max(node.start))
  }

  /// Where Stylelint's `report({ node, word })` puts `word`: its first
  /// occurrence in the node's text, or the whole node when it is not there.
  pub fn word_span(self, word: &str) -> Span {
    self.source_text().find(word).map_or(self.span(), |offset| {
      Span::new(self.data().start + offset, word.len())
    })
  }

  /// The node's text as written (PostCSS's `toString()` without
  /// `raws.before`).
  pub fn source_text(self) -> &'t str {
    self.tree.text(self.id)
  }

  /// The node's `raws.before`.
  pub fn before(self) -> &'t str {
    self.tree.before(self.id)
  }

  /// The line the node starts on (Stylelint's `getNodeLine`).
  pub fn start_line(self) -> usize {
    self.tree.start_line(self.id)
  }

  /// The line of the node's last character.
  pub fn end_line(self) -> usize {
    self.tree.end_line(self.id)
  }
}

impl<'t> Deref for NodeRef<'t> {
  type Target = Node;

  fn deref(&self) -> &Node {
    self.data()
  }
}

impl PartialEq for NodeRef<'_> {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id && std::ptr::eq(self.tree, other.tree)
  }
}

impl Eq for NodeRef<'_> {}

impl fmt::Debug for NodeRef<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("NodeRef")
      .field("id", &self.id)
      .field("kind", &self.kind)
      .field("text", &self.source_text())
      .finish()
  }
}

/// Declare a view of one kind of node, which dereferences to its
/// [`NodeRef`].
macro_rules! typed_view {
  ($(#[$attr:meta])* $name:ident) => {
    $(#[$attr])*
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct $name<'t>(NodeRef<'t>);

    impl<'t> $name<'t> {
      /// The node, as a view of any kind.
      pub fn node(self) -> NodeRef<'t> {
        self.0
      }
    }

    impl<'t> Deref for $name<'t> {
      type Target = NodeRef<'t>;

      fn deref(&self) -> &NodeRef<'t> {
        &self.0
      }
    }
  };
}

typed_view! {
  /// A style rule (PostCSS's `Rule`).
  RuleRef
}

typed_view! {
  /// An at-rule (PostCSS's `AtRule`).
  AtRuleRef
}

typed_view! {
  /// A declaration (PostCSS's `Declaration`).
  DeclRef
}

typed_view! {
  /// A comment (PostCSS's `Comment`).
  CommentRef
}

impl<'t> RuleRef<'t> {
  /// The selector as PostCSS stores it (`rule.selector`): the source text
  /// with the comments between its tokens removed and the whitespace
  /// before the `{` left out.
  pub fn selector(self) -> &'t str {
    &self.0.data().name
  }

  /// The selector as written, comments included, as Stylelint's
  /// `getRuleSelector` reads it (see [`PostcssTree::raw_text`]).
  pub fn raw_selector(self) -> Cow<'t, str> {
    self.0.tree.raw_text(self.0.data().name_span.clone())
  }

  /// Where [`Self::raw_selector`] starts in the source.
  pub fn selector_start(self) -> usize {
    self.0.data().name_span.start
  }

  /// Whether offsets into [`Self::raw_selector`] line up with the source, so that
  /// a fix computed from them lands on target.  False when postcss-scss
  /// rewrote a `//` comment in it as `/* */`, which runs every offset after
  /// the comment two bytes ahead: rules then report but do not fix.
  pub fn selector_fixable(self) -> bool {
    matches!(self.raw_selector(), Cow::Borrowed(_))
  }

  /// The style rule this one is nested in, through any at-rules between
  /// them: the rule whose selector `&` stands for.
  pub fn parent_rule(self) -> Option<RuleRef<'t>> {
    self.0.ancestors().find_map(NodeRef::as_rule)
  }

  /// Stylelint's `isStandardSyntaxRule`.
  pub fn is_standard_syntax(self) -> bool {
    self.0.tree.is_standard_syntax_rule(self.0.id)
  }

  /// Stylelint's `isKeyframeRule`: whether the rule sits directly in an
  /// at-rule named `keyframes`, in any case but without a vendor prefix.
  pub fn is_keyframe_rule(self) -> bool {
    self
      .0
      .parent()
      .and_then(NodeRef::as_at_rule)
      .is_some_and(|at| at.name().eq_ignore_ascii_case("keyframes"))
  }
}

impl<'t> AtRuleRef<'t> {
  /// The name without the `@`, as written (`atRule.name`).
  pub fn name(self) -> &'t str {
    &self.0.data().name
  }

  /// Where the name starts in the source, just past the `@`.
  pub fn name_start(self) -> usize {
    self.0.data().name_span.start
  }

  /// The params as PostCSS stores them (`atRule.params`): the comments
  /// next to whitespace removed.
  pub fn params(self) -> &'t str {
    &self.0.data().params
  }

  /// The params as written, as Stylelint's `getAtRuleParams` reads them
  /// (see [`PostcssTree::raw_text`]).
  pub fn raw_params(self) -> Cow<'t, str> {
    self.0.tree.raw_text(self.0.data().value_span.clone())
  }

  /// Where [`Self::raw_params`] starts in the source.
  pub fn params_start(self) -> usize {
    self.0.data().value_span.start
  }

  /// Whether offsets into [`Self::raw_params`] line up with the source, so that
  /// a fix computed from them lands on target.  False when postcss-scss
  /// rewrote a `//` comment in it as `/* */`, which runs every offset after
  /// the comment two bytes ahead: rules then report but do not fix.
  pub fn params_fixable(self) -> bool {
    matches!(self.raw_params(), Cow::Borrowed(_))
  }

  /// stylelint-scss's `atRuleParamIndex`: where the params start, counted
  /// from the `@`.
  pub fn param_index(self) -> usize {
    self.0.tree.at_rule_param_index(self.0.data())
  }

  /// Stylelint's `isStandardSyntaxAtRule`.
  pub fn is_standard_syntax(self) -> bool {
    self.0.tree.is_standard_syntax_at_rule(self.0.id)
  }

  /// Whether the at-rule is `@keyframes`, vendor-prefixed or not, in any
  /// case: Stylelint's `atRuleRegexes.keyframesName`,
  /// `/^(-(o|moz|ms|webkit)-)?keyframes$/i`.
  pub fn is_keyframes(self) -> bool {
    let name = self.name().to_ascii_lowercase();
    let unprefixed = ["-o-", "-moz-", "-ms-", "-webkit-"]
      .iter()
      .find_map(|prefix| name.strip_prefix(prefix))
      .unwrap_or(&name);
    unprefixed == "keyframes"
  }
}

impl<'t> DeclRef<'t> {
  /// The property as written, without a `*` or `_` hack (`decl.prop`).
  pub fn prop(self) -> &'t str {
    &self.0.data().name
  }

  /// Where [`Self::prop`] starts in the source.
  pub fn prop_start(self) -> usize {
    self.0.data().name_span.start
  }

  /// The value as PostCSS stores it (`decl.value`): `!important` left out
  /// and the comments next to whitespace removed.
  pub fn value(self) -> &'t str {
    &self.0.data().value
  }

  /// The value as written, comments kept and `!important` left out, as
  /// Stylelint's `getDeclarationValue` reads it (see
  /// [`PostcssTree::raw_text`]).
  pub fn raw_value(self) -> Cow<'t, str> {
    self.0.tree.raw_text(self.0.data().value_span.clone())
  }

  /// The value exactly as the source has it, `!important` left out:
  /// [`Self::raw_value`] before postcss-scss rewrites its `//` comments, so
  /// that offsets into it are offsets into the source from
  /// [`Self::value_start`].
  pub fn written_value(self) -> &'t str {
    let span = self.0.data().value_span.clone();
    self.0.tree.source().get(span).unwrap_or("")
  }

  /// Where [`Self::raw_value`] starts in the source.
  pub fn value_start(self) -> usize {
    self.0.data().value_span.start
  }

  /// Whether offsets into [`Self::raw_value`] line up with the source, so that
  /// a fix computed from them lands on target.  False when postcss-scss
  /// rewrote a `//` comment in it as `/* */`, which runs every offset after
  /// the comment two bytes ahead: rules then report but do not fix.
  pub fn value_fixable(self) -> bool {
    matches!(self.raw_value(), Cow::Borrowed(_))
  }

  /// Where the `;` that ends the declaration is, when it has one.
  pub fn semicolon(self) -> Option<usize> {
    self
      .source_text()
      .ends_with(';')
      .then(|| self.span().end() - 1)
  }

  /// Whether the declaration is `!important` (`decl.important`).
  pub fn important(self) -> bool {
    self.0.data().important
  }

  /// Whether the property is a custom property (`--name`).
  pub fn is_custom_property(self) -> bool {
    self.prop().starts_with("--")
  }

  /// stylelint-scss's `declarationValueIndex`: where the value starts,
  /// counted from the start of the declaration.
  pub fn value_index(self) -> Option<usize> {
    self.0.tree.declaration_value_index(self.0.data())
  }

  /// Stylelint's `isStandardSyntaxDeclaration`.
  pub fn is_standard_syntax(self) -> bool {
    self.0.tree.is_standard_syntax_declaration(self.0.id)
  }
}

impl<'t> CommentRef<'t> {
  /// The comment's text without its delimiters and the whitespace around
  /// it (`comment.text`).
  pub fn text(self) -> &'t str {
    &self.0.data().name
  }

  /// Whether it is a `//` comment (`raws.inline`).
  pub fn is_inline(self) -> bool {
    self.0.data().inline
  }
}

#[cfg(test)]
mod tests {
  use crate::Syntax;

  use super::*;

  #[test]
  fn walks_visit_nodes_in_document_order() {
    let source = "/* a */\n@media x { b { color: red; top: 0 } }\nc { left: 1px }";
    let tree = PostcssTree::parse(source, Syntax::Css);
    let texts: Vec<&str> = tree.walk().map(|n| n.source_text()).collect();
    assert_eq!(
      texts,
      [
        "/* a */",
        "@media x { b { color: red; top: 0 } }",
        "b { color: red; top: 0 }",
        "color: red;",
        "top: 0",
        "c { left: 1px }",
        "left: 1px",
      ]
    );
    let props: Vec<&str> = tree.walk_decls().map(DeclRef::prop).collect();
    assert_eq!(props, ["color", "top", "left"]);
    let semicolons: Vec<Option<usize>> = tree.walk_decls().map(DeclRef::semicolon).collect();
    assert_eq!(semicolons, [source.find(';'), None, None]);
    let selectors: Vec<&str> = tree.walk_rules().map(RuleRef::selector).collect();
    assert_eq!(selectors, ["b", "c"]);
    let comments: Vec<&str> = tree.walk_comments().map(CommentRef::text).collect();
    assert_eq!(comments, ["a"]);
    let roots: Vec<NodeKind> = tree.root_nodes().map(|n| n.kind).collect();
    assert_eq!(roots, [NodeKind::Comment, NodeKind::AtRule, NodeKind::Rule]);

    // A node's walks stay inside it.
    let media = tree.walk_at_rules().next().unwrap();
    assert_eq!((media.name(), media.params()), ("media", "x"));
    let inside: Vec<&str> = media.walk_decls().map(DeclRef::prop).collect();
    assert_eq!(inside, ["color", "top"]);
    let last = tree.root_nodes().last().unwrap();
    assert_eq!(last.walk().count(), 1);
    assert_eq!(tree.walk_decls().last().unwrap().walk().count(), 0);
  }

  #[test]
  fn relatives_are_views_too() {
    let source = "a { b { color: red } /* c */ top: 0 }";
    let tree = PostcssTree::parse(source, Syntax::Scss);
    let decl = tree.walk_decls().next().unwrap();
    let rule = decl.parent().unwrap().as_rule().unwrap();
    assert_eq!(rule.selector(), "b");
    let outer: Vec<&str> = decl.ancestors().map(|n| n.source_text()).collect();
    assert_eq!(outer, ["b { color: red }", source]);
    assert_eq!(
      rule.next().and_then(NodeRef::as_comment).unwrap().text(),
      "c"
    );
    assert_eq!(rule.prev(), None);
    let a = rule.parent().unwrap();
    assert_eq!(a.children().len(), 3);
    let own: Vec<&str> = a.child_decls().map(DeclRef::prop).collect();
    assert_eq!(own, ["top"]);
    assert!(a.has_block() && !decl.has_block());
    assert_eq!(decl.node(), tree.node(decl.id()));
    assert_eq!(decl.span(), Span::from_range(8, 18));
  }

  #[test]
  fn texts_come_cleaned_and_raw_with_their_offsets() {
    let source = "a /* x */ , b { color: red /* y */  !important; }\n@media /* m */ screen { }";
    let tree = PostcssTree::parse(source, Syntax::Css);
    let rule = tree.walk_rules().next().unwrap();
    assert_eq!(rule.selector(), "a  , b");
    assert_eq!(rule.raw_selector(), "a /* x */ , b");
    assert_eq!(rule.selector_start(), 0);

    let decl = tree.walk_decls().next().unwrap();
    assert_eq!(decl.prop(), "color");
    assert_eq!(decl.prop_start(), 16);
    assert_eq!(decl.value(), "red ");
    assert_eq!(decl.raw_value(), "red /* y */");
    assert_eq!(&source[decl.value_start()..][..3], "red");
    assert!(decl.important());
    assert_eq!(decl.value_index(), Some(7));

    let media = tree.walk_at_rules().next().unwrap();
    assert_eq!(media.name(), "media");
    assert_eq!(&source[media.name_start()..][..5], "media");
    assert_eq!(media.params(), "screen");
    assert_eq!(media.raw_params(), "screen");
    assert_eq!(&source[media.params_start()..][..6], "screen");
    assert_eq!(media.param_index(), 1 + "media".len() + " /* m */ ".len());
  }

  #[test]
  fn rules_know_their_nesting() {
    let source = "a { @media x { b { } } }\n@-webkit-keyframes k { from { } }";
    let tree = PostcssTree::parse(source, Syntax::Scss);
    let rules: Vec<RuleRef> = tree.walk_rules().collect();
    let parents: Vec<Option<&str>> = rules
      .iter()
      .map(|r| r.parent_rule().map(RuleRef::selector))
      .collect();
    assert_eq!(parents, [None, Some("a"), None]);
    let keyframes: Vec<bool> = rules.iter().map(|r| r.in_keyframes()).collect();
    assert_eq!(keyframes, [false, false, true]);
  }

  #[test]
  fn keyframes_are_named_as_stylelint_names_them() {
    let source = "@KEYFRAMES a { from {} } @-webkit-keyframes b { to {} } @x-keyframes c {}";
    let tree = PostcssTree::parse(source, Syntax::Css);
    let at_rules: Vec<bool> = tree.walk_at_rules().map(AtRuleRef::is_keyframes).collect();
    assert_eq!(at_rules, [true, true, false]);
    // `isKeyframeRule` takes no vendor prefix.
    let rules: Vec<bool> = tree.walk_rules().map(RuleRef::is_keyframe_rule).collect();
    assert_eq!(rules, [true, false]);
  }

  #[test]
  fn scss_raw_text_writes_line_comments_as_block_comments() {
    let source = "a, // x */\nb { c: d // e\n; @media x // y\n{} }";
    let tree = PostcssTree::parse(source, Syntax::Scss);
    let rule = tree.walk_rules().next().unwrap();
    assert_eq!(rule.raw_selector(), "a, /* x *//**/\nb");
    assert_eq!(rule.selector_start(), 0);
    let decl = tree.walk_decls().next().unwrap();
    assert_eq!(decl.raw_value(), "d /* e*/\n");
    assert_eq!(decl.written_value(), "d // e\n");
    let media = tree.walk_at_rules().next().unwrap();
    assert_eq!(media.raw_params(), "x");
    // A rewritten reading runs ahead of the source, so no fix is safe.
    assert!(!rule.selector_fixable());
    assert!(!decl.value_fixable());
    assert!(media.params_fixable());

    // Plain CSS has no line comments to rewrite.
    let css = PostcssTree::parse("a, b // c { }", Syntax::Css);
    let rule = css.walk_rules().next().unwrap();
    assert_eq!(rule.raw_selector(), "a, b // c");
    assert!(rule.selector_fixable());
  }

  #[test]
  fn nodes_know_whether_a_style_rule_holds_them() {
    let source = "a { b: c; @media x { d: e; } }
@nest f & { g: h; }
i: j;";
    let tree = PostcssTree::parse(source, Syntax::Scss);
    let parents: Vec<(&str, bool, bool)> = tree
      .walk_decls()
      .map(|decl| (decl.prop(), decl.parent_is_rule(), decl.in_style_rule()))
      .collect();
    assert_eq!(
      parents,
      [
        ("b", true, true),
        ("d", false, false),
        ("g", false, true),
        ("i", false, false)
      ]
    );
  }

  #[test]
  fn comments_know_their_form() {
    let tree = PostcssTree::parse("// line\n/* block */", Syntax::Scss);
    let forms: Vec<(&str, bool)> = tree
      .walk_comments()
      .map(|c| (c.text(), c.is_inline()))
      .collect();
    assert_eq!(forms, [("line", true), ("block", false)]);
  }
}
