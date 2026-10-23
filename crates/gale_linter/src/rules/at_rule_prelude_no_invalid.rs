use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::parens_balance;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "at-rule-prelude-no-invalid";

declare_rule! {
  /// Disallow invalid preludes (params) for at-rules.
  ///
  /// Checks for clearly invalid prelude values on common at-rules:
  /// - `@media` with unbalanced or empty parentheses
  /// - `@layer` with an empty layer name in its list
  ///
  /// Stands in for Stylelint's `at-rule-prelude-no-invalid` rule, which
  /// matches every prelude against css-tree's grammar.
  pub struct AtRulePreludeNoInvalid => AtRulePreludeNoInvalidCheck {
    name: NAME,
    description: "Disallow invalid preludes for at-rules",
    default_severity: Error,
  }
}

/// `at-rule-prelude-no-invalid` as configured.
pub struct AtRulePreludeNoInvalidCheck {
  /// `ignoreAtRules`: at-rules whose prelude is not checked.
  ignore_at_rules: Matchers,
}

impl AtRulePreludeNoInvalidCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

/// What is wrong with a prelude.
enum Problem {
  /// A prelude Stylelint finds invalid too: reported on the params, where
  /// Stylelint reports it.
  Invalid(&'static str),
  /// A prelude only gale finds wrong: reported on the whole at-rule.
  Suspect(&'static str),
}

/// The problem with the params of the at-rule `name` (lower-cased), if this
/// rule knows one.
///
/// An empty prelude is not one: Stylelint accepts `@media {}`,
/// `@keyframes {}` and `@supports {}` with a block, and skips a blockless
/// at-rule without params.
fn prelude_problem(name: &str, params: &str) -> Option<Problem> {
  match name {
    "media" if !parens_balance(params) => Some(Problem::Invalid(
      "Unexpected unbalanced parentheses in @media prelude",
    )),
    "media" if params.contains("()") => Some(Problem::Suspect(
      "Unexpected empty parentheses in @media prelude",
    )),
    // Layer names: comma-separated, each a dot-separated identifier.  An
    // empty prelude is an anonymous layer, which is fine.
    "layer"
      if !params.is_empty()
        && params
          .split(',')
          .any(|layer_name| layer_name.trim().is_empty()) =>
    {
      Some(Problem::Invalid(
        "Unexpected empty layer name in @layer prelude",
      ))
    }
    _ => None,
  }
}

impl Check for AtRulePreludeNoInvalidCheck {
  /// Validates the prelude (`atRule.params`) of the at-rules this rule knows
  /// about, skipping what Stylelint's `isStandardSyntaxAtRule` rejects.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.is_standard_syntax() || self.ignore_at_rules.matches(at.name()) {
        continue;
      }
      let params = at.params().trim();
      let name = at.name().to_ascii_lowercase();
      let (message, span) = match prelude_problem(&name, params) {
        None => continue,
        Some(Problem::Suspect(message)) => (message, at.span()),
        Some(Problem::Invalid(message)) => (message, Span::new(at.params_start(), params.len())),
      };
      diags.push(Diagnostic::new(NAME, message).span(span));
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  /// The reports for `@name params {}` in CSS.
  fn check(name: &str, params: &str) -> Vec<Diagnostic> {
    lint(
      NAME,
      json!(true),
      &format!("@{name} {params} {{}}"),
      Syntax::Css,
    )
  }

  #[test]
  fn accepts_empty_preludes() {
    // As Stylelint does: an empty prelude with a block is valid, and a
    // blockless at-rule without params is not standard syntax.
    for name in ["media", "import", "keyframes", "supports"] {
      assert!(check(name, "").is_empty(), "{name}");
    }
    assert!(lint(NAME, json!(true), "@import;\n@keyframes;", Syntax::Scss).is_empty());
  }

  #[test]
  fn allows_valid_media_prelude() {
    assert!(check("media", "(min-width: 768px)").is_empty());
    assert!(check("media", "screen and (color)").is_empty());
  }

  #[test]
  fn reports_unbalanced_parens_in_media_at_the_params() {
    let d = check("MEDIA", "(min-width: 768px))");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("unbalanced"));
    assert_eq!((d[0].span.offset, d[0].span.length), (7, 19));
    // An unclosed `(` keeps the `{` in the params, as PostCSS reads it.
    let d = check("media", "(min-width: 768px");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("unbalanced"));
  }

  #[test]
  fn reports_empty_parens_in_media() {
    let d = check("media", "screen and ()");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty parentheses"));
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 23));
  }

  #[test]
  fn allows_import_with_string_or_url() {
    for css in [
      "@import \"foo.css\";",
      "@import 'foo.css' screen;",
      "@import url(\"foo.css\");",
    ] {
      assert!(
        lint(NAME, json!(true), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn allows_named_keyframes_supports_and_layers() {
    assert!(check("keyframes", "slide-in").is_empty());
    assert!(check("supports", "(display: flex)").is_empty());
    assert!(check("layer", "").is_empty());
    assert!(check("layer", "utilities").is_empty());
  }

  #[test]
  fn reports_empty_layer_name_in_list() {
    let d = lint(NAME, json!(true), "@layer base, , utilities;", Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty layer name"));
    assert_eq!((d[0].span.offset, d[0].span.length), (7, 17));
  }

  #[test]
  fn ignores_unknown_at_rules() {
    assert!(check("custom-rule", "").is_empty());
  }

  #[test]
  fn ignore_at_rules_skips_listed_names() {
    let options = json!([true, { "ignoreAtRules": ["media"] }]);
    let css = "@media (a)) {}\n@layer a,,b;";
    let d = lint(NAME, options, css, Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@layer"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["/^x/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"at-rule-prelude-no-invalid\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["media"] }])),
      ["Invalid option name \"ignore\" for rule \"at-rule-prelude-no-invalid\""]
    );
  }
}
