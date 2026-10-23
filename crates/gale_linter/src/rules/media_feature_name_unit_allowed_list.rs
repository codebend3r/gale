use std::borrow::Cow;

use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Entries, KeyedCheck, KeyedFound, KeyedListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "media-feature-name-unit-allowed-list";

declare_rule! {
  /// Only allow the units listed for a media feature.
  ///
  /// Options: an object mapping a media feature name or `/regex/` to the
  /// units allowed in its value; the first key that matches the name as
  /// written applies, so `width` does not cover `min-width`.  Units are
  /// compared lowercased.
  ///
  /// Equivalent to Stylelint's `media-feature-name-unit-allowed-list` rule.
  pub struct MediaFeatureNameUnitAllowedList => KeyedCheck<MediaFeatureNameUnitAllowedList> {
    name: NAME,
    description: "Specify a list of allowed units for specific media feature names",
    default_severity: Warning,
    primary_option_object: true,
  }
}

impl KeyedListRule for MediaFeatureNameUnitAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  const ENTRIES: Entries = Entries::Names;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<KeyedFound>) {
    subjects::media_feature_units(tree, found);
  }

  fn normalize(unit: &str) -> Cow<'_, str> {
    Cow::Owned(unit.to_ascii_lowercase())
  }

  fn message(found: &KeyedFound) -> String {
    format!(
      "Unexpected unit \"{}\" for media feature \"{}\"",
      found.name, found.key
    )
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_units_missing_from_the_feature_list() {
    check_css(
      NAME,
      &[
        (
          json!({ "/width$/": ["em"] }),
          "@media (min-width: 10px) and (max-width: 20EM) {}",
          &["1:20 Unexpected unit \"px\" for media feature \"min-width\""],
        ),
        (
          json!({ "width": ["em"] }),
          "@media (min-width: 10px) {}",
          &[],
        ),
        (
          json!({ "width": ["em"] }),
          "@media (10px <= width < 20em) {}",
          &["1:9 Unexpected unit \"px\" for media feature \"width\""],
        ),
        (
          json!({ "resolution": "dpi" }),
          "@media (resolution: 2dppx) {}",
          &["1:21 Unexpected unit \"dppx\" for media feature \"resolution\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!({ "width": ["em", "rem"] })).is_empty());
  }
}
