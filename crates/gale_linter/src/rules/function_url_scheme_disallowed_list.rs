use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "function-url-scheme-disallowed-list";

declare_rule! {
  /// Disallow the URL schemes listed in `url()`.
  ///
  /// Options: a scheme or `/regex/`, or a list of them, matched against the
  /// lowercased scheme.  A URL has a scheme when it is absolute and the
  /// scheme is followed by `//`, or is `data:`.
  ///
  /// Equivalent to Stylelint's `function-url-scheme-disallowed-list` rule.
  pub struct FunctionUrlSchemeDisallowedList => ListCheck<FunctionUrlSchemeDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed URL schemes",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for FunctionUrlSchemeDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::url_schemes(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected URL scheme \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_schemes() {
    check_css(
      NAME,
      &[
        (
          json!(["http"]),
          "a { background: URL(\"HTTP://a.b/c.png\"); }",
          &["1:21 Unexpected URL scheme \"http\""],
        ),
        (
          json!(["data"]),
          "a { background: url(data:image/png;base64,AAAA); }",
          &["1:21 Unexpected URL scheme \"data\""],
        ),
        (
          json!(["http"]),
          "a { background: url(https://a.b/c.png); }",
          &[],
        ),
        (
          json!(["localhost"]),
          "a { background: url(localhost:8080/a); }",
          &[],
        ),
        (
          json!(["http"]),
          "a { background: url($base + 'http://a'); }",
          &[],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!(["http", "/^ftp/"])).is_empty());
    assert_eq!(
      invalid_options(NAME, json!(false)),
      [
        "Invalid option value \"false\" for rule \"function-url-scheme-disallowed-list\". Are you trying to disable this rule? If so use \"null\" instead"
      ]
    );
  }
}
