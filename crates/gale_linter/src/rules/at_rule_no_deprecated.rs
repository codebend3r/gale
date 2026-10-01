use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::style_rules::{RawAtRule, scan_at_rules};

/// Stylelint's `deprecatedAtKeywords`.
const DEPRECATED_AT_RULES: &[&str] = &["document", "nest", "viewport"];

/// Disallow deprecated at-rules.
///
/// Equivalent to Stylelint's `at-rule-no-deprecated` rule: `@document`,
/// `@nest` and `@viewport` are reported, anywhere in the stylesheet.  The
/// fix turns `@nest <selector> { ... }` into the plain nested rule
/// `<selector> { ... }`; the others have no replacement.
pub struct AtRuleNoDeprecated;

impl Rule for AtRuleNoDeprecated {
  fn name(&self) -> &'static str {
    "at-rule-no-deprecated"
  }

  fn description(&self) -> &'static str {
    "Disallow deprecated at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  /// Flags deprecated at-rules, skipping `ignoreAtRules`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore = ctx.secondary_options().and_then(|v| v.get("ignoreAtRules"));
    let mut diags = Vec::new();
    for at in scan_at_rules(ctx.source, ctx.syntax) {
      let lower = at.name.to_ascii_lowercase();
      if !DEPRECATED_AT_RULES.contains(&lower.as_str())
        || !is_standard_syntax_at_rule(&at)
        || pattern::option_matches(ignore, &at.name)
      {
        continue;
      }
      let mut diag = Diagnostic::new(
        self.name(),
        format!("Unexpected deprecated at-rule \"@{}\"", at.name),
      )
      .severity(self.default_severity())
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
  use std::collections::HashMap;

  use gale_css_parser::Syntax;
  use gale_diagnostics::apply_fixes;

  use crate::{LintRunner, RuleRegistry};

  /// Lint `css` with only this rule enabled, configured with `options`.
  fn lint(css: &str, options: serde_json::Value) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "at-rule-no-deprecated".to_string();
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec![rule.clone()],
      HashMap::from([(rule, options)]),
    );
    runner.lint_source(css, "test.css", Syntax::Css).diagnostics
  }

  /// `css` after applying the rule's fixes until nothing changes, the way
  /// `gale --fix` does.
  fn fix(css: &str, options: serde_json::Value) -> String {
    let mut current = css.to_string();
    for _ in 0..10 {
      let (next, applied) = apply_fixes(&current, &lint(&current, options.clone()));
      if applied == 0 || next == current {
        break;
      }
      current = next;
    }
    current
  }

  #[test]
  fn unwraps_nest_into_a_nested_rule() {
    let on = serde_json::json!(true);
    assert_eq!(fix("a { @NEST .foo & {} }", on.clone()), "a { .foo & {} }");
    assert_eq!(
      fix("a { @nest .foo {} @nest .bar { color: red } }", on.clone()),
      "a { .foo {} .bar { color: red } }"
    );
    let warnings = lint("a { @nest .foo & {} }", on);
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
      let warnings = lint(css, on.clone());
      assert_eq!(warnings.len(), 1, "{css}");
      assert!(warnings[0].fix.is_none(), "{css}");
    }
    for css in [
      "@charset \"utf-8\";",
      "@container (min-width: 1px) {}",
      "a { @apply --foo; }",
    ] {
      assert!(lint(css, on.clone()).is_empty(), "{css}");
    }
  }

  #[test]
  fn ignore_at_rules_matches_names_and_regexes() {
    let options = serde_json::json!([true, { "ignoreAtRules": ["document", "/^view/"] }]);
    assert!(lint("@document url(x);", options.clone()).is_empty());
    assert!(lint("@viewport { a: b }", options.clone()).is_empty());
    assert_eq!(lint("a { @nest .foo & {} }", options).len(), 1);
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    let css = "a { @nest .foo & {} }";
    assert_eq!(lint(css, options.clone()).len(), 1);
    assert_eq!(fix(css, options), css);
  }
}
