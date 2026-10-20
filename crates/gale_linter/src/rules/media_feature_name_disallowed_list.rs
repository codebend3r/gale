use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "media-feature-name-disallowed-list";

declare_rule! {
  /// Disallow the media feature names listed.
  ///
  /// Options: a feature name or `/regex/`, or a list of them, matched
  /// exactly as written, as Stylelint 17 does.  Range features count by
  /// their name, and custom media (`--name`) is not checked.
  ///
  /// Equivalent to Stylelint's `media-feature-name-disallowed-list` rule.
  pub struct MediaFeatureNameDisallowedList => ListCheck<MediaFeatureNameDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed media feature names",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for MediaFeatureNameDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
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
  fn reports_listed_features() {
    check_css(
      NAME,
      &[
        (
          json!(["max-width"]),
          "@media (min-width: 1px) and (max-width: 2px) {}",
          &["1:30 Unexpected media feature name \"max-width\""],
        ),
        (
          json!(["/^device-/"]),
          "@media (device-width < 2px) {}",
          &["1:9 Unexpected media feature name \"device-width\""],
        ),
        (json!(["max-width"]), "@media (MAX-WIDTH: 2px) {}", &[]),
        (json!(["max-width"]), "@media print {}", &[]),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!("max-width")).is_empty());
  }
}
