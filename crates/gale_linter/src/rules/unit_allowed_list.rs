use std::borrow::Cow;

use crate::list_rules::subjects::{self, UnitFilter};
use crate::list_rules::{
  Entries, Found, KeyedList, List, ListCheck, ListRule, Polarity, keyed_option, list_option,
};
use crate::options::Options;
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "unit-allowed-list";

declare_rule! {
  /// Only allow the units listed.
  ///
  /// Options: a unit or a list of them, compared with the lowercased unit.
  /// Units in declaration values and `@media` params are checked, but not
  /// those inside `url()`.  `ignoreFunctions` names functions whose
  /// arguments are not checked; `ignoreProperties` maps a unit to the
  /// properties it is allowed in anyway.
  ///
  /// Equivalent to Stylelint's `unit-allowed-list` rule.
  pub struct UnitAllowedList => ListCheck<UnitAllowedList> {
    name: NAME,
    description: "Specify a list of allowed units",
    default_severity: Warning,
    primary_option_array: true,
  }
}

/// `unit-allowed-list`'s secondary options.
#[derive(Default)]
pub struct UnitAllowedListOptions {
  ignore_functions: List,
  ignore_properties: KeyedList,
}

impl ListRule for UnitAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  const ENTRIES: Entries = Entries::Names;
  type Options = UnitAllowedListOptions;

  fn options(options: &mut Options<'_>) -> Self::Options {
    UnitAllowedListOptions {
      ignore_functions: list_option(options, "ignoreFunctions"),
      ignore_properties: keyed_option(options, "ignoreProperties"),
    }
  }

  fn find(tree: &PostcssTree, _: &RuleContext, options: &Self::Options, found: &mut Vec<Found>) {
    let filter = UnitFilter {
      function: &|name| options.ignore_functions.matches(name),
      property: &|unit, property| {
        options
          .ignore_properties
          .first(unit)
          .is_some_and(|list| list.matches(property))
      },
      media_feature: &|_, _| false,
    };
    subjects::value_units(tree, &filter, found);
  }

  fn normalize(unit: &str) -> Cow<'_, str> {
    Cow::Owned(unit.to_ascii_lowercase())
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected unit \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_units_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["px"]),
          "a { width: 1px; height: 1em; }",
          &["1:26 Unexpected unit \"em\""],
        ),
        (
          json!([["px"], { "severity": "warning" }]),
          "a { margin: 1PX 2%; }",
          &["1:18 Unexpected unit \"%\""],
        ),
        (json!(["px"]), "a { background: url(a-2em.png); }", &[]),
        (json!(["px"]), "a { color: #1e1e1e; width: 1px\\9; }", &[]),
        (
          json!(["px"]),
          "@media (min-width: 30em) {}",
          &["1:22 Unexpected unit \"em\""],
        ),
        (
          json!(["px"]),
          "a { width: calc(1px * 2em); }",
          &["1:24 Unexpected unit \"em\""],
        ),
      ],
    );
  }

  #[test]
  fn ignores_functions_and_properties() {
    check_css(
      NAME,
      &[
        (
          json!([["px"], { "ignoreFunctions": ["/^calc$/"] }]),
          "a { width: calc(1em + 1px); height: 1em; }",
          &["1:38 Unexpected unit \"em\""],
        ),
        (
          json!([["px"], { "ignoreProperties": { "em": ["/^height/"] } }]),
          "a { height: 1em; width: 1em; }",
          &["1:26 Unexpected unit \"em\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(
        NAME,
        json!([["px"], { "ignoreProperties": ["em"], "ignoreFunctions": [1], "other": 1 }])
      ),
      // In the order the options are written, as Stylelint reports them.
      [
        "Invalid value \"em\" for option \"ignoreProperties\" of rule \"unit-allowed-list\"",
        "Invalid value \"1\" for option \"ignoreFunctions\" of rule \"unit-allowed-list\"",
        "Invalid option name \"other\" for rule \"unit-allowed-list\"",
      ]
    );
  }
}
