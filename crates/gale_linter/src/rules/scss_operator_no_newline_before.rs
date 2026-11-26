use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::sass_operators::{is_whitespace, operator_sites};

const NAME: &str = "scss/operator-no-newline-before";

declare_rule! {
  /// Disallow a line break right before a Sass operator (`10px\n  + 1`).
  ///
  /// Equivalent to stylelint-scss's `scss/operator-no-newline-before`, which
  /// runs `operator-no-unspaced`'s `calculationOperatorSpaceChecker` over the
  /// stylesheet and reports each operator with only whitespace, a line break
  /// among it, before it.  Operators are told from signs and hyphens by
  /// [`find_operators`](crate::sass_operators::find_operators).
  ///
  /// The checker also looks inside the `#{...}` interpolation of properties,
  /// selectors and comments, but that never spans a line, so no operator
  /// there can follow a line break.
  pub struct ScssOperatorNoNewlineBefore => ScssOperatorNoNewlineBeforeCheck {
    name: NAME,
    description: "Disallow newlines before Sass operators",
    default_severity: Warning,
  }
}

/// `scss/operator-no-newline-before`, which takes no options.
pub struct ScssOperatorNoNewlineBeforeCheck;

impl ScssOperatorNoNewlineBeforeCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssOperatorNoNewlineBeforeCheck {
  /// Reports each operator that has a line break in the whitespace before
  /// it (`checkNewlineBefore`).
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    operator_sites(tree)
      .into_iter()
      .filter(|site| {
        // Upstream's scan stops before index 0, so a string never counts its
        // own first character.
        let before = site.before();
        before[1.min(before.len())..]
          .iter()
          .rev()
          .take_while(|&&b| is_whitespace(b))
          .any(|&b| b == b'\n')
      })
      .map(|site| {
        let symbol = site.symbol();
        Diagnostic::new(NAME, format!("Unexpected newline before \"{symbol}\""))
          .message_args([symbol])
          .span(site.span())
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::NAME;
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  const RULE: &str = "scss/operator-no-newline-before";

  /// The `(line, column, endLine, endColumn, message)` of each warning for
  /// `source`.
  fn warnings(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize, String)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .into_iter()
      .map(|d| {
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column, d.message)
      })
      .collect()
  }

  /// `(line, column, endLine, endColumn, message)` for an operator.
  fn at(
    line: usize,
    column: usize,
    end_column: usize,
    operator: &str,
  ) -> (usize, usize, usize, usize, String) {
    let message = format!("Unexpected newline before \"{operator}\"");
    (line, column, line, end_column, message)
  }

  #[test]
  fn accepts_upstream_cases() {
    for source in [
      "a { width: 1 +1; }",
      "a { width: s1- +1; }",
      "\n      a {\n        width: 10px +\n          1;\n      }\n    ",
      "\n      a {\n        width: 10px\n          -1;\n      }\n    ",
      "\n      a {\n        width: 10px +\n          1 + 1;\n      }\n    ",
      "\n        :root {\n          --foo: '{{}}';\n        }\n      ",
      "\n      @font-face {\n        font-family: 'Ampersand';\n        src: local('Times New Roman');\n        unicode-range: U+26;\n      }\n      ",
    ] {
      assert_eq!(warnings(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_an_indented_operator_starting_a_line() {
    assert_eq!(
      warnings(
        "\n      a {\n        width: 10px\n        + 1;\n      }\n    ",
        Syntax::Scss
      ),
      vec![at(4, 9, 10, "+")]
    );
  }

  #[test]
  fn reports_an_operator_at_the_start_of_a_line() {
    assert_eq!(
      warnings(
        "\n      a {\n        width: 10px\n+ 1;\n      }\n    ",
        Syntax::Scss
      ),
      vec![at(4, 1, 2, "+")]
    );
  }

  #[test]
  fn reports_in_variables_and_at_rule_params() {
    assert_eq!(
      warnings("$a: 1\n  * 2;\n", Syntax::Scss),
      vec![at(2, 3, 4, "*")]
    );
    assert_eq!(
      warnings("@if $a\n  == 1 { }\n", Syntax::Scss),
      vec![at(2, 3, 5, "==")]
    );
    assert_eq!(
      warnings("@function f($a: 1\n  + 2) { }\n", Syntax::Scss),
      vec![at(2, 3, 4, "+")]
    );
  }

  #[test]
  fn reports_in_media_feature_values_only() {
    assert_eq!(
      warnings("@media (min-width: $a\n  + 1) { }\n", Syntax::Scss),
      vec![at(2, 3, 4, "+")]
    );
    assert_eq!(
      warnings("@media screen\n  and (color) { }\n", Syntax::Scss),
      vec![]
    );
  }

  #[test]
  fn skips_strings_data_uris_and_module_rules() {
    for source in [
      "a { content: 'a\n  + b'; }",
      "a { background: url(data:image/png;base64,iVBOR\n  +w0KGgo) $a\n  + 1; }",
      "@use 'a' with ($b: 1\n  + 2);",
    ] {
      assert_eq!(warnings(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_in_plain_css_too() {
    assert_eq!(
      warnings("a { width: 10px\n  + 1; }", Syntax::Css),
      vec![at(2, 3, 4, "+")]
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/operator-no-newline-before\""]
    );
  }
}
