use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "media-feature-name-allowed-list";

declare_rule! {
  /// Only allow the media feature names listed.
  ///
  /// Options: a feature name or `/regex/`, or a list of them, matched
  /// exactly as written, as Stylelint 17 does: `min-width` needs an entry
  /// of its own beside `width`.  Range features count by their name, and
  /// custom media (`--name`) is not checked.
  ///
  /// Equivalent to Stylelint's `media-feature-name-allowed-list` rule.
  pub struct MediaFeatureNameAllowedList => ListCheck<MediaFeatureNameAllowedList> {
    name: NAME,
    description: "Specify a list of allowed media feature names",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for MediaFeatureNameAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::media_feature_names(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected media feature name \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_features_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["width"]),
          "@media screen and (min-width: 10px) and (color) {}",
          &[
            "1:20 Unexpected media feature name \"min-width\"",
            "1:42 Unexpected media feature name \"color\"",
          ],
        ),
        (
          json!(["/width$/"]),
          "@media (min-width: 10px) and (max-width: 20px) {}",
          &[],
        ),
        (
          json!(["width"]),
          "@media (400px <= width <= 700px), not all and (height > 1px) {}",
          &["1:48 Unexpected media feature name \"height\""],
        ),
        (
          json!(["width"]),
          "@media (WIDTH: 1px) {}",
          &["1:9 Unexpected media feature name \"WIDTH\""],
        ),
        (json!(["width"]), "@media (--custom) {}", &[]),
        (json!(["width"]), "@supports (height: 1px) {}", &[]),
        (
          json!(["width"]),
          "@media not ((height: 1px) or (width: 2px)) {}",
          &["1:14 Unexpected media feature name \"height\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([["width", 2]])),
      ["Invalid option value \"2\" for rule \"media-feature-name-allowed-list\""]
    );
  }
}
