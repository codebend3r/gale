//! A port of [postcss-selector-parser], the selector parser Stylelint's
//! selector rules walk, keeping every node's position in the source.
//!
//! [`super::parser`] builds Gale's own selector AST, which carries no
//! positions.  The autofixing selector rules need to know exactly where a
//! tag, pseudo-class or attribute value sits in the text the author wrote,
//! and need to agree with Stylelint on what each piece is, so they parse
//! with this instead.  It follows postcss-selector-parser's tokenizer and
//! parser, including its quirks (a pseudo-class argument is always parsed
//! as a selector list, comments between compounds fold into the
//! combinator), and fails wherever that parser throws.
//!
//! Offsets are byte offsets into the source the selector came from: the
//! `base` passed to [`parse`] plus the position within the selector text.
//!
//! [postcss-selector-parser]: https://github.com/postcss/postcss-selector-parser

mod escape;
mod list;
mod parser;
mod tokenizer;

pub use escape::{Escape, cssesc};
pub use list::{
  blank_comments, rule_selectors, split_selectors, strip_comments, stripped_selector,
};
use parser::Parser;
use tokenizer::tokenize;

/// What kind of selector node this is, as postcss-selector-parser names
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  /// A type selector, or anything else written as a bare word.
  Tag,
  /// `.name`
  Class,
  /// `#name`
  Id,
  /// `*`
  Universal,
  /// `&`
  Nesting,
  /// `:name` or `::name`, with any parenthesised argument.
  Pseudo,
  /// `[...]`
  Attribute,
  /// `>`, `+`, `~`, `||`, `/name/`, or whitespace (`" "`).
  Combinator,
  /// `/* ... */`
  Comment,
  /// A quoted string, a parenthesised group that follows no pseudo-class,
  /// or whitespace with nothing to attach to.
  String,
}

/// One node of a selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
  pub kind: Kind,
  /// The node's text as written: a tag or class name (without `.`/`#`), a
  /// pseudo's colons and name, a combinator, a comment with its
  /// delimiters, `&` or `*`.  Attributes keep their parts in [`Self::attr`]
  /// and have the whole `[...]` here.
  pub value: String,
  /// Byte offset where the node starts (`sourceIndex`).
  pub start: usize,
  /// Exclusive byte offset where the node ends.  A pseudo's end includes
  /// its argument.
  pub end: usize,
  /// The namespace prefix of a tag, universal or attribute (`svg` in
  /// `svg|a`; empty for `|a`).
  pub namespace: Option<String>,
  /// The argument of a pseudo-class or pseudo-element.
  pub args: Option<Args>,
  /// The parts of an attribute selector.
  pub attr: Option<Attr>,
}

/// The parenthesised argument of a pseudo, parsed as a selector list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
  /// Byte offset of the `(`.
  pub open: usize,
  /// Byte offset of the matching `)`.
  pub close: usize,
  pub selectors: Vec<Selector>,
}

/// The parts of an attribute selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
  /// The attribute name as written.
  pub name: String,
  /// `=`, `~=`, `|=`, `^=`, `$=` or `*=`.
  pub operator: Option<String>,
  /// The value, when there is one.
  pub value: Option<AttrValue>,
  /// The `i`/`s` flag after the value, as written.
  pub flag: Option<String>,
}

/// An attribute selector's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrValue {
  /// The value as written, quotes and escapes included.
  pub raw: String,
  /// The value with quotes removed and escapes resolved.
  pub unescaped: String,
  /// The quote mark it was written with, if any.
  pub quote: Option<char>,
  /// Byte offset where the value starts.
  pub start: usize,
  /// Exclusive byte offset where the value ends.
  pub end: usize,
}

/// One selector of a selector list: the nodes between two commas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
  pub nodes: Vec<Node>,
  /// Byte offset where the selector starts (`sourceIndex`).
  pub start: usize,
}

/// A node met by [`walk`], with where it sits.
pub struct Visit<'a> {
  pub node: &'a Node,
  /// The pseudo whose argument holds the node, if any.
  pub parent_pseudo: Option<&'a Node>,
  /// The nodes of the selector holding it.
  pub siblings: &'a [Node],
  /// Its index in `siblings`.
  pub index: usize,
}

impl Visit<'_> {
  /// The node before this one in its selector (postcss's `node.prev()`).
  pub fn prev(&self) -> Option<&Node> {
    self.index.checked_sub(1).map(|i| &self.siblings[i])
  }

  /// The node after this one in its selector (postcss's `node.next()`).
  pub fn next(&self) -> Option<&Node> {
    self.siblings.get(self.index + 1)
  }
}

/// Visit every node depth first in source order, descending into pseudo
/// arguments, the way postcss-selector-parser's `walk` does.
pub fn walk<'a>(selectors: &'a [Selector], visit: &mut dyn FnMut(&Visit<'a>)) {
  walk_in(selectors, None, visit);
}

/// [`walk`] over `selectors`, which sit inside `parent_pseudo`'s argument.
fn walk_in<'a>(
  selectors: &'a [Selector],
  parent_pseudo: Option<&'a Node>,
  visit: &mut dyn FnMut(&Visit<'a>),
) {
  for selector in selectors {
    for (index, node) in selector.nodes.iter().enumerate() {
      visit(&Visit {
        node,
        parent_pseudo,
        siblings: &selector.nodes,
        index,
      });
      if let Some(args) = &node.args {
        walk_in(&args.selectors, Some(node), visit);
      }
    }
  }
}

/// Parse `text`, which starts at byte `base` of the source, as a selector
/// list.  `None` wherever postcss-selector-parser would throw, which is
/// where Stylelint's rules give up on the selector.
pub fn parse(text: &str, base: usize) -> Option<Vec<Selector>> {
  let tokens = tokenize(text)?;
  let mut parser = Parser {
    text,
    base,
    tokens,
    position: 0,
  };
  let mut selectors = vec![Selector {
    nodes: Vec::new(),
    start: base,
  }];
  while parser.position < parser.tokens.len() {
    parser.parse(&mut selectors, true)?;
  }
  Some(selectors)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `(kind, value, start, end)` of every node of every selector, flattened
  /// in walk order.
  fn flat(text: &str) -> Vec<(Kind, String, usize, usize)> {
    let selectors = parse(text, 0).expect("parses");
    let mut out = Vec::new();
    walk(&selectors, &mut |v| {
      out.push((v.node.kind, v.node.value.clone(), v.node.start, v.node.end));
    });
    out
  }

  #[test]
  fn splits_compounds_and_combinators_with_positions() {
    assert_eq!(
      flat("A.b#c > d"),
      vec![
        (Kind::Tag, "A".into(), 0, 1),
        (Kind::Class, "b".into(), 1, 3),
        (Kind::Id, "c".into(), 3, 5),
        (Kind::Combinator, ">".into(), 6, 7),
        (Kind::Tag, "d".into(), 8, 9),
      ]
    );
    let descendant = flat("a /* c */ b");
    assert_eq!(descendant[1], (Kind::Combinator, " ".into(), 1, 10));
  }

  #[test]
  fn parses_pseudo_arguments_as_selector_lists() {
    let selectors = parse("p, img:not(a , div) {", 10);
    // An unbalanced `{` is just a word to the tokenizer.
    assert!(selectors.is_some());
    let selectors = parse("p, img:not(a , div)", 10).unwrap();
    assert_eq!(selectors.len(), 2);
    let not = &selectors[1].nodes[1];
    assert_eq!(not.kind, Kind::Pseudo);
    assert_eq!(not.value, ":not");
    assert_eq!((not.start, not.end), (16, 29));
    let args = not.args.as_ref().unwrap();
    assert_eq!((args.open, args.close), (20, 28));
    assert_eq!(args.selectors.len(), 2);
    assert_eq!(args.selectors[1].nodes[0].value, "div");
  }

  #[test]
  fn reads_attribute_parts() {
    let selectors = parse("[class ^= top i]", 0).unwrap();
    let attr = selectors[0].nodes[0].attr.as_ref().unwrap();
    assert_eq!(attr.name, "class");
    assert_eq!(attr.operator.as_deref(), Some("^="));
    let value = attr.value.as_ref().unwrap();
    assert_eq!(
      (value.raw.as_str(), value.start, value.end),
      ("top", 10, 13)
    );
    assert_eq!(attr.flag.as_deref(), Some("i"));

    let selectors = parse("[href='te\\'s']", 0).unwrap();
    let value = selectors[0].nodes[0]
      .attr
      .as_ref()
      .unwrap()
      .value
      .clone()
      .unwrap();
    assert_eq!(value.quote, Some('\''));
    assert_eq!(value.unescaped, "te's");
  }

  #[test]
  fn fails_where_postcss_selector_parser_throws() {
    for text in ["a]", "a;", ":", "a)", "[\"x\"]", "a\"b", "/* x"] {
      assert!(parse(text, 0).is_none(), "{text}");
    }
  }
}
