//! What the `order/*` rules share with stylelint-order and the
//! postcss-sorting fixers it calls: how their options are read, how
//! properties and comments are classified, how a sorted block is printed,
//! and the sort itself.
//!
//! The fixers sort a block's children with JavaScript's `Array.prototype.sort`
//! and comparators that are not always consistent (vendor-prefixed
//! duplicates against the comments that travel with them), so [`js_sort`]
//! reproduces the algorithm V8 uses rather than any stable sort.

use gale_diagnostics::{Edit, Span};

use crate::postcss_tree::{NodeKind, PostcssTree, has_delimited};

/// Stylelint's `normalizeRuleSettings` for a rule whose primary option may
/// be an array (`primaryOptionArray`): the primary and secondary options.
///
/// `[[...]]` and `[[...], { ... }]` hold the primary array (and the
/// secondary options); any other array is the primary option itself, as is
/// a value that is not an array.
pub fn split_array_settings(
  options: Option<&serde_json::Value>,
) -> (Option<&serde_json::Value>, Option<&serde_json::Value>) {
  let Some(options) = options else {
    return (None, None);
  };
  let Some(items) = options.as_array() else {
    return (Some(options), None);
  };
  match items.as_slice() {
    [] => (Some(options), None),
    [first, ..] if first.is_null() => (None, None),
    [primary] if primary.is_array() => (Some(primary), None),
    [primary, secondary] if !primary.is_object() && secondary.is_object() => {
      (Some(primary), Some(secondary))
    }
    _ => (Some(options), None),
  }
}

/// `vendor.prefix`: the `-\w+-` a property starts with, or `""`.
pub fn vendor_prefix(prop: &str) -> &str {
  let Some(rest) = prop.strip_prefix('-') else {
    return "";
  };
  let word = rest
    .bytes()
    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
    .count();
  match rest.as_bytes().get(word) {
    Some(b'-') if word > 0 => &prop[..word + 2],
    _ => "",
  }
}

/// `vendor.unprefixed`: the property without its vendor prefix.
pub fn unprefixed(prop: &str) -> &str {
  &prop[vendor_prefix(prop).len()..]
}

/// The name the order rules look properties up by: unprefixed, lowercased,
/// and with `-moz-osx-font-smoothing` read as `font-smoothing`.
pub fn normalized_property(prop: &str) -> String {
  let name = unprefixed(prop).to_lowercase();
  match name.strip_prefix("osx-") {
    Some(rest) => rest.to_string(),
    None => name,
  }
}

/// stylelint-order's `isStandardSyntaxProperty`: not a `$` or `@`
/// variable and no `#{}`, `@{}` or `$()` interpolation on one line.
pub fn is_standard_syntax_property(prop: &str) -> bool {
  !(prop.starts_with('$')
    || prop.starts_with('@')
    || has_delimited(prop, "#{", '}', false)
    || has_delimited(prop, "@{", '}', false)
    || has_delimited(prop, "$(", ')', false))
}

/// stylelint-order's `isProperty`: a declaration of a standard property
/// that is not a custom property.
pub fn is_property(tree: &PostcssTree, i: usize) -> bool {
  let node = &tree.nodes[i];
  node.kind == NodeKind::Decl
    && is_standard_syntax_property(&node.name)
    && !node.name.starts_with("--")
}

/// The blocks the order rules check, in document order: every rule,
/// at-rule and SCSS nested property with something inside
/// (`isRuleWithNodes`).
pub fn blocks<'t>(tree: &'t PostcssTree) -> impl Iterator<Item = (usize, &'t [usize])> + 't {
  tree.nodes.iter().enumerate().filter_map(|(i, node)| {
    let children = node.children.as_deref()?;
    (!children.is_empty()).then_some((i, children))
  })
}

/// postcss-sorting's `isAllowedToProcess`: Sass control and function
/// at-rules are never sorted.
pub fn is_allowed_to_process(tree: &PostcssTree, block: usize) -> bool {
  let node = &tree.nodes[block];
  !(node.kind == NodeKind::AtRule
    && matches!(
      node.name.as_str(),
      "function" | "if" | "else" | "for" | "each" | "while"
    ))
}

/// Whether the comment's `raws.before` is set (not empty) and holds a line
/// break.
fn before_has_newline(tree: &PostcssTree, i: usize) -> (bool, bool) {
  let before = tree.before(i);
  (!before.is_empty(), before.contains('\n'))
}

/// postcss-sorting's `getComments.beforeDeclaration`: the comments right
/// before `node`, each on a line of its own, earliest first.
pub fn comments_before_declaration(tree: &PostcssTree, node: usize) -> Vec<usize> {
  let mut comments = Vec::new();
  let mut previous = tree.prev(node);
  while let Some(prev) = previous {
    let (set, newline) = before_has_newline(tree, prev);
    if tree.nodes[prev].kind != NodeKind::Comment || !set || !newline {
      break;
    }
    comments.insert(0, prev);
    previous = tree.prev(prev);
  }
  comments
}

/// postcss-sorting's `getComments.afterDeclaration`: the comments right
/// after `node` on its line.
pub fn comments_after_declaration(tree: &PostcssTree, node: usize) -> Vec<usize> {
  let mut comments = Vec::new();
  let mut next = tree.next(node);
  while let Some(n) = next {
    let (set, newline) = before_has_newline(tree, n);
    if tree.nodes[n].kind != NodeKind::Comment || !set || newline {
      break;
    }
    comments.push(n);
    next = tree.next(n);
  }
  comments
}

/// postcss-sorting's `getComments.beforeNode`: like
/// [`comments_before_declaration`], but a comment on the line of the
/// opening brace counts when it is the first thing in the block.
pub fn comments_before_node(tree: &PostcssTree, node: usize) -> Vec<usize> {
  let mut comments = Vec::new();
  let mut previous = tree.prev(node);
  while let Some(prev) = previous {
    let (set, newline) = before_has_newline(tree, prev);
    if tree.nodes[prev].kind != NodeKind::Comment || !set || (!newline && tree.prev(prev).is_some())
    {
      break;
    }
    comments.insert(0, prev);
    previous = tree.prev(prev);
  }
  comments
}

/// postcss-sorting's `getComments.afterNode`, the same as
/// [`comments_after_declaration`].
pub fn comments_after_node(tree: &PostcssTree, node: usize) -> Vec<usize> {
  comments_after_declaration(tree, node)
}

/// The edit that turns the block's children into `order`, printed as
/// PostCSS prints them (see [`PostcssTree::print_children`]), narrowed to
/// the text that changes.  `None` when nothing changes.
pub fn reorder_edit(
  tree: &PostcssTree,
  children: &[usize],
  order: &[usize],
  semicolon: bool,
) -> Option<Edit> {
  let span = tree.children_span(children)?;
  let original = tree.source().get(span.clone())?;
  let printed = tree.print_children(order, semicolon);
  if printed == original {
    return None;
  }
  let mut prefix = original
    .bytes()
    .zip(printed.bytes())
    .take_while(|(a, b)| a == b)
    .count();
  while !original.is_char_boundary(prefix) || !printed.is_char_boundary(prefix) {
    prefix -= 1;
  }
  let mut suffix = original[prefix..]
    .bytes()
    .rev()
    .zip(printed[prefix..].bytes().rev())
    .take_while(|(a, b)| a == b)
    .count();
  while !original.is_char_boundary(original.len() - suffix)
    || !printed.is_char_boundary(printed.len() - suffix)
  {
    suffix -= 1;
  }
  Some(Edit::new(
    Span::from_range(span.start + prefix, span.end - suffix),
    printed[prefix..printed.len() - suffix].to_string(),
  ))
}

/// Sort `items` the way V8's `Array.prototype.sort` does with `compare` as
/// the comparator (negative: `a` first; NaN counts as 0).
///
/// Below 64 items V8's TimSort is one binary insertion sort after the
/// leading run, reproduced here step for step, so even an inconsistent
/// comparator gives V8's answer.  Longer lists, which declaration blocks
/// hardly ever are, fall back to a stable sort.
pub fn js_sort<T>(items: &mut [T], mut compare: impl FnMut(&T, &T) -> f64) {
  let len = items.len();
  if len < 2 {
    return;
  }
  let mut less = |a: &T, b: &T| {
    let order = compare(a, b);
    !order.is_nan() && order < 0.0
  };
  if len >= 64 {
    items.sort_by(|a, b| {
      if less(a, b) {
        std::cmp::Ordering::Less
      } else if less(b, a) {
        std::cmp::Ordering::Greater
      } else {
        std::cmp::Ordering::Equal
      }
    });
    return;
  }
  // CountAndMakeRun.
  let descending = less(&items[1], &items[0]);
  let mut run = 2;
  while run < len {
    let in_run = if descending {
      less(&items[run], &items[run - 1])
    } else {
      !less(&items[run], &items[run - 1])
    };
    if !in_run {
      break;
    }
    run += 1;
  }
  if descending {
    items[..run].reverse();
  }
  // BinaryInsertionSort of the rest into the run.
  for start in run..len {
    let mut left = 0;
    let mut right = start;
    while left < right {
      let mid = left + ((right - left) >> 1);
      if less(&items[start], &items[mid]) {
        right = mid;
      } else {
        left = mid + 1;
      }
    }
    items[left..=start].rotate_right(1);
  }
}

/// postcss-sorting's and stylelint-order's `isShorthand`: whether `a` is a
/// shorthand that sets `b`, directly or through another shorthand.
pub fn is_shorthand(a: &str, b: &str) -> bool {
  let Some(longhands) = shorthand_longhands(a) else {
    return false;
  };
  longhands.contains(&b) || longhands.iter().any(|longhand| is_shorthand(longhand, b))
}

/// The longhands of a shorthand, from stylelint-order's `shorthandData`.
fn shorthand_longhands(shorthand: &str) -> Option<&'static [&'static str]> {
  const TRBL_MARGIN: &[&str] = &["margin-top", "margin-bottom", "margin-left", "margin-right"];
  const TRBL_PADDING: &[&str] = &[
    "padding-top",
    "padding-bottom",
    "padding-left",
    "padding-right",
  ];
  const BORDER_SIDES: &[&str] = &["border-top", "border-bottom", "border-left", "border-right"];
  const BORDER_WIDTHS: &[&str] = &[
    "border-top-width",
    "border-bottom-width",
    "border-left-width",
    "border-right-width",
  ];
  const BORDER_STYLES: &[&str] = &[
    "border-top-style",
    "border-bottom-style",
    "border-left-style",
    "border-right-style",
  ];
  const BORDER_COLORS: &[&str] = &[
    "border-top-color",
    "border-bottom-color",
    "border-left-color",
    "border-right-color",
  ];
  const INSET_SIDES: &[&str] = &["top", "bottom", "left", "right"];
  Some(match shorthand {
    "margin" => &[
      "margin-top",
      "margin-bottom",
      "margin-left",
      "margin-right",
      "margin-block",
      "margin-inline",
    ],
    "margin-block" => &["margin-block-start", "margin-block-end"],
    "margin-inline" => &["margin-inline-start", "margin-inline-end"],
    "margin-block-start" | "margin-block-end" | "margin-inline-start" | "margin-inline-end" => {
      TRBL_MARGIN
    }
    "padding" => &[
      "padding-top",
      "padding-bottom",
      "padding-left",
      "padding-right",
      "padding-block",
      "padding-block-start",
      "padding-block-end",
      "padding-inline",
      "padding-inline-start",
      "padding-inline-end",
    ],
    "padding-block" => &[
      "padding-block-start",
      "padding-block-end",
      "padding-top",
      "padding-bottom",
      "padding-left",
      "padding-right",
    ],
    "padding-inline" => &[
      "padding-inline-start",
      "padding-inline-end",
      "padding-top",
      "padding-bottom",
      "padding-left",
      "padding-right",
    ],
    "padding-block-start" | "padding-block-end" | "padding-inline-start" | "padding-inline-end" => {
      TRBL_PADDING
    }
    "background" => &[
      "background-image",
      "background-size",
      "background-position",
      "background-repeat",
      "background-origin",
      "background-clip",
      "background-attachment",
      "background-color",
    ],
    "font" => &[
      "font-style",
      "font-variant",
      "font-weight",
      "font-stretch",
      "font-size",
      "font-family",
      "line-height",
    ],
    "border" => &[
      "border-inline",
      "border-block",
      "border-top",
      "border-bottom",
      "border-left",
      "border-right",
      "border-width",
      "border-style",
      "border-color",
    ],
    "border-inline" => &[
      "border-inline-start",
      "border-inline-end",
      "border-inline-width",
      "border-inline-style",
      "border-inline-color",
    ],
    "border-inline-width" => &["border-inline-start-width", "border-inline-end-width"],
    "border-inline-style" => &["border-inline-start-style", "border-inline-end-style"],
    "border-inline-color" => &["border-inline-start-color", "border-inline-end-color"],
    "border-inline-start" => &[
      "border-inline-start-width",
      "border-inline-start-style",
      "border-inline-start-color",
      "border-top",
      "border-bottom",
      "border-left",
      "border-right",
    ],
    "border-inline-end" => &[
      "border-inline-end-width",
      "border-inline-end-style",
      "border-inline-end-color",
      "border-top",
      "border-bottom",
      "border-left",
      "border-right",
    ],
    "border-block" => &[
      "border-block-start",
      "border-block-end",
      "border-block-width",
      "border-block-style",
      "border-block-color",
    ],
    "border-block-width" => &["border-block-start-width", "border-block-end-width"],
    "border-block-style" => &["border-block-start-style", "border-block-end-style"],
    "border-block-color" => &["border-block-start-color", "border-block-end-color"],
    "border-block-start" => &[
      "border-block-start-width",
      "border-block-start-style",
      "border-block-start-color",
      "border-top",
      "border-bottom",
      "border-left",
      "border-right",
    ],
    "border-block-end" => &[
      "border-block-end-width",
      "border-block-end-style",
      "border-block-end-color",
      "border-top",
      "border-bottom",
      "border-left",
      "border-right",
    ],
    "border-inline-start-width"
    | "border-inline-end-width"
    | "border-block-start-width"
    | "border-block-end-width"
    | "border-width" => BORDER_WIDTHS,
    "border-inline-start-style"
    | "border-inline-end-style"
    | "border-block-start-style"
    | "border-block-end-style"
    | "border-style" => BORDER_STYLES,
    "border-inline-start-color"
    | "border-inline-end-color"
    | "border-block-start-color"
    | "border-block-end-color"
    | "border-color" => BORDER_COLORS,
    "border-top" => &["border-top-width", "border-top-style", "border-top-color"],
    "border-bottom" => &[
      "border-bottom-width",
      "border-bottom-style",
      "border-bottom-color",
    ],
    "border-left" => &[
      "border-left-width",
      "border-left-style",
      "border-left-color",
    ],
    "border-right" => &[
      "border-right-width",
      "border-right-style",
      "border-right-color",
    ],
    "border-image" => &[
      "border-image-source",
      "border-image-slice",
      "border-image-width",
      "border-image-outset",
      "border-image-repeat",
    ],
    "border-radius" => &[
      "border-top-right-radius",
      "border-top-left-radius",
      "border-bottom-right-radius",
      "border-bottom-left-radius",
    ],
    "list-style" => &["list-style-type", "list-style-position", "list-style-image"],
    "transition" => &[
      "transition-delay",
      "transition-duration",
      "transition-property",
      "transition-timing-function",
    ],
    "animation" => &[
      "animation-name",
      "animation-duration",
      "animation-timing-function",
      "animation-delay",
      "animation-iteration-count",
      "animation-direction",
      "animation-fill-mode",
      "animation-play-state",
    ],
    "column-rule" => &[
      "column-rule-width",
      "column-rule-style",
      "column-rule-color",
    ],
    "columns" => &["column-width", "column-count"],
    "flex" => &["flex-grow", "flex-shrink", "flex-basis"],
    "flex-flow" => &["flex-direction", "flex-wrap"],
    "grid" => &[
      "grid-template-rows",
      "grid-template-columns",
      "grid-template-areas",
      "grid-auto-rows",
      "grid-auto-columns",
      "grid-auto-flow",
      "grid-column-gap",
      "grid-row-gap",
    ],
    "grid-area" => &[
      "grid-row-start",
      "grid-column-start",
      "grid-row-end",
      "grid-column-end",
    ],
    "grid-column" => &["grid-column-start", "grid-column-end"],
    "grid-gap" => &["grid-row-gap", "grid-column-gap"],
    "grid-row" => &["grid-row-start", "grid-row-end"],
    "grid-template" => &[
      "grid-template-columns",
      "grid-template-rows",
      "grid-template-areas",
    ],
    "offset" => &[
      "offset-anchor",
      "offset-distance",
      "offset-path",
      "offset-position",
      "offset-rotate",
    ],
    "outline" => &["outline-color", "outline-style", "outline-width"],
    "overflow" => &[
      "overflow-block",
      "overflow-inline",
      "overflow-x",
      "overflow-y",
    ],
    "overflow-block" | "overflow-inline" => &["overflow-x", "overflow-y"],
    "overscroll-behavior" => &[
      "overscroll-behavior-x",
      "overscroll-behavior-y",
      "overscroll-behavior-block",
      "overscroll-behavior-inline",
    ],
    "overscroll-behavior-block" | "overscroll-behavior-inline" => {
      &["overscroll-behavior-x", "overscroll-behavior-y"]
    }
    "text-decoration" => &[
      "text-decoration-color",
      "text-decoration-style",
      "text-decoration-line",
    ],
    "text-emphasis" => &["text-emphasis-style", "text-emphasis-color"],
    "mask" => &[
      "mask-image",
      "mask-mode",
      "mask-position",
      "mask-size",
      "mask-repeat",
      "mask-origin",
      "mask-clip",
      "mask-composite",
    ],
    "mask-border" => &[
      "mask-border-mode",
      "mask-border-outset",
      "mask-border-repeat",
      "mask-border-slice",
      "mask-border-source",
      "mask-border-width",
    ],
    "inset" => &[
      "top",
      "right",
      "bottom",
      "left",
      "inset-block",
      "inset-inline",
    ],
    "inset-block" => &["inset-block-end", "inset-block-start"],
    "inset-inline" => &["inset-inline-end", "inset-inline-start"],
    "inset-block-start" | "inset-block-end" | "inset-inline-start" | "inset-inline-end" => {
      INSET_SIDES
    }
    _ => return None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  #[test]
  fn settings_split_like_normalize_rule_settings() {
    let nested = json!([["a", "b"]]);
    assert_eq!(
      split_array_settings(Some(&nested)),
      (Some(&json!(["a", "b"])), None)
    );
    let flat = json!(["a", "b"]);
    assert_eq!(split_array_settings(Some(&flat)), (Some(&flat), None));
    let with_secondary = json!([["a"], { "unspecified": "top" }]);
    assert_eq!(
      split_array_settings(Some(&with_secondary)),
      (Some(&json!(["a"])), Some(&json!({ "unspecified": "top" })))
    );
    let objects = json!([{ "type": "rule" }, { "type": "at-rule" }]);
    assert_eq!(split_array_settings(Some(&objects)), (Some(&objects), None));
  }

  #[test]
  fn vendor_prefixes() {
    assert_eq!(vendor_prefix("-webkit-transform"), "-webkit-");
    assert_eq!(unprefixed("-moz-osx-font-smoothing"), "osx-font-smoothing");
    assert_eq!(
      normalized_property("-MOZ-OSX-Font-Smoothing"),
      "font-smoothing"
    );
    assert_eq!(vendor_prefix("--x"), "");
    assert_eq!(unprefixed("color"), "color");
  }

  #[test]
  fn js_sort_matches_v8_on_short_lists() {
    let mut numbers = vec![5, 3, 9, 1, 3, 7];
    js_sort(&mut numbers, |a, b| f64::from(*a - *b));
    assert_eq!(numbers, vec![1, 3, 3, 5, 7, 9]);
    // Equal keys keep their order.
    let mut pairs = vec![(1, 'a'), (0, 'b'), (1, 'c'), (0, 'd')];
    js_sort(&mut pairs, |a, b| f64::from(a.0 - b.0));
    assert_eq!(pairs, vec![(0, 'b'), (0, 'd'), (1, 'a'), (1, 'c')]);
    // A strictly descending run is reversed in place.
    let mut down = vec![4, 3, 2, 1];
    js_sort(&mut down, |a, b| f64::from(*a - *b));
    assert_eq!(down, vec![1, 2, 3, 4]);
  }

  #[test]
  fn shorthands_include_nested_longhands() {
    assert!(is_shorthand("border", "border-top-width"));
    assert!(is_shorthand("margin", "margin-block-start"));
    assert!(!is_shorthand("border-top", "border"));
  }

  #[test]
  fn reorder_edit_prints_like_postcss_and_narrows_to_the_change() {
    let source = "a { color: pink; top: 0 }";
    let tree = PostcssTree::parse(source, Syntax::Css);
    let children = tree.nodes[tree.root[0]].children.clone().unwrap();
    let reversed: Vec<usize> = children.iter().rev().copied().collect();
    let semicolon = tree.nodes[tree.root[0]].semicolon;
    let edit = reorder_edit(&tree, &children, &reversed, semicolon).unwrap();
    let mut fixed = source.to_string();
    fixed.replace_range(edit.span.offset..edit.span.end(), &edit.new_text);
    assert_eq!(fixed, "a { top: 0; color: pink }");
    assert!(reorder_edit(&tree, &children, &children, semicolon).is_none());
  }

  #[test]
  fn comments_travel_with_their_declarations() {
    let tree = PostcssTree::parse(
      "a {\n  /* a */\n  /* b */\n  color: red; /* c */ /* d */\n  top: 0;\n}",
      Syntax::Css,
    );
    let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
    assert_eq!(
      comments_before_declaration(&tree, kids[2]),
      vec![kids[0], kids[1]]
    );
    assert_eq!(
      comments_after_declaration(&tree, kids[2]),
      vec![kids[3], kids[4]]
    );
    let tree = PostcssTree::parse("a { /* x */ color: red; }", Syntax::Css);
    let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
    assert!(comments_before_declaration(&tree, kids[1]).is_empty());
    assert_eq!(comments_before_node(&tree, kids[1]), vec![kids[0]]);
  }
}
