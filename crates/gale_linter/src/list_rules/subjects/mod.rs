//! The subjects list rules match: each finder walks the file's
//! [`PostcssTree`] the way the upstream rule walks PostCSS's, and yields
//! what the rule matches against its list with where to report it.
//!
//! Values are split with [`value_parser`] (postcss-value-parser), media
//! queries and raw values with [`css_tokenizer`] (`@csstools/css-tokenizer`)
//! and selectors with [`selector::postcss`] (postcss-selector-parser),
//! whichever the upstream rule uses, so the pieces and their offsets agree
//! with Stylelint's.

use gale_css_parser::Syntax;
use gale_css_parser::css_tokenizer::{self, ComponentValue, Token, TokenType};
use gale_css_parser::postcss_tree::{Node, NodeKind, PostcssTree, RuleRef};
use gale_css_parser::selector::postcss::{self as selector, Kind};
use gale_css_parser::value_parser::{self, ValueNode};

use super::{Found, KeyedFound};
use crate::data;
use crate::standard_syntax;

mod media;
mod scss;
mod selectors;
mod units;

pub use media::*;
pub use scss::*;
pub use selectors::*;
pub use units::*;

/// Every node of `kind`, with its index, in document order.
fn nodes_of<'t>(tree: &'t PostcssTree, kind: NodeKind) -> impl Iterator<Item = (usize, &'t Node)> {
  tree
    .nodes
    .iter()
    .enumerate()
    .filter(move |(_, node)| node.kind == kind)
}

/// Every at-rule whose name is `name`, ignoring case (Stylelint's
/// `atRuleRegexes.*Name`).
fn at_rules_named<'t>(
  tree: &'t PostcssTree,
  name: &'static str,
) -> impl Iterator<Item = (usize, &'t Node)> {
  nodes_of(tree, NodeKind::AtRule).filter(move |(_, node)| node.name.eq_ignore_ascii_case(name))
}

/// An at-rule's params or a declaration's value as written, comments
/// included (`getAtRuleParams`, `getDeclarationValue`), and where it
/// starts.
fn raw_params<'t>(tree: &PostcssTree<'t>, node: &Node) -> (&'t str, usize) {
  let span = node.value_span.clone();
  (tree.source().get(span.clone()).unwrap_or(""), span.start)
}

/// Every standard style rule (`isStandardSyntaxRule`), with its selector as
/// written (`getRuleSelector`) and where that starts.
fn standard_rules<'t>(
  tree: &'t PostcssTree,
) -> impl Iterator<Item = (usize, &'t Node, &'t str)> + 't {
  nodes_of(tree, NodeKind::Rule)
    .filter(|&(i, _)| tree.is_standard_syntax_rule(i))
    .map(|(i, node)| {
      let raw = tree.source().get(node.name_span.clone()).unwrap_or("");
      (i, node, raw)
    })
}

/// At-rule names, without the `@`: `at-rule-*-list`.  Non-standard
/// at-rules (`@charset`, Less mixins and variables, SCSS `@content`) are
/// skipped.
pub fn at_rule_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::AtRule) {
    if tree.is_standard_syntax_at_rule(i) {
      found.push(Found::new(
        node.name.as_str(),
        i,
        node.start,
        node.start + 1 + node.name.len(),
      ));
    }
  }
}

/// Declaration properties: `property-*-list`.  Sass and Less variables,
/// Less merges and interpolated properties are skipped.
pub fn property_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Decl) {
    if standard_syntax::is_standard_syntax_property(&node.name) {
      found.push(Found::new(
        node.name.as_str(),
        i,
        node.name_span.start,
        node.name_span.end,
      ));
    }
  }
}

/// The text of every comment but a `/*# sourceMappingURL */` one, reported
/// where the text starts: `comment-word-disallowed-list`.
pub fn comment_texts(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Comment) {
    let raw = tree.text(i);
    if raw.starts_with("/*# ") {
      continue;
    }
    let at = raw.find(node.name.as_str()).unwrap_or(0);
    let start = node.start + at;
    found.push(Found::new(
      node.name.as_str(),
      i,
      start,
      start + node.name.len(),
    ));
  }
}

/// The name of every function in a declaration value: `function-*-list`.
/// Nameless parentheses and interpolated names are skipped.
pub fn function_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Decl) {
    if !node.value.contains('(') {
      continue;
    }
    let base = node.value_span.start;
    value_parser::walk(&value_parser::parse(&node.value), &mut |value| {
      if value.is_function() && standard_syntax::is_standard_syntax_function(value) {
        let start = base + value.source_index;
        found.push(Found::new(value.value, i, start, start + value.value.len()));
      }
      true
    });
  }
}

/// Stylelint's `hasPrevPropertyDeclaration`: whether a declaration of the
/// same property (ignoring case) comes before declaration `i` in its block,
/// the fallback `function-allowed-list`'s `exceptWithoutPropertyFallback`
/// asks for.  Never for a custom property.
pub fn has_fallback_declaration(tree: &PostcssTree, i: usize) -> bool {
  let prop = tree.nodes[i].name.to_ascii_lowercase();
  if prop.starts_with("--") {
    return false;
  }
  let mut prev = tree.prev(i);
  while let Some(p) = prev {
    let node = &tree.nodes[p];
    if node.kind == NodeKind::Decl && node.name.eq_ignore_ascii_case(&prop) {
      return true;
    }
    prev = tree.prev(p);
  }
  false
}

/// The scheme of every `url()` in a declaration, lowercased as Stylelint
/// lowercases the whole declaration first, reported at the argument:
/// `function-url-scheme-*-list`.
pub fn url_schemes(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Decl) {
    let text = tree.text(i).trim_end_matches(';').to_ascii_lowercase();
    for (args, index) in function_arguments(&text, "url") {
      let unspaced = args.trim();
      if !standard_syntax::is_standard_syntax_url(unspaced) {
        continue;
      }
      let url = unspaced.trim_matches(['\'', '"']);
      if let Some(scheme) = url_scheme(url) {
        let start = node.start + index;
        found.push(Found::new(scheme, i, start, start + args.len()));
      }
    }
  }
}

/// Stylelint's `functionArgumentsSearch`: the argument text of every call
/// to `name` in `source`, with where it starts.
fn function_arguments<'s>(source: &'s str, name: &str) -> Vec<(&'s str, usize)> {
  let mut out = Vec::new();
  value_parser::walk(&value_parser::parse(source), &mut |node| {
    if !node.is_function() || node.value != name {
      return true;
    }
    let open = node.source_index + node.value.len();
    let start = open + 1;
    let end = if node.value.eq_ignore_ascii_case("url") {
      // Re-parse from the `(` as a nameless group, as upstream does, so
      // interpolation inside an unquoted url keeps its extent.
      let group_end = value_parser::parse(source.get(open..).unwrap_or(""))
        .first()
        .map_or(1, |group| group.source_end_index);
      let close = source.as_bytes().get((start + group_end).wrapping_sub(2)) == Some(&b')');
      (start + group_end - 1).saturating_sub(usize::from(close))
    } else {
      let close = source.as_bytes().get(node.source_end_index.wrapping_sub(1)) == Some(&b')');
      node.source_end_index - usize::from(close)
    };
    if let Some(args) = source.get(start..end.max(start)) {
      out.push((args, start));
    }
    true
  });
  out
}

/// Stylelint's `getSchemeFromUrl`: the scheme of an absolute URL that is
/// followed by `//` (or is `data:`), so `host:port` is not taken for one.
fn url_scheme(url: &str) -> Option<&str> {
  let url = url.trim_start_matches(|c: char| c <= ' ');
  let colon = url.find(':')?;
  let scheme = &url[..colon];
  let mut chars = scheme.chars();
  let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
  if !valid {
    return None;
  }
  let rest = &url[colon + 1..];
  if scheme.eq_ignore_ascii_case("data") {
    return Some(scheme);
  }
  // `new URL()` rejects a special scheme without a host.
  let special = ["http", "https", "ws", "wss", "ftp"]
    .iter()
    .any(|s| scheme.eq_ignore_ascii_case(s));
  if !rest.starts_with("//") || (special && rest.len() <= 2) {
    return None;
  }
  Some(scheme)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_scheme_needs_slashes_unless_it_is_data() {
    assert_eq!(url_scheme("https://a.b/c"), Some("https"));
    assert_eq!(url_scheme("data:image/png;base64,AA"), Some("data"));
    assert_eq!(url_scheme("localhost:8080/a"), None);
    assert_eq!(url_scheme("http://"), None);
    assert_eq!(url_scheme("//cdn/a.png"), None);
    assert_eq!(url_scheme("a/b.png"), None);
  }

  #[test]
  fn partial_extensions_skip_css_imports() {
    assert_eq!(partial_extension(" \"a/b.scss\""), Some("scss"));
    assert_eq!(partial_extension("'.scss'"), None);
    assert_eq!(partial_extension("\"a.css\""), None);
    assert_eq!(partial_extension("url(a.scss)"), None);
    assert_eq!(partial_extension("\"a.scss\" screen"), None);
  }

  #[test]
  fn media_features_are_found_in_every_form() {
    let names = |params: &str| -> Vec<String> {
      media_features(params)
        .into_iter()
        .map(|feature| feature.name.value)
        .collect()
    };
    assert_eq!(
      names("screen and (min-width: 1px), not ((color) or (10px < width < 20px))"),
      ["min-width", "color", "width"]
    );
    assert_eq!(names("(1px >= height)"), ["height"]);
    assert!(names("print").is_empty());
  }
}
