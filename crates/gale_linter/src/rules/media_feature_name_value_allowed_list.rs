use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{KeyedCheck, KeyedFound, KeyedListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "media-feature-name-value-allowed-list";

declare_rule! {
  /// Only allow the values listed for a media feature.
  ///
  /// Options: an object mapping a media feature name or `/regex/` to the
  /// values allowed for it, each a string that must equal the whole value
  /// or a `/regex/` that need only match part of it.  The first key that
  /// matches the name as written applies; each side of a range feature is
  /// a value of its own.
  ///
  /// Equivalent to Stylelint's `media-feature-name-value-allowed-list` rule.
  pub struct MediaFeatureNameValueAllowedList => KeyedCheck<MediaFeatureNameValueAllowedList> {
    name: NAME,
    description: "Specify a list of allowed values for specific media feature names",
    default_severity: Warning,
    primary_option_object: true,
  }
}

impl KeyedListRule for MediaFeatureNameValueAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<KeyedFound>) {
    subjects::media_feature_values(tree, found);
  }

  fn message(found: &KeyedFound) -> String {
    format!(
      "Unexpected value \"{}\" for media feature \"{}\"",
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
  fn reports_values_missing_from_the_feature_list() {
    check_css(
      NAME,
      &[
        (
          json!({ "min-width": ["768px", "1024px"] }),
          "@media (min-width: 500px) and (min-width: 768px) {}",
          &["1:20 Unexpected value \"500px\" for media feature \"min-width\""],
        ),
        (
          json!({ "/resolution/": ["/dpi$/"] }),
          "@media (min-resolution: 2dppx) {}",
          &["1:25 Unexpected value \"2dppx\" for media feature \"min-resolution\""],
        ),
        (
          json!({ "width": ["10px"] }),
          "@media (10px < width <= 20px) {}",
          &["1:25 Unexpected value \"20px\" for media feature \"width\""],
        ),
        (json!({ "width": ["10px"] }), "@media (color) {}", &[]),
        (
          json!({ "width": ["10px"] }),
          "@media (min-width: 1px) {}",
          &[],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!({ "width": "/px$/" })).is_empty());
  }
}
