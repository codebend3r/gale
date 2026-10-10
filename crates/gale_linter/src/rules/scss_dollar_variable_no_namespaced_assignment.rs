use std::sync::LazyLock;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Disallow assigning to a variable of another module
/// (`imported.$foo: 10px`).
///
/// Equivalent to stylelint-scss's
/// `scss/dollar-variable-no-namespaced-assignment`, which tests every
/// declaration's property with `/^[^$.]+\.\$./` and reports the whole
/// declaration.
pub struct ScssDollarVariableNoNamespacedAssignment;

/// Upstream's `/^[^$.]+\.\$./`, with JavaScript's `.`, which matches
/// anything but a line terminator.
static NAMESPACED: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[^$.]+\.\$[^\n\r\x{2028}\x{2029}]").expect("valid regex"));

impl Rule for ScssDollarVariableNoNamespacedAssignment {
  fn name(&self) -> &'static str {
    "scss/dollar-variable-no-namespaced-assignment"
  }

  fn description(&self) -> &'static str {
    "Disallow assignment to namespaced variables"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports every declaration whose property is a `namespace.$variable`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return Vec::new();
    }
    ctx
      .postcss_tree()
      .decls()
      .filter(|decl| NAMESPACED.is_match(&decl.name))
      .map(|decl| {
        Diagnostic::new(
          self.name(),
          "Unexpected assignment to a namespaced $ variable",
        )
        .severity(self.default_severity())
        .span(Span::from_range(decl.start, decl.end))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::lint;

  const RULE: &str = "scss/dollar-variable-no-namespaced-assignment";

  /// The `(line, column, endLine, endColumn)` of each warning for `source`.
  fn ranges(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        assert_eq!(
          d.message,
          "Unexpected assignment to a namespaced $ variable"
        );
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column)
      })
      .collect()
  }

  #[test]
  fn accepts_plain_variables_and_namespaced_reads() {
    for source in [
      "\n      p {\n        $foo: 10px;\n      }\n    ",
      "\n      p {\n        a: imported.$foo;\n      }\n    ",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_the_whole_declaration() {
    assert_eq!(
      ranges(
        "\n      p {\n        imported.$foo: 10px;\n      }\n    ",
        Syntax::Scss
      ),
      vec![(3, 9, 3, 29)]
    );
  }

  #[test]
  fn reports_at_the_top_level() {
    assert_eq!(
      ranges("imported.$foo: 1;\na { b: imported.$foo; }\n", Syntax::Scss),
      vec![(1, 1, 1, 18)]
    );
  }

  #[test]
  fn needs_a_namespace_and_a_variable_name() {
    for source in ["a { .$foo: 1; }", "a { a.$: 1; }", "a { $a.$b: 1; }"] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn ignores_plain_css() {
    assert_eq!(ranges("a { imported.$foo: 1; }", Syntax::Css), vec![]);
  }
}
