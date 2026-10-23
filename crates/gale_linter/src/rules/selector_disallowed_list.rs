use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::subjects::{self, SelectorOptions};
use crate::list_rules::{Found, ListCheck, ListRule, Polarity};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-disallowed-list";

declare_rule! {
  /// Disallow the selectors listed.
  ///
  /// Options: a selector or `/regex/`, or a list of them.  A string must
  /// equal the rule's whole selector as written; a regex need only match
  /// part of it.  `splitList: true` matches each selector of a list on its
  /// own; `ignore: ["inside-block"]` skips nested rules and
  /// `ignore: ["keyframe-selectors"]` the selectors in `@keyframes`.
  ///
  /// Equivalent to Stylelint's `selector-disallowed-list` rule.
  pub struct SelectorDisallowedList => ListCheck<SelectorDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed selectors",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = SelectorOptions;

  fn options(options: &mut Options<'_>) -> SelectorOptions {
    let ignore = options.keywords("ignore", &["inside-block", "keyframe-selectors"]);
    SelectorOptions {
      ignore_inside_block: ignore.contains(&"inside-block"),
      ignore_keyframes: ignore.contains(&"keyframe-selectors"),
      split_list: options.flag("splitList"),
    }
  }

  fn find(tree: &PostcssTree, _: &RuleContext, options: &SelectorOptions, found: &mut Vec<Found>) {
    subjects::selectors(tree, *options, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected selector \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_selectors() {
    check_css(
      NAME,
      &[
        (
          json!(["a > b"]),
          "a > b { color: red; }",
          &["1:1 Unexpected selector \"a > b\""],
        ),
        (json!(["a > b"]), "a > b, c { color: red; }", &[]),
        (
          json!(["/\\.foo/"]),
          "a, .foo { color: red; }",
          &["1:1 Unexpected selector \"a, .foo\""],
        ),
        (
          json!([["c"], { "splitList": true }]),
          "a, c { color: red; }",
          &["1:4 Unexpected selector \"c\""],
        ),
        (
          json!([["b"], { "ignore": ["inside-block"] }]),
          "b {}\na { b {} }",
          &["1:1 Unexpected selector \"b\""],
        ),
        (
          json!([["from"], { "ignore": ["keyframe-selectors"] }]),
          "@keyframes k { from {} }",
          &[],
        ),
        (
          json!(["from"]),
          "@keyframes k { from {} }",
          &["1:16 Unexpected selector \"from\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(
        NAME,
        json!([["a"], { "ignore": ["outside"], "splitList": "yes" }])
      ),
      [
        "Invalid value \"outside\" for option \"ignore\" of rule \"selector-disallowed-list\"",
        "Invalid value \"yes\" for option \"splitList\" of rule \"selector-disallowed-list\"",
      ]
    );
  }
}
