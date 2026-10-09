use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::postcss_tree::{Node, PostcssTree};
use crate::rule::{Rule, RuleContext};
use crate::value_parser::{self, ValueNode};

/// Require `color.channel()` in place of the Sass functions that read one
/// channel of a colour (`color.alpha()`, `red()`, `opacity()` and the
/// rest).
///
/// Equivalent to stylelint-scss's `scss/function-color-channel`.  Names are
/// matched exactly, and nothing in a `filter` declaration is reported, since
/// `opacity()` there is the CSS filter function.
pub struct ScssFunctionColorChannel;

/// The functions upstream reports.
const FUNCTION_NAMES: &[&str] = &[
  "color.alpha",
  "color.blackness",
  "color.blue",
  "color.green",
  "color.hue",
  "color.lightness",
  "color.red",
  "color.saturation",
  "color.whiteness",
  "alpha",
  "blackness",
  "blue",
  "green",
  "hue",
  "lightness",
  "opacity",
  "red",
  "saturation",
];

impl Rule for ScssFunctionColorChannel {
  fn name(&self) -> &'static str {
    "scss/function-color-channel"
  }

  fn description(&self) -> &'static str {
    "Require the color.channel function"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports the name of every channel function called in a declaration
  /// value, outside `filter`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return Vec::new();
    }
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for decl in tree.decls() {
      if decl.name == "filter" {
        continue;
      }
      let Some(value_index) = declaration_value_index(&tree, decl) else {
        continue;
      };
      let parsed = value_parser::parse(&decl.value);
      value_parser::walk(&parsed, &mut |node: &ValueNode<'_>| {
        if node.is_function() && FUNCTION_NAMES.contains(&node.value) {
          diags.push(
            Diagnostic::new(
              self.name(),
              "Expected the color.channel function to be used",
            )
            .severity(self.default_severity())
            .span(Span::new(
              decl.start + value_index + node.source_index,
              node.value.len(),
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

  const RULE: &str = "scss/function-color-channel";

  /// The `(line, column, endLine, endColumn)` of each warning for `source`.
  fn ranges(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        assert_eq!(d.message, "Expected the color.channel function to be used");
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column)
      })
      .collect()
  }

  #[test]
  fn accepts_color_channel_scale_color_and_filters() {
    for source in [
      "\n        @use 'sass:color';\n        p {\n          opacity: color.channel(hsl(80deg 30% 50%), \"opacity\");\n        }\n      ",
      "\n        p {\n          color: scale-color(blue, $alpha: -40%);\n        }\n      ",
      "\n        p {\n          filter: opacity(50%);\n        }\n      ",
      "\n        p {\n          filter: drop-shadow(0 0 5px black) opacity(50%);\n        }\n      ",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_module_channel_functions() {
    for name in [
      "alpha",
      "blackness",
      "blue",
      "green",
      "hue",
      "lightness",
      "red",
      "saturation",
      "whiteness",
    ] {
      let source = format!(
        "\n        @use 'sass:color';\n        p {{\n          opacity: color.{name}(#e1d7d2);\n        }}\n      "
      );
      let end = 20 + "color.".len() + name.len();
      assert_eq!(
        ranges(&source, Syntax::Scss),
        vec![(4, 20, 4, end)],
        "{source}"
      );
    }
  }

  #[test]
  fn reports_global_channel_functions() {
    for name in [
      "alpha",
      "opacity",
      "blackness",
      "blue",
      "green",
      "hue",
      "lightness",
      "red",
      "saturation",
    ] {
      let source =
        format!("\n        p {{\n          opacity: {name}(#e1d7d2);\n        }}\n      ");
      let end = 20 + name.len();
      assert_eq!(
        ranges(&source, Syntax::Scss),
        vec![(3, 20, 3, end)],
        "{source}"
      );
    }
  }

  #[test]
  fn reports_nested_calls_and_variables() {
    assert_eq!(
      ranges("$a: rgba(red(#fff), 0, 0, alpha($c));", Syntax::Scss),
      vec![(1, 10, 1, 13), (1, 27, 1, 32)]
    );
  }

  #[test]
  fn matches_names_exactly() {
    for source in [
      "a { b: Alpha(#fff); }",
      "a { b: color.whiteness-x(#fff); }",
      "a { b: whiteness(#fff); }",
      "a { b: (1 + 2); }",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn ignores_plain_css() {
    assert_eq!(ranges("a { opacity: alpha(#fff); }", Syntax::Css), vec![]);
  }
}
