use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::parens_balance;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "media-query-no-invalid";

declare_rule! {
  /// Reports obviously invalid media queries.
  ///
  /// Checks for common errors in `@media` params such as:
  /// - Unbalanced parentheses
  /// - Empty parentheses
  ///
  /// Equivalent to Stylelint's `media-query-no-invalid` rule.
  pub struct MediaQueryNoInvalid => MediaQueryNoInvalidCheck {
    name: NAME,
    description: "Disallow invalid media queries",
    default_severity: Warning,
  }
}

/// `media-query-no-invalid`, whose options change nothing yet.
pub struct MediaQueryNoInvalidCheck;

impl MediaQueryNoInvalidCheck {
  /// Stylelint's options: `true`, then `ignoreFunctions`.  Gale accepts
  /// but does not yet apply `ignoreFunctions`, since it does not check the
  /// functions in a query.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.strings("ignoreFunctions");
    options.finish()?;
    Ok(Self)
  }
}

impl Check for MediaQueryNoInvalidCheck {
  /// Flags media queries with unbalanced parentheses, at the params, or with
  /// empty parentheses, at the `()`, where Stylelint reports them.  An empty
  /// query (`@media {}`) is valid to Stylelint, so it is left alone.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.name().eq_ignore_ascii_case("media") {
        continue;
      }
      // The params as Stylelint's `getAtRuleParams` reads them.
      let params = at.raw_params();
      let start = at.params_start();
      if !parens_balance(&params) {
        diags.push(
          Diagnostic::new(NAME, "Unexpected unbalanced parentheses in media query")
            .span(Span::new(start, params.len())),
        );
      } else if let Some(empty) = params.find("()") {
        diags.push(
          Diagnostic::new(NAME, "Unexpected empty parentheses in media query")
            .span(Span::new(start + empty, 2)),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::lint;

  /// The reports for `a {}` then `@media <params> {}` in `syntax`.
  fn media(params: &str, syntax: Syntax) -> Vec<Diagnostic> {
    lint(
      NAME,
      json!(true),
      &format!("a {{}}\n@media {params} {{}}"),
      syntax,
    )
  }

  #[test]
  fn accepts_an_empty_media_query() {
    assert!(media("", Syntax::Css).is_empty());
    assert!(media("", Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_empty_parentheses_at_them() {
    for syntax in [Syntax::Css, Syntax::Scss] {
      let d = media("screen and ()", syntax);
      assert_eq!(d.len(), 1);
      assert!(d[0].message.contains("empty parentheses"));
      assert_eq!((d[0].span.offset, d[0].span.length), (23, 2));
    }
  }

  #[test]
  fn reports_unbalanced_parentheses_at_the_params() {
    let d = media("(min-width: 1px))", Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("unbalanced"));
    assert_eq!((d[0].span.offset, d[0].span.length), (12, 17));
  }

  #[test]
  fn allows_valid_media_queries() {
    for params in ["(min-width: 768px)", "screen and (color)", "(hover: hover)"] {
      assert!(media(params, Syntax::Css).is_empty(), "{params}");
    }
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFunctions": ["/^theme/", "env"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreFunctions": [{}] }])),
      ["Invalid value \"{}\" for option \"ignoreFunctions\" of rule \"media-query-no-invalid\""]
    );
  }
}
