use crate::list_rules::{Entries, Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-combinator-allowed-list";

declare_rule! {
  /// Only allow the selector combinators listed.
  ///
  /// Options: a combinator (`>`, `+`, `~`, `||`, or `" "` for the
  /// descendant combinator) or a list of them; entries are compared
  /// literally against the combinator with its whitespace collapsed.
  /// Reference combinators (`/deep/`) are not checked.
  ///
  /// Equivalent to Stylelint's `selector-combinator-allowed-list` rule.
  pub struct SelectorCombinatorAllowedList => ListCheck<SelectorCombinatorAllowedList> {
    name: NAME,
    description: "Specify a list of allowed selector combinators",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorCombinatorAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  const ENTRIES: Entries = Entries::Names;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::combinators(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    if found.name == " " {
      "Unexpected combinator descendant ( )".to_string()
    } else {
      format!("Unexpected combinator \"{}\"", found.name)
    }
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_combinators_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!([">"]),
          "a > b + c d {}",
          &[
            "1:7 Unexpected combinator \"+\"",
            "1:10 Unexpected combinator descendant ( )",
          ],
        ),
        (json!([" ", ">"]), "a  >  b c {}", &[]),
        (json!([">"]), "> a {}", &[]),
        (
          json!([">"]),
          "a:not(b ~ c) {}",
          &["1:9 Unexpected combinator \"~\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([[">", {}]])),
      ["Invalid option value \"{}\" for rule \"selector-combinator-allowed-list\""]
    );
  }
}
