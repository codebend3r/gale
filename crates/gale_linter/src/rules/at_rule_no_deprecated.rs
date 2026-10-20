use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::style_rules::RawAtRule;

/// Stylelint's `deprecatedAtKeywords`.
const DEPRECATED_AT_RULES: &[&str] = &["document", "nest", "viewport"];

const NAME: &str = "at-rule-no-deprecated";

declare_rule! {
  /// Disallow deprecated at-rules.
  ///
  /// Equivalent to Stylelint's `at-rule-no-deprecated` rule: `@document`,
  /// `@nest` and `@viewport` are reported, anywhere in the stylesheet.  The
  /// fix turns `@nest <selector> { ... }` into the plain nested rule
  /// `<selector> { ... }`; the others have no replacement.
  pub struct AtRuleNoDeprecated => AtRuleNoDeprecatedCheck {
    name: NAME,
    description: "Disallow deprecated at-rules",
    default_severity: Error,
  }
}

/// `at-rule-no-deprecated` as configured.
pub struct AtRuleNoDeprecatedCheck {
  /// `ignoreAtRules`.
  ignore_at_rules: Matchers,
}

impl AtRuleNoDeprecatedCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

impl Check for AtRuleNoDeprecatedCheck {
  /// Flags deprecated at-rules, skipping `ignoreAtRules`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in &ctx.scanned_rules().at_rules {
      let lower = at.name.to_ascii_lowercase();
      if !DEPRECATED_AT_RULES.contains(&lower.as_str())
        || !is_standard_syntax_at_rule(&at)
        || self.ignore_at_rules.matches(&at.name)
      {
        continue;
      }
      let mut diag = Diagnostic::new(
        NAME,
        format!("Unexpected deprecated at-rule \"@{}\"", at.name),
      )
      .span(Span::new(at.offset, at.name.len() + 1));
      if lower == "nest" && at.has_block {
        // `@nest .foo & {}` becomes `.foo & {}`.
        diag = diag.fix(Fix::new(
          "Replace @nest with a nested rule",
          vec![Edit::new(Span::from_range(at.offset, at.params_offset), "")],
        ));
      }
      diags.push(diag);
    }
    diags
  }
}

/// Stylelint's `isStandardSyntaxAtRule`, for a scanned at-rule: not Sass's
/// blockless, paramless `@content`, nor a Less variable or detached ruleset
/// call.  (`@charset` is never deprecated, so it needs no check here.)
fn is_standard_syntax_at_rule(at: &RawAtRule) -> bool {
  if !at.has_block && at.params.is_empty() {
    return false;
  }
  let after_name = at.offset + 1 + at.name.len();
  let less_variable = at.params.starts_with(':');
  let detached_ruleset_call =
    !at.has_block && at.params_offset == after_name && at.params.starts_with('(');
  !(less_variable || detached_ruleset_call)
}

#[cfg(test)]
mod tests {
  use super::NAME;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, invalid_options, lint};

  const RULE: &str = "at-rule-no-deprecated";

  #[test]
  fn unwraps_nest_into_a_nested_rule() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(RULE, on.clone(), "a { @NEST .foo & {} }", Syntax::Css),
      "a { .foo & {} }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { @nest .foo {} @nest .bar { color: red } }",
        Syntax::Css
      ),
      "a { .foo {} .bar { color: red } }"
    );
    let warnings = lint(RULE, on, "a { @nest .foo & {} }", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (4, 5));
  }

  #[test]
  fn reports_document_and_viewport_without_a_fix() {
    let on = serde_json::json!(true);
    for css in [
      "@document url(http://www.w3.org/);",
      "@VIEWPORT { orientation: landscape; }",
    ] {
      let warnings = lint(RULE, on.clone(), css, Syntax::Css);
      assert_eq!(warnings.len(), 1, "{css}");
      assert!(warnings[0].fix.is_none(), "{css}");
    }
    for css in [
      "@charset \"utf-8\";",
      "@container (min-width: 1px) {}",
      "a { @apply --foo; }",
    ] {
      assert!(lint(RULE, on.clone(), css, Syntax::Css).is_empty(), "{css}");
    }
  }

  #[test]
  fn ignore_at_rules_matches_names_and_regexes() {
    let options = serde_json::json!([true, { "ignoreAtRules": ["document", "/^view/"] }]);
    assert!(lint(RULE, options.clone(), "@document url(x);", Syntax::Css).is_empty());
    assert!(lint(RULE, options.clone(), "@viewport { a: b }", Syntax::Css).is_empty());
    assert_eq!(
      lint(RULE, options, "a { @nest .foo & {} }", Syntax::Css).len(),
      1
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    let css = "a { @nest .foo & {} }";
    assert_eq!(lint(RULE, options.clone(), css, Syntax::Css).len(), 1);
    assert_eq!(fix(RULE, options, css, Syntax::Css), css);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        json!([true, { "ignoreAtRules": ["viewport", "/^doc/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, json!("always")),
      ["Unexpected option value \"always\" for rule \"at-rule-no-deprecated\""]
    );
    assert_eq!(
      invalid_options(NAME, json!([true, { "ignoreAtRules": [true] }])),
      ["Invalid value \"true\" for option \"ignoreAtRules\" of rule \"at-rule-no-deprecated\""]
    );
  }
}
