use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity};

use crate::empty_lines::{empty_line_report, has_empty_line};
use crate::pattern::option_matches;
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext};

/// Require or disallow an empty line before declarations.
///
/// Equivalent to Stylelint's `declaration-empty-line-before` rule, autofix
/// included.  Primary option `"always"` or `"never"`; secondary options:
///   - `except`: `after-comment`, `after-declaration`, `first-nested`,
///     `after-block`
///   - `ignore`: `after-comment`, `after-declaration`, `first-nested`,
///     `inside-single-line-block`
///
/// Custom properties are left to `custom-property-empty-line-before`, and
/// SCSS/Less variables are not declarations to Stylelint.  Works on the
/// [`PostcssTree`] of the source, so comments and multi-line values read
/// as Stylelint reads them.
pub struct DeclarationEmptyLineBefore;

impl Rule for DeclarationEmptyLineBefore {
  fn name(&self) -> &'static str {
    "declaration-empty-line-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow an empty line before declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks every declaration in the document, in source order.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let always = ctx.primary_option_str() == Some("always");
    let secondary = ctx.secondary_options();
    let except = |name: &str| option_matches(secondary.and_then(|s| s.get("except")), name);
    let ignore = |name: &str| option_matches(secondary.and_then(|s| s.get("ignore")), name);
    let tree = ctx.postcss_tree();

    let mut diags = Vec::new();
    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if node.kind != NodeKind::Decl
        || node
          .parent
          .is_some_and(|p| !matches!(tree.nodes[p].kind, NodeKind::Rule | NodeKind::AtRule))
        || tree.is_first_node_of_root(i)
        || !tree.is_standard_syntax_declaration(i)
        || node.name.starts_with("--")
      {
        continue;
      }
      if (ignore("after-comment") && tree.is_after_comment(i))
        || (ignore("after-declaration") && is_after_standard_declaration(&tree, i))
        || (ignore("first-nested") && tree.is_first_nested(i))
        || (ignore("inside-single-line-block")
          && !tree.parent_block_string(i).contains(['\n', '\r']))
      {
        continue;
      }

      let flip = (except("first-nested") && tree.is_first_nested(i))
        || (except("after-block") && tree.is_after_block(i))
        || (except("after-comment") && tree.is_after_comment(i))
        || (except("after-declaration") && is_after_standard_declaration(&tree, i));
      let expect_empty_line = always != flip;
      if expect_empty_line == has_empty_line(tree.before(i)) {
        continue;
      }
      diags.push(empty_line_report(
        self.name(),
        self.default_severity(),
        &tree,
        i,
        expect_empty_line,
        "declaration",
      ));
    }
    diags
  }
}

/// Stylelint's `isAfterStandardPropertyDeclaration`: the node before
/// (skipping shared-line comments) is a standard declaration that is not a
/// custom property.
fn is_after_standard_declaration(tree: &PostcssTree, i: usize) -> bool {
  tree
    .previous_non_shared_line_comment(i)
    .is_some_and(|prev| {
      tree.nodes[prev].kind == NodeKind::Decl
        && tree.is_standard_syntax_declaration(prev)
        && !tree.nodes[prev].name.starts_with("--")
    })
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{context, fix};

  /// The messages for `source` in `syntax` with `options`.
  fn messages(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<String> {
    let ctx = context(source, syntax, &options);
    DeclarationEmptyLineBefore
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn reports_missing_and_unexpected_empty_lines() {
    let css = Syntax::Css;
    assert_eq!(
      messages(
        "a {\n  color: red;\n\n  display: block;\n}",
        css,
        serde_json::json!("never")
      ),
      vec!["Expected no empty line before declaration"]
    );
    assert!(
      messages(
        "a {\n  color: red;\n  display: block;\n}",
        css,
        serde_json::json!("never")
      )
      .is_empty()
    );
    assert_eq!(
      messages(
        "a {\n  color: red;\n  display: block;\n}",
        css,
        serde_json::json!("always")
      )
      .len(),
      2
    );
    let first_nested = serde_json::json!(["always", { "except": ["first-nested"] }]);
    assert!(
      messages(
        "a {\n  color: red;\n\n  display: block;\n}",
        css,
        first_nested
      )
      .is_empty()
    );
  }

  #[test]
  fn ignore_options() {
    let css = Syntax::Css;
    let single_line = serde_json::json!(["always", { "ignore": ["inside-single-line-block"] }]);
    assert!(messages("a { color: red; display: block; }", css, single_line).is_empty());
    let after_decl = serde_json::json!(["always", { "except": ["first-nested"], "ignore": ["after-declaration"] }]);
    let multi_line_value = "a {\n  inset-block-start: min(\n    calc(1px),\n    var(--x)\n  );\n  inset-inline-end: 10px;\n}";
    assert!(messages(multi_line_value, css, after_decl.clone()).is_empty());
    let comma_value = "a {\n  box-shadow: 0 0 1px red,\n    0 0 2px blue,\n    0 0 3px green;\n  animation: fade 1s;\n}";
    assert!(messages(comma_value, css, after_decl).is_empty());
  }

  #[test]
  fn line_comment_inside_a_multiline_value_is_part_of_it() {
    // carbon's `dropdown.scss`: postcss-scss keeps the comment in the value.
    let source = "a {\n  background:\n    linear-gradient(red, red)\n      padding-box,\n    // the border gradient\n    linear-gradient(\n        to bottom,\n        blue 100%\n      )\n      border-box;\n  border-color: transparent;\n}\n";
    let after_decl = serde_json::json!(["always", { "except": ["first-nested"], "ignore": ["after-declaration"] }]);
    assert!(messages(source, Syntax::Scss, after_decl.clone()).is_empty());
    // A `//` line between two declarations is still a comment node.
    let source = "a {\n  color: red;\n  // note\n  margin: 0;\n}\n";
    assert_eq!(messages(source, Syntax::Scss, after_decl).len(), 1);
  }

  #[test]
  fn skips_variables_custom_properties_and_nested_properties() {
    let always = serde_json::json!("always");
    assert!(messages("a {\n  $x: 1;\n  --y: 2;\n}", Syntax::Scss, always.clone()).is_empty());
    assert!(messages("a {\n  @x: 1;\n}", Syntax::Less, always.clone()).is_empty());
    assert!(
      messages(
        "a {\n\n  font: {\n    family: x;\n    size: 1px;\n  }\n}",
        Syntax::Scss,
        always
      )
      .is_empty()
    );
  }

  #[test]
  fn fix_adds_and_removes_empty_lines() {
    assert_eq!(
      fix(
        "declaration-empty-line-before",
        serde_json::json!(["always", { "except": ["first-nested"] }]),
        "a {\n\n  color: red;\n  top: 0; /* c */\n  left: 0;\n}",
        Syntax::Css,
      ),
      "a {\n  color: red;\n\n  top: 0; /* c */\n\n  left: 0;\n}"
    );
    assert_eq!(
      fix(
        "declaration-empty-line-before",
        serde_json::json!("never"),
        "a {\r\n  color: red;\r\n\r\n  top: 0;\r\n}",
        Syntax::Less,
      ),
      "a {\r\n  color: red;\r\n  top: 0;\r\n}"
    );
  }

  #[test]
  fn fix_keeps_prop_hacks_attached() {
    assert_eq!(
      fix(
        "declaration-empty-line-before",
        serde_json::json!("always"),
        "a {\n  color: red;\n  *zoom: 1;\n}",
        Syntax::Css,
      ),
      "a {\n\n  color: red;\n\n  *zoom: 1;\n}"
    );
  }
}
