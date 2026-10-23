use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Entries, Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-combinator-disallowed-list";

declare_rule! {
  /// Disallow the selector combinators listed.
  ///
  /// Options: a combinator (`>`, `+`, `~`, `||`, or `" "` for the
  /// descendant combinator) or a list of them; entries are compared
  /// literally against the combinator with its whitespace collapsed.
  /// Reference combinators (`/deep/`) are not checked.
  ///
  /// Equivalent to Stylelint's `selector-combinator-disallowed-list` rule.
  pub struct SelectorCombinatorDisallowedList => ListCheck<SelectorCombinatorDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed selector combinators",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorCombinatorDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  const ENTRIES: Entries = Entries::Names;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::combinators(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected combinator \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_combinators() {
    check_css(
      NAME,
      &[
        (
          json!([">", " "]),
          "a > b c, d + e {}",
          &[
            "1:3 Unexpected combinator \">\"",
            "1:6 Unexpected combinator \" \"",
          ],
        ),
        (
          json!(["~"]),
          "a\n~\nb {}",
          &["2:1 Unexpected combinator \"~\""],
        ),
        (json!(["+"]), "a {}", &[]),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!(">")).is_empty());
  }
}
