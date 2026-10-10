use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::postcss_tree::{Node, PostcssTree};
use crate::rule::{Rule, RuleContext};
use crate::value_parser::{self, ValueNode};

/// Require `scale-color()` in place of the Sass functions that shift a
/// colour by a fixed amount (`darken()`, `lighten()`, `saturate()` and the
/// rest).
///
/// Equivalent to stylelint-scss's `scss/function-color-relative`.  Names are
/// matched exactly.  In a `filter` declaration `saturate()` and `opacity()`
/// are CSS filter functions, so there only the colour functions passed
/// straight to a `drop-shadow()` are reported.
pub struct ScssFunctionColorRelative;

/// The functions upstream reports.
const FUNCTION_NAMES: &[&str] = &[
  "saturate",
  "desaturate",
  "darken",
  "lighten",
  "opacify",
  "fade-in",
  "transparentize",
  "fade-out",
];

/// Whether `node` calls one of [`FUNCTION_NAMES`].
fn is_color_function(node: &ValueNode<'_>) -> bool {
  node.is_function() && FUNCTION_NAMES.contains(&node.value)
}

impl Rule for ScssFunctionColorRelative {
  fn name(&self) -> &'static str {
    "scss/function-color-relative"
  }

  fn description(&self) -> &'static str {
    "Require the scale-color function"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports the name of every relative colour function called in a
  /// declaration value, and in a `filter` the ones inside `drop-shadow()`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return Vec::new();
    }
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for decl in tree.decls() {
      let Some(value_index) = declaration_value_index(&tree, decl) else {
        continue;
      };
      let is_filter = decl.name == "filter";
      let parsed = value_parser::parse(&decl.value);
      value_parser::walk(&parsed, &mut |node: &ValueNode<'_>| {
        if !node.is_function() || node.value.is_empty() {
          return true;
        }
        let reported: Vec<&ValueNode<'_>> = if !is_filter {
          if is_color_function(node) {
            vec![node]
          } else {
            Vec::new()
          }
        } else if node.value == "drop-shadow" {
          node.nodes.iter().filter(|n| is_color_function(n)).collect()
        } else {
          Vec::new()
        };
        for func in reported {
          diags.push(
            Diagnostic::new(self.name(), "Expected the scale-color function to be used")
              .severity(self.default_severity())
              .span(Span::new(
                decl.start + value_index + func.source_index,
                func.value.len(),
              )),
          );
        }
        true
      });
    }
    diags
  }
}

/// stylelint-scss's `declarationValueIndex`: where the value starts, counted
/// from the start of the declaration as the position of the first `:` in
/// `decl.toString()` plus what follows it in `raws.between`.
fn declaration_value_index(tree: &PostcssTree<'_>, decl: &Node) -> Option<usize> {
  let source = tree.source();
  let prop_and_between = source.get(decl.name_span.start..decl.value_span.start)?;
  let between = source.get(decl.name_span.end..decl.value_span.start)?;
  let before_colon = prop_and_between.find(':')?;
  let after_colon = between.len() - between.find(':')?;
  Some(before_colon + after_colon)
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::lint;

  const RULE: &str = "scss/function-color-relative";

  /// The `(line, column, endLine, endColumn)` of each warning for `source`.
  fn ranges(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        assert_eq!(d.message, "Expected the scale-color function to be used");
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column)
      })
      .collect()
  }

  /// `value` as the only declaration of a rule, the way upstream's tests
  /// write it.
  fn in_rule(value: &str) -> String {
    format!("\n        p {{\n          {value}\n        }}\n      ")
  }

  #[test]
  fn accepts_scale_color_and_css_filters() {
    for value in [
      "color: scale-color(blue, $alpha: -40%);",
      "filter: saturate(50%);",
      "filter: drop-shadow(0 0 5px black) saturate(50%);",
    ] {
      assert_eq!(ranges(&in_rule(value), Syntax::Scss), vec![], "{value}");
    }
  }

  #[test]
  fn reports_relative_color_functions() {
    let cases = [
      ("color: saturate(blue, 20%);", 26),
      ("color: desaturate(blue, 20%);", 28),
      ("color: darken(blue, .2);", 24),
      ("color: lighten(blue, .2);", 25),
      ("color: opacify(blue, .2);", 25),
      ("color: fade-in(blue, .2);", 25),
      ("color: transparentize(blue, .2);", 32),
      ("color: fade-out(blue, .2);", 26),
    ];
    for (value, end) in cases {
      assert_eq!(
        ranges(&in_rule(value), Syntax::Scss),
        vec![(3, 18, 3, end)],
        "{value}"
      );
    }
  }

  #[test]
  fn reports_color_functions_inside_drop_shadow() {
    let cases = [
      ("filter: drop-shadow(0 0 5px saturate(red, 50%));", (39, 47)),
      (
        "filter: contrast(175%) drop-shadow(0 0 5px saturate(red, 50%));",
        (54, 62),
      ),
      (
        "filter: saturate(50%) drop-shadow(0 0 5px saturate(red, 50%));",
        (53, 61),
      ),
      (
        "filter: drop-shadow(0 0 5px saturate(red, 50%)) saturate(50%);",
        (39, 47),
      ),
    ];
    for (value, (column, end)) in cases {
      assert_eq!(
        ranges(&in_rule(value), Syntax::Scss),
        vec![(3, column, 3, end)],
        "{value}"
      );
    }
  }

  #[test]
  fn reports_nested_calls_and_variables() {
    assert_eq!(
      ranges("$a: mix(darken($b, 10%), lighten($c, 5%));", Syntax::Scss),
      vec![(1, 9, 1, 15), (1, 26, 1, 33)]
    );
  }

  #[test]
  fn matches_names_exactly() {
    for source in [
      "a { color: Darken(#fff, 10%); }",
      "a { color: color.darken(#fff); }",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn ignores_plain_css() {
    assert_eq!(
      ranges("a { color: darken(#fff, 10%); }", Syntax::Css),
      vec![]
    );
  }
}
