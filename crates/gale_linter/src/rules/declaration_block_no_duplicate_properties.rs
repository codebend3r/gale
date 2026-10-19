use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::data::is_named_color;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::{is_standard_syntax_property, is_standard_syntax_value};
use crate::value_syntax::{self, Node};

const NAME: &str = "declaration-block-no-duplicate-properties";

declare_rule! {
  /// Disallow duplicate properties within declaration blocks.
  ///
  /// Equivalent to Stylelint's `declaration-block-no-duplicate-properties`
  /// rule, autofix included: the fix removes the declaration that loses (the
  /// earlier one, or the later one when only the earlier is `!important`).
  /// Secondary options:
  ///   - `ignore`: `consecutive-duplicates`,
  ///     `consecutive-duplicates-with-different-values`,
  ///     `consecutive-duplicates-with-different-syntaxes`,
  ///     `consecutive-duplicates-with-same-prefixless-values`
  ///   - `ignoreProperties`: names or `/regex/` entries
  ///
  /// Each block (style rule, at-rule or the stylesheet itself) is checked on
  /// its own, as Stylelint's `eachDeclarationBlock` does, over the
  /// [`PostcssTree`] of the source so values compare as PostCSS reads them.
  /// Custom properties, `src`, and SCSS/Less variables and interpolated
  /// properties are never duplicates.
  pub struct DeclarationBlockNoDuplicateProperties => DeclarationBlockNoDuplicatePropertiesCheck {
    name: NAME,
    description: "Disallow duplicate properties within declaration blocks",
    default_severity: Warning,
  }
}

/// `declaration-block-no-duplicate-properties` as configured.
pub struct DeclarationBlockNoDuplicatePropertiesCheck {
  /// The `ignore` flags.
  ignore: Ignore,
  /// `ignoreProperties`.
  ignore_properties: Matchers,
}

impl DeclarationBlockNoDuplicatePropertiesCheck {
  /// Stylelint's options: `true`, then `ignore` and `ignoreProperties`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore = options.keywords(
      "ignore",
      &[
        "consecutive-duplicates",
        "consecutive-duplicates-with-different-values",
        "consecutive-duplicates-with-different-syntaxes",
        "consecutive-duplicates-with-same-prefixless-values",
      ],
    );
    let ignore_properties = options.matchers("ignoreProperties");
    options.finish()?;
    Ok(Self {
      ignore: Ignore {
        consecutive: ignore.contains(&"consecutive-duplicates"),
        different_values: ignore.contains(&"consecutive-duplicates-with-different-values"),
        different_syntaxes: ignore.contains(&"consecutive-duplicates-with-different-syntaxes"),
        same_prefixless_values: ignore
          .contains(&"consecutive-duplicates-with-same-prefixless-values"),
      },
      ignore_properties,
    })
  }
}

/// The `ignore` flags that change how consecutive duplicates are treated.
#[derive(Debug, Clone, Copy, Default)]
struct Ignore {
  /// `consecutive-duplicates`.
  consecutive: bool,
  /// `consecutive-duplicates-with-different-values`.
  different_values: bool,
  /// `consecutive-duplicates-with-different-syntaxes`.
  different_syntaxes: bool,
  /// `consecutive-duplicates-with-same-prefixless-values`.
  same_prefixless_values: bool,
}

impl Check for DeclarationBlockNoDuplicatePropertiesCheck {
  /// Checks the declarations of every block in the document.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let tree = ctx.postcss_tree();
    let mut checker = Checker {
      tree: &tree,
      ignore: self.ignore,
      ignore_properties: &self.ignore_properties,
      diags: Vec::new(),
    };
    checker.check_block(&tree.root);
    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if matches!(node.kind, NodeKind::Rule | NodeKind::AtRule)
        && let Some(children) = &node.children
        && is_walked(&tree, i)
      {
        checker.check_block(children);
      }
    }
    checker.diags
  }
}

/// Whether `eachDeclarationBlock` reaches the block of node `i`: it only
/// descends through the stylesheet, rules and at-rules, so a block inside an
/// SCSS nested property (`font: { ... }`) is never visited.
fn is_walked(tree: &PostcssTree, i: usize) -> bool {
  let mut parent = tree.nodes[i].parent;
  while let Some(p) = parent {
    if tree.nodes[p].kind == NodeKind::Decl {
      return false;
    }
    parent = tree.nodes[p].parent;
  }
  true
}

/// The state of one run of the rule over a tree.
struct Checker<'a, 't> {
  tree: &'a PostcssTree<'t>,
  ignore: Ignore,
  ignore_properties: &'a Matchers,
  diags: Vec<Diagnostic>,
}

impl Checker<'_, '_> {
  /// Stylelint's loop over the declarations among `children`.
  fn check_block(&mut self, children: &[usize]) {
    let tree = self.tree;
    // The "active" declaration for each property seen so far.
    let mut active: Vec<usize> = Vec::new();
    let mut previous_prop = String::new();
    for &decl in children {
      let node = &tree.nodes[decl];
      if node.kind != NodeKind::Decl {
        continue;
      }
      let prop = node.name.as_str();
      let lower_prop = prop.to_ascii_lowercase();
      if !is_standard_syntax_property(prop)
        || prop.starts_with("--")
        || self.ignore_properties.matches(prop)
        || lower_prop == "src"
      {
        continue;
      }

      let index = active
        .iter()
        .position(|&d| tree.nodes[d].name.eq_ignore_ascii_case(&lower_prop));
      let consecutive = previous_prop == lower_prop;
      previous_prop = lower_prop;
      let Some(index) = index else {
        active.push(decl);
        continue;
      };
      let duplicate = active[index];

      let value = node.value.as_str();
      let duplicate_value = tree.nodes[duplicate].value.as_str();
      let duplicate_is_more_important = !node.important && tree.nodes[duplicate].important;
      let Ignore {
        consecutive: ignore_consecutive,
        different_values,
        different_syntaxes,
        same_prefixless_values,
      } = self.ignore;

      if different_values || different_syntaxes || same_prefixless_values {
        if !consecutive
          || (same_prefixless_values && unprefixed(value) != unprefixed(duplicate_value))
        {
          self.fix_or_report(&mut active, index, decl, duplicate_is_more_important);
          continue;
        }
        if different_syntaxes && is_equal_value_syntaxes(value, duplicate_value, prop) {
          self.fix_or_report(&mut active, index, decl, duplicate_is_more_important);
          continue;
        }
        if value != duplicate_value {
          continue;
        }
        // The current declaration becomes the active one either way.
        active[index] = decl;
        self.report(duplicate);
        continue;
      }

      if ignore_consecutive && consecutive {
        continue;
      }
      self.fix_or_report(&mut active, index, decl, duplicate_is_more_important);
    }
  }

  /// Stylelint's `fixOrReport`: report (and remove) the declaration that
  /// loses, keeping the current one active unless the earlier one is more
  /// important.
  fn fix_or_report(
    &mut self,
    active: &mut [usize],
    index: usize,
    decl: usize,
    duplicate_is_more_important: bool,
  ) {
    if duplicate_is_more_important {
      self.report(decl);
    } else {
      let duplicate = active[index];
      active[index] = decl;
      self.report(duplicate);
    }
  }

  /// Report the declaration `node` at its property, with the fix that
  /// removes it.
  fn report(&mut self, node: usize) {
    let tree = self.tree;
    let n = &tree.nodes[node];
    let edits = tree
      .removal_ranges(node)
      .into_iter()
      .map(|range| Edit::new(Span::from_range(range.start, range.end), String::new()))
      .collect();
    self.diags.push(
      Diagnostic::new(NAME, format!("Unexpected duplicate \"{}\"", n.name))
        .span(Span::new(n.start, n.name.len().min(n.end - n.start)))
        .fix(Fix::new("Remove the duplicate declaration", edits)),
    );
  }
}

/// PostCSS's `vendor.unprefixed`: `value` without a leading `-\w+-`.
fn unprefixed(value: &str) -> &str {
  let Some(rest) = value.strip_prefix('-') else {
    return value;
  };
  let word = rest
    .bytes()
    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
    .count();
  // `\w+` backtracks to the last `-` it can end on, but a word never
  // contains `-`, so the prefix ends at the first one.
  match rest.as_bytes().get(word) {
    Some(b'-') if word > 0 => &rest[word + 1..],
    _ => value,
  }
}

/// Stylelint's `isEqualValueSyntaxes`: the same text, or both standard and
/// parsed by css-tree into the same shape (see [`is_equal_value_nodes`]).
fn is_equal_value_syntaxes(value1: &str, value2: &str, property: &str) -> bool {
  if value1 == value2 {
    return true;
  }
  if !(is_standard_syntax_value(value1) && is_standard_syntax_value(value2)) {
    return false;
  }
  let (Some(nodes1), Some(nodes2)) = (value_syntax::parse(value1), value_syntax::parse(value2))
  else {
    return false;
  };
  is_equal_value_nodes(&nodes1, &nodes2, property)
}

/// Stylelint's `isEqualValueNodes`: the same node types in the same order,
/// with the same identifier and function names (ignoring case), the same
/// units, and the same shape inside functions and brackets.  Two custom
/// property names count as equal, as do two named colors in a color
/// property.
fn is_equal_value_nodes(nodes1: &[Node], nodes2: &[Node], property: &str) -> bool {
  if nodes1.len() != nodes2.len() {
    return false;
  }
  for (node1, node2) in nodes1.iter().zip(nodes2) {
    if std::mem::discriminant(node1) != std::mem::discriminant(node2) {
      return false;
    }
    if let (Node::Identifier(name1), Node::Identifier(name2)) = (node1, node2) {
      if name1.starts_with("--") && name2.starts_with("--") {
        continue;
      }
      if is_color_property(&property.to_ascii_lowercase())
        && is_named_color(&name1.to_ascii_lowercase())
        && is_named_color(&name2.to_ascii_lowercase())
      {
        continue;
      }
    }
    if !node1.name().eq_ignore_ascii_case(node2.name()) || node1.unit() != node2.unit() {
      return false;
    }
    if let (Some(children1), Some(children2)) = (node1.children(), node2.children())
      && !is_equal_value_nodes(children1, children2, property)
    {
      return false;
    }
  }
  true
}

/// Stylelint's `colorProperties`.
fn is_color_property(prop: &str) -> bool {
  matches!(
    prop,
    "accent-color"
      | "background-color"
      | "border-block-color"
      | "border-block-end-color"
      | "border-block-start-color"
      | "border-bottom-color"
      | "border-inline-color"
      | "border-inline-end-color"
      | "border-inline-start-color"
      | "border-left-color"
      | "border-right-color"
      | "border-top-color"
      | "caret-color"
      | "color"
      | "column-rule-color"
      | "outline-color"
      | "text-decoration-color"
      | "text-emphasis-color"
      | "flood-color"
      | "lighting-color"
      | "stop-color"
      | "border-color"
      | "scrollbar-color"
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{configured, context, fix, invalid_options};

  const RULE: &str = "declaration-block-no-duplicate-properties";

  /// The offsets reported for `source` in `syntax` with `options`.
  fn offsets(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<usize> {
    let ctx = context(source, syntax);
    configured(&DeclarationBlockNoDuplicateProperties, options)
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.span.offset)
      .collect()
  }

  #[test]
  fn reports_the_earlier_duplicate_in_each_block() {
    let on = serde_json::json!(true);
    assert_eq!(
      offsets("a { color: pink; color: orange }", Syntax::Css, on.clone()),
      vec![4]
    );
    assert_eq!(
      offsets(
        "a { COlOr: pink; coLOR: pink; color: pink }",
        Syntax::Css,
        on.clone()
      ),
      vec![4, 17]
    );
    assert!(
      offsets(
        "a { color: pink; @media { color: orange; } }",
        Syntax::Css,
        on.clone()
      )
      .is_empty()
    );
    assert!(offsets("@font-face { src: a; SRC: b }", Syntax::Css, on.clone()).is_empty());
    assert!(
      offsets(
        "a { $s: 0; $s: 1; --c: 0; --c: 1; }",
        Syntax::Scss,
        on.clone()
      )
      .is_empty()
    );
    assert!(offsets("a { @l: 0; @l: 1; }", Syntax::Less, on).is_empty());
  }

  #[test]
  fn an_important_duplicate_wins() {
    let on = serde_json::json!(true);
    assert_eq!(
      offsets(
        "a { color: red !important; color: blue; }",
        Syntax::Css,
        on.clone()
      ),
      vec![27]
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { color: red !important; color: blue; }",
        Syntax::Css
      ),
      "a { color: red !important; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { color: red !important; color: blue }",
        Syntax::Css
      ),
      "a { color: red !important }"
    );
    assert_eq!(
      fix(
        RULE,
        on,
        "a { color: red ! IMPORTANT; color: blue; }",
        Syntax::Css
      ),
      "a { color: red ! IMPORTANT; }"
    );
  }

  #[test]
  fn fix_removes_the_losing_declarations() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { color: pink; color: pink; color: orange }",
        Syntax::Css
      ),
      "a { color: orange }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a {\n  color: pink;\n  /* c */\n  color: orange;\n}",
        Syntax::Css
      ),
      "a {\n  /* c */\n  color: orange;\n}"
    );
    assert_eq!(
      fix(
        RULE,
        on,
        "a { color: pink; @media { color: orange; color: black; } }",
        Syntax::Css
      ),
      "a { color: pink; @media { color: black; } }"
    );
  }

  #[test]
  fn consecutive_ignore_options() {
    let consecutive = serde_json::json!([true, { "ignore": ["consecutive-duplicates"] }]);
    assert!(
      offsets(
        "p { font-size: 16px; font-size: 1rem; }",
        Syntax::Css,
        consecutive.clone()
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        consecutive,
        "p { font-size: 16px !important; font-weight: 400; font-size: 1rem; }",
        Syntax::Css
      ),
      "p { font-size: 16px !important; font-weight: 400; }"
    );
    let values =
      serde_json::json!([true, { "ignore": ["consecutive-duplicates-with-different-values"] }]);
    assert!(
      offsets(
        "p { font-size: 16px; font-size: 18px; }",
        Syntax::Css,
        values.clone()
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        values,
        "p { font-size: 16px; font-size: 16px; font-weight: 400; }",
        Syntax::Css
      ),
      "p { font-size: 16px; font-weight: 400; }"
    );
    let prefixless = serde_json::json!([true, { "ignore": ["consecutive-duplicates-with-same-prefixless-values"] }]);
    assert!(
      offsets(
        "p { width: fit-content; width: -moz-fit-content; }",
        Syntax::Css,
        prefixless.clone()
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        prefixless,
        "p { width: 100%; width: -moz-fit-content; height: 32px; }",
        Syntax::Css
      ),
      "p { width: -moz-fit-content; height: 32px; }"
    );
  }

  #[test]
  fn different_syntaxes_compare_value_shapes() {
    assert!(is_equal_value_syntaxes("100vw", "50vw", "width"));
    assert!(!is_equal_value_syntaxes("100vw", "100dvw", "width"));
    assert!(!is_equal_value_syntaxes("100%", "fit-content", "width"));
    assert!(!is_equal_value_syntaxes(
      "min(10px, 11px)",
      "max(10px, 11px)",
      "width"
    ));
    assert!(is_equal_value_syntaxes(
      "CaLC(10px + 4rem)",
      "calc(10px + 2rem)",
      "width"
    ));
    assert!(is_equal_value_syntaxes(
      "calc(100vw   + 10vw)",
      "calc(100vw + 10vw)",
      "width"
    ));
    assert!(!is_equal_value_syntaxes(
      "calc((10px + 2px))",
      "calc((10rem + 2rem))",
      "width"
    ));
    assert!(is_equal_value_syntaxes("var(--foo)", "var(--bar)", "width"));
    assert!(!is_equal_value_syntaxes("env(foo)", "env(--bar)", "width"));
    assert!(is_equal_value_syntaxes("red", "blue", "color"));
    assert!(!is_equal_value_syntaxes("red", "blue", "animation-name"));
    assert!(!is_equal_value_syntaxes("red", "transparent", "color"));
    assert!(!is_equal_value_syntaxes("_$a", "_$a2", "width"));
    assert!(!is_equal_value_syntaxes("$a", "calc(1 + $a)", "width"));
    let syntaxes =
      serde_json::json!([true, { "ignore": ["consecutive-duplicates-with-different-syntaxes"] }]);
    assert_eq!(
      fix(
        RULE,
        syntaxes.clone(),
        "p { width: calc(100vw /* a comment */  + 10vw); width: calc(100vw + 10vw); }",
        Syntax::Css
      ),
      "p { width: calc(100vw + 10vw); }"
    );
    assert!(
      offsets(
        "p { width: 100vw; width: 100dvw; height: 100vh; }",
        Syntax::Css,
        syntaxes
      )
      .is_empty()
    );
  }

  #[test]
  fn ignore_properties_takes_names_and_regexes() {
    let options = serde_json::json!([true, { "ignoreProperties": ["color", "/^back/"] }]);
    assert_eq!(
      offsets(
        "p { color: a; color: b; background: c; background: d; margin: 0; margin: 1; }",
        Syntax::Css,
        options
      ),
      vec![54]
    );
  }

  #[test]
  fn unprefixed_strips_one_vendor_prefix() {
    assert_eq!(unprefixed("-moz-fit-content"), "fit-content");
    assert_eq!(unprefixed("-webkit-box"), "box");
    assert_eq!(unprefixed("fit-content"), "fit-content");
    assert_eq!(unprefixed("--x"), "--x");
    assert_eq!(unprefixed("-1px"), "-1px");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        RULE,
        serde_json::json!([true, {
          "ignore": [
            "consecutive-duplicates",
            "consecutive-duplicates-with-different-values",
            "consecutive-duplicates-with-different-syntaxes",
            "consecutive-duplicates-with-same-prefixless-values"
          ],
          "ignoreProperties": ["color", "/^back/"]
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"declaration-block-no-duplicate-properties\""]
    );
    assert_eq!(
      invalid_options(
        RULE,
        serde_json::json!([true, { "ignore": ["duplicates"] }])
      ),
      [
        "Invalid value \"duplicates\" for option \"ignore\" of rule \"declaration-block-no-duplicate-properties\""
      ]
    );
  }
}
