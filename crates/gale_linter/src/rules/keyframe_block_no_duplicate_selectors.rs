use std::collections::HashSet;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{split_selectors, stripped_selector};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "keyframe-block-no-duplicate-selectors";

declare_rule! {
  /// Disallow duplicate selectors within keyframe blocks.
  ///
  /// Equivalent to Stylelint's `keyframe-block-no-duplicate-selectors` rule.
  ///
  /// Checks `@keyframes` at-rules for child `Style` nodes that share the same
  /// selector (e.g. two `from` or two `50%` blocks).
  pub struct KeyframeBlockNoDuplicateSelectors => KeyframeBlockNoDuplicateSelectorsCheck {
    name: NAME,
    description: "Disallow duplicate selectors within keyframe blocks",
    default_severity: Warning,
  }
}

/// `keyframe-block-no-duplicate-selectors`, which takes no options.
pub struct KeyframeBlockNoDuplicateSelectorsCheck;

impl KeyframeBlockNoDuplicateSelectorsCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for KeyframeBlockNoDuplicateSelectorsCheck {
  /// Flags a keyframe selector that an earlier one in the same `@keyframes`
  /// (vendor-prefixed or not, in any case) already names, comparing the
  /// selectors of each list one by one, as written and ignoring case.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for keyframes in tree
      .walk_at_rules()
      .filter(|at_rule| at_rule.is_keyframes())
    {
      let mut seen = HashSet::new();
      for rule in keyframes
        .walk_rules()
        .filter(|rule| rule.is_standard_syntax())
      {
        let prelude = rule.raw_selector();
        let start = rule.selector_start();
        for (offset, part) in split_selectors(&prelude) {
          let (index, selector) = stripped_selector(part);
          if !seen.insert(selector.to_lowercase()) {
            let index = start + offset + index;
            diagnostics.push(
              Diagnostic::new(
                NAME,
                format!("Unexpected duplicate keyframe selector \"{selector}\""),
              )
              .span(Span::from_range(index, index + selector.len())),
            );
          }
        }
      }
    }
    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source`.
  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&KeyframeBlockNoDuplicateSelectors, None).check_source(&context(source, syntax))
  }

  #[test]
  fn reports_duplicate_keyframe_selectors() {
    let source =
      "@keyframes fade { from { opacity: 0; } to { opacity: 1; } from { opacity: 0.5; } }";
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      let diags = check(source, syntax);
      assert_eq!(diags.len(), 1, "{syntax:?}");
      assert_eq!(
        diags[0].message,
        "Unexpected duplicate keyframe selector \"from\""
      );
      assert_eq!(diags[0].span, Span::new(58, 4));
    }
  }

  #[test]
  fn ignores_unique_keyframe_selectors() {
    assert!(check("@keyframes fade { from {} to {} }", Syntax::Css).is_empty());
  }

  #[test]
  fn ignores_non_keyframes_at_rules() {
    assert!(check("@media print { a {} a {} }", Syntax::Css).is_empty());
  }

  #[test]
  fn compares_the_selectors_of_a_list_one_by_one() {
    // As Stylelint does: `50%` repeats the second selector of the first
    // list, and the report quotes each repeat as written, where it stands.
    let source = "@-webkit-keyframes k { 0%, /* c */ 50% {} FROM {} 50% {} from {} }";
    let diags = check(source, Syntax::Css);
    let found: Vec<_> = diags
      .iter()
      .map(|d| (d.message.as_str(), &source[d.span.offset..d.span.end()]))
      .collect();
    assert_eq!(
      found,
      [
        ("Unexpected duplicate keyframe selector \"50%\"", "50%"),
        ("Unexpected duplicate keyframe selector \"from\"", "from"),
      ]
    );
    assert_eq!(diags[0].span.offset, source.rfind("50%").unwrap());
  }

  #[test]
  fn reads_prefixed_and_upper_case_keyframes_in_every_syntax() {
    for name in ["-webkit-keyframes", "KEYFRAMES", "-moz-Keyframes"] {
      let source = format!("@{name} k {{ to {{}} to {{}} }}");
      assert_eq!(check(&source, Syntax::Scss).len(), 1, "{name}");
    }
    assert!(check("@-foo-keyframes k { to {} to {} }", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        "keyframe-block-no-duplicate-selectors",
        serde_json::json!(true)
      )
      .is_empty()
    );
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "keyframe-block-no-duplicate-selectors",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        "keyframe-block-no-duplicate-selectors",
        serde_json::json!("always")
      ),
      ["Unexpected option value \"always\" for rule \"keyframe-block-no-duplicate-selectors\""]
    );
  }
}
