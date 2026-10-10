use std::sync::LazyLock;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::postcss_tree::NodeKind;
use crate::rule::{Rule, RuleContext};

/// Disallow `@use` rules that load a module without a namespace
/// (`@use "foo" as *`).
///
/// Equivalent to stylelint-scss's `scss/at-use-no-unnamespaced`.  Like
/// upstream it tests the params of every `@use` with
/// `/(as\s*\*)\s*(?:$|with\s*\()/` and reports the first place the matched
/// `as *` appears in the at-rule.
pub struct ScssAtUseNoUnnamespaced;

/// The characters JavaScript's `\s` matches, for a regex character class.
const JS_SPACE: &str =
  r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// Upstream's `/(as\s*\*)\s*(?:$|with\s*\()/`.
static UNNAMESPACED: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(&format!(
    r"(as[{JS_SPACE}]*\*)[{JS_SPACE}]*(?:$|with[{JS_SPACE}]*\()"
  ))
  .expect("valid regex")
});

impl Rule for ScssAtUseNoUnnamespaced {
  fn name(&self) -> &'static str {
    "scss/at-use-no-unnamespaced"
  }

  fn description(&self) -> &'static str {
    "Disallow @use without a namespace"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports the `as *` of every `@use` that loads a module into the global
  /// namespace.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return Vec::new();
    }
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for at_rule in tree
      .nodes
      .iter()
      .filter(|n| n.kind == NodeKind::AtRule && n.name == "use")
    {
      let Some(word) = UNNAMESPACED
        .captures(&at_rule.params)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
      else {
        continue;
      };
      // PostCSS's `rangeBy({ word })`: the first place the word appears in
      // the at-rule's source, or the whole at-rule when it is not there.
      let span = ctx
        .source_slice(at_rule.start, at_rule.end)
        .and_then(|text| text.find(word))
        .map_or(Span::from_range(at_rule.start, at_rule.end), |at| {
          Span::new(at_rule.start + at, word.len())
        });
      diags.push(
        Diagnostic::new(self.name(), "Unexpected @use without namespace")
          .severity(self.default_severity())
          .span(span),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::lint;

  const RULE: &str = "scss/at-use-no-unnamespaced";

  /// The `(line, column, endLine, endColumn)` of each warning for `source`.
  fn ranges(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        assert_eq!(d.message, "Unexpected @use without namespace");
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column)
      })
      .collect()
  }

  #[test]
  fn accepts_a_namespace() {
    for source in [
      "\n      @use \"foo\";\n    ",
      "\n      @use \"foo\" as bar;\n    ",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_as_star() {
    let cases = [
      ("\n      @use \"foo\" as *;\n    ", (2, 18, 2, 22)),
      ("\n      @use \"foo\"as*;\n    ", (2, 17, 2, 20)),
      ("\n      @use \"foo\"   as   *  ;\n    ", (2, 20, 2, 26)),
      (
        "\n      @use \"foo\" as * with ($baz: 1px);\n    ",
        (2, 18, 2, 22),
      ),
      (
        "\n      @use \"foo\" as * with (\n        $baz: 1px\n      );\n    ",
        (2, 18, 2, 22),
      ),
    ];
    for (source, range) in cases {
      assert_eq!(ranges(source, Syntax::Scss), vec![range], "{source}");
    }
  }

  #[test]
  fn ignores_other_at_rules_and_plain_css() {
    assert_eq!(ranges("@forward \"foo\" as *;", Syntax::Scss), vec![]);
    assert_eq!(ranges("@USE \"foo\" as *;", Syntax::Scss), vec![]);
    assert_eq!(ranges("@use \"foo\" as *;", Syntax::Css), vec![]);
  }

  #[test]
  fn falls_back_to_the_whole_at_rule_when_the_word_is_not_in_the_source() {
    // PostCSS drops the comment from the params, so `as  *` is not written
    // anywhere and Stylelint reports the whole at-rule.
    assert_eq!(
      ranges("@use \"foo\" as /* c */ *;", Syntax::Scss),
      vec![(1, 1, 1, 25)]
    );
  }
}
