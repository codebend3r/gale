use std::borrow::Cow;

use crate::list_rules::subjects::{self, UnitFilter};
use crate::list_rules::{
  Entries, Found, KeyedList, List, ListCheck, ListRule, Polarity, keyed_option, list_option,
};
use crate::options::Options;
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "unit-disallowed-list";

declare_rule! {
  /// Disallow the units listed.
  ///
  /// Options: a unit or a list of them, compared with the lowercased unit;
  /// `%` counts as a unit.  Units in declaration values and in the media
  /// features of `@media` params are checked, but not those inside `url()`
  /// or in `unicode-range`.  `ignoreFunctions` names functions whose
  /// arguments are not checked; `ignoreProperties` and
  /// `ignoreMediaFeatureNames` map a unit to the properties or media
  /// features it is allowed in anyway.
  ///
  /// Equivalent to Stylelint's `unit-disallowed-list` rule.
  pub struct UnitDisallowedList => ListCheck<UnitDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed units",
    default_severity: Warning,
    primary_option_array: true,
  }
}

/// `unit-disallowed-list`'s secondary options.
#[derive(Default)]
pub struct UnitDisallowedListOptions {
  ignore_functions: List,
  ignore_properties: KeyedList,
  ignore_media_feature_names: KeyedList,
}

impl ListRule for UnitDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  const ENTRIES: Entries = Entries::Names;
  type Options = UnitDisallowedListOptions;

  fn options(options: &mut Options<'_>) -> Self::Options {
    UnitDisallowedListOptions {
      ignore_functions: list_option(options, "ignoreFunctions"),
      ignore_properties: keyed_option(options, "ignoreProperties"),
      ignore_media_feature_names: keyed_option(options, "ignoreMediaFeatureNames"),
    }
  }

  fn find(tree: &PostcssTree, _: &RuleContext, options: &Self::Options, found: &mut Vec<Found>) {
    let exempt = |lists: &KeyedList, unit: &str, name: &str| {
      lists.first(unit).is_some_and(|list| list.matches(name))
    };
    let filter = UnitFilter {
      function: &|name| options.ignore_functions.matches(name),
      property: &|unit, property| exempt(&options.ignore_properties, unit, property),
      media_feature: &|unit, feature| exempt(&options.ignore_media_feature_names, unit, feature),
    };
    subjects::token_units(tree, &filter, found);
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
  fn reports_listed_units() {
    check_css(
      NAME,
      &[
        (
          json!(["px", "%"]),
          "a { width: 1PX; height: 50%; top: 1em; }",
          &["1:13 Unexpected unit \"PX\"", "1:27 Unexpected unit \"%\""],
        ),
        (json!(["px"]), "a { background: url(a-1px.png); }", &[]),
        (
          json!(["em"]),
          "@font-face { unicode-range: U+0025-00FF; }",
          &[],
        ),
        (
          json!(["px"]),
          "@media (min-width: 10px) and (max-width: /* 2px */ 20em) {}",
          &["1:22 Unexpected unit \"px\""],
        ),
        (
          json!(["px"]),
          "a { width: /* 1px */ var(--a, 2px); }",
          &["1:32 Unexpected unit \"px\""],
        ),
      ],
    );
  }

  #[test]
  fn ignores_functions_properties_and_media_features() {
    check_css(
      NAME,
      &[
        (
          json!([["px"], { "ignoreFunctions": ["calc"] }]),
          "a { width: CALC(1px + 2px); height: 1px; }",
          &["1:38 Unexpected unit \"px\""],
        ),
        (
          json!([["px"], { "ignoreProperties": { "px": ["/^border/"] } }]),
          "a { border-top: 1px; margin: 1px; }",
          &["1:31 Unexpected unit \"px\""],
        ),
        (
          json!([["px"], { "ignoreMediaFeatureNames": { "px": ["min-width"] } }]),
          "@media (MIN-WIDTH: 1px) and (max-width: 2px) {}",
          &["1:42 Unexpected unit \"px\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(
        NAME,
        json!([["px"], { "ignoreMediaFeatureNames": { "px": [true] } }])
      ),
      [
        "Invalid value \"{\"px\":[true]}\" for option \"ignoreMediaFeatureNames\" of rule \"unit-disallowed-list\""
      ]
    );
  }
}
