use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "selector-pseudo-class-allowed-list";

declare_rule! {
  /// Only allow the pseudo-classes listed.
  ///
  /// Options: a pseudo-class name (without the colon) or `/regex/`, or a
  /// list of them, matched exactly as written, as Stylelint 17 does:
  /// `:HOVER` and `:-moz-focusring` need entries of their own.  Selectors
  /// in `@page` are checked too.
  ///
  /// Equivalent to Stylelint's `selector-pseudo-class-allowed-list` rule.
  pub struct SelectorPseudoClassAllowedList => ListCheck<SelectorPseudoClassAllowedList> {
    name: NAME,
    description: "Specify a list of allowed pseudo-class selectors",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for SelectorPseudoClassAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::pseudo_classes(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected pseudo-class \":{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_pseudo_classes_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["hover"]),
          "a:hover, a:focus::before {}",
          &["1:11 Unexpected pseudo-class \":focus\""],
        ),
        (
          json!(["not"]),
          "a:not(:first-child) {}",
          &["1:7 Unexpected pseudo-class \":first-child\""],
        ),
        (json!(["/^nth-/"]), "li:nth-child(2n) {}", &[]),
        (
          json!(["hover"]),
          "a:HOVER {}",
          &["1:2 Unexpected pseudo-class \":HOVER\""],
        ),
        (
          json!(["hover"]),
          "@page :first { margin: 0; }",
          &["1:7 Unexpected pseudo-class \":first\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!(["hover", "/^nth/"])).is_empty());
  }
}
