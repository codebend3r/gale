use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "property-disallowed-list";

declare_rule! {
  /// Disallow the properties listed.
  ///
  /// Options: a property name or `/regex/`, or a list of them, matched
  /// exactly as written, as Stylelint 17 does.  Sass and Less variables,
  /// Less merges and interpolated properties are not checked.
  ///
  /// Equivalent to Stylelint's `property-disallowed-list` rule.
  pub struct PropertyDisallowedList => ListCheck<PropertyDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed properties",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for PropertyDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::property_names(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected property \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_properties() {
    check_css(
      NAME,
      &[
        (
          json!(["color"]),
          "a { color: red; margin: 0; }",
          &["1:5 Unexpected property \"color\""],
        ),
        (
          json!([["color"], { "severity": "warning" }]),
          "a { color: red; }",
          &["1:5 Unexpected property \"color\""],
        ),
        (
          json!(["/^border/"]),
          "a { border-top: 0; margin: 0; }",
          &["1:5 Unexpected property \"border-top\""],
        ),
        (json!(["transform"]), "a { -webkit-transform: none; }", &[]),
        (json!(["color"]), "a { COLOR: red; }", &[]),
        (
          json!(["color"]),
          "@media print { a { color: red; } }",
          &["1:20 Unexpected property \"color\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([null, 1])),
      Vec::<String>::new(),
      "a rule set to null is off"
    );
    assert_eq!(
      invalid_options(NAME, json!(["color", 1])),
      ["Invalid option value \"1\" for rule \"property-disallowed-list\""]
    );
  }
}
