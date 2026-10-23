use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "property-allowed-list";

declare_rule! {
  /// Only allow the properties listed.
  ///
  /// Options: a property name or `/regex/`, or a list of them, matched
  /// exactly as written, as Stylelint 17 does: `-webkit-transform` needs
  /// an entry of its own.  Sass and Less variables, Less merges and
  /// interpolated properties are not checked.
  ///
  /// Equivalent to Stylelint's `property-allowed-list` rule.
  pub struct PropertyAllowedList => ListCheck<PropertyAllowedList> {
    name: NAME,
    description: "Specify a list of allowed properties",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for PropertyAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
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
  use crate::list_rules::testing::{check, check_css};
  use crate::testing::invalid_options;

  #[test]
  fn reports_properties_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["color"]),
          "a { color: red; margin: 0; }",
          &["1:17 Unexpected property \"margin\""],
        ),
        (
          json!([["color", "margin"], { "severity": "warning" }]),
          "a { color: red; margin: 0; padding: 0; }",
          &["1:28 Unexpected property \"padding\""],
        ),
        (
          json!("color"),
          "a { Color: red; }",
          &["1:5 Unexpected property \"Color\""],
        ),
        (
          json!(["transform"]),
          "a { -webkit-transform: none; }",
          &["1:5 Unexpected property \"-webkit-transform\""],
        ),
        (
          json!(["/^border/", "--x"]),
          "a { border-top: 0; --x: 1; }",
          &[],
        ),
      ],
    );
    check(
      NAME,
      gale_css_parser::Syntax::Scss,
      &[(json!(["color"]), "$size: 1px;\na { color: red; }", &[])],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!(5)),
      ["Invalid option value \"5\" for rule \"property-allowed-list\""]
    );
    assert_eq!(
      invalid_options(NAME, json!(["color", true])),
      ["Invalid option value \"true\" for rule \"property-allowed-list\""]
    );
    assert!(invalid_options(NAME, json!(["color", "/^margin/"])).is_empty());
  }
}
