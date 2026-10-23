use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-nested-pattern";

declare_rule! {
  /// Specify a pattern for the selectors of rules nested within rules.
  ///
  /// Equivalent to Stylelint's `selector-nested-pattern` rule.
  /// The primary option is a regex pattern string that nested selectors must match.
  pub struct SelectorNestedPattern => SelectorNestedPatternCheck {
    name: NAME,
    description: "Specify a pattern for the selectors of rules nested within rules",
    default_severity: Warning,
  }
}

/// `selector-nested-pattern` as configured.
pub struct SelectorNestedPatternCheck {
  /// The pattern nested selectors must match; with none configured the
  /// rule reports nothing.
  pattern: Option<Pattern>,
}

impl SelectorNestedPatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`,
  /// then `splitList`, which gale accepts but does not yet apply (it always
  /// tests each selector of a list).  Lookarounds such as `^(?!.*&[-_])`
  /// work, and a pattern that does not compile is an invalid option.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.flag("splitList");
    options.finish()?;
    Ok(Self { pattern })
  }
}

impl Check for SelectorNestedPatternCheck {
  /// Flags nested selectors that do not match the configured pattern. Uses
  /// `fancy_regex` so lookarounds in the pattern work.
  ///
  /// As in Stylelint, a rule is nested when its parent is a rule (not an
  /// at-rule), and each one is reported once, at the rule.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(pattern) = &self.pattern else {
      return vec![];
    };
    let pattern_str = &pattern.source;

    let mut diags = Vec::new();
    for rule in tree
      .walk_rules()
      .filter(|rule| rule.parent_is_rule() && rule.is_standard_syntax())
    {
      let selector = rule.raw_selector();
      // Check each selector in a selector list (split by comma), and report
      // the first that fails, once per list.
      if let Some(sel) = selector
        .trim()
        .split(',')
        .map(str::trim)
        .find(|sel| !sel.is_empty() && !pattern.is_match(sel))
      {
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Expected nested selector \"{sel}\" to match pattern \"{pattern_str}\""),
          )
          .span(rule.span()),
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

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// What the rule configured with `options` reports for `source`.
  fn check(options: serde_json::Value, source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&SelectorNestedPattern, options).check_source(&context(source, syntax))
  }

  #[test]
  fn reports_nested_without_ampersand() {
    let source = ".parent { color: red; .child { color: red; } }";
    let d = check(serde_json::json!("^&"), source, Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".child"));
    assert_eq!(d[0].span.offset, source.find(".child").unwrap());
  }

  #[test]
  fn allows_nested_with_ampersand() {
    let source = ".parent { color: red; &:hover { color: red; } }";
    assert!(check(serde_json::json!("^&"), source, Syntax::Css).is_empty());
  }

  #[test]
  fn no_diagnostics_for_no_children() {
    let source = ".parent { color: red; }";
    assert!(check(serde_json::json!("^&"), source, Syntax::Css).is_empty());
  }

  #[test]
  fn no_diagnostics_without_options() {
    let source = ".parent { color: red; .child { color: red; } }";
    let d = configured(&SelectorNestedPattern, None).check_source(&context(source, Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn negative_lookahead_pattern() {
    // Pattern like patternfly uses: "^(?!.*&[-_])"
    // This pattern rejects selectors containing &- or &_
    let opts = serde_json::json!("^(?!.*&[-_])");
    // &__child should be rejected (contains &_)
    let source = ".parent { color: red; &__child { color: red; } }";
    assert_eq!(check(opts.clone(), source, Syntax::Scss).len(), 1);

    // &:hover should be accepted (no &- or &_)
    let source = ".parent { color: red; &:hover { color: red; } }";
    assert!(check(opts.clone(), source, Syntax::Scss).is_empty());

    // .child without & should also be accepted (no &-)
    let source = ".parent { color: red; .child { color: red; } }";
    assert!(check(opts, source, Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_each_nested_rule_once() {
    // Each rule nested in a rule, however deep, is reported once; one in
    // an at-rule is not nested in a rule, as Stylelint sees it.
    let source = ".a { .b { .c { .d {} } } @media print { .e {} } }";
    let d = check(serde_json::json!("^&"), source, Syntax::Scss);
    let found: Vec<_> = d
      .iter()
      .map(|d| &source[d.span.offset..d.span.offset + 2])
      .collect();
    assert_eq!(found, [".b", ".c", ".d"]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(["^&", { "splitList": true }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"selector-nested-pattern\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["^&", { "splitList": "yes" }])),
      ["Invalid value \"yes\" for option \"splitList\" of rule \"selector-nested-pattern\""]
    );
  }
}
