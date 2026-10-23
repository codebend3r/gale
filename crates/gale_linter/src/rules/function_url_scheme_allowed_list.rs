use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "function-url-scheme-allowed-list";

declare_rule! {
  /// Only allow the URL schemes listed in `url()`.
  ///
  /// Options: a scheme or `/regex/`, or a list of them, matched against the
  /// lowercased scheme.  A URL has a scheme when it is absolute and the
  /// scheme is followed by `//`, or is `data:`; relative and
  /// scheme-relative URLs, and URLs built with interpolation, are not
  /// checked.
  ///
  /// Equivalent to Stylelint's `function-url-scheme-allowed-list` rule.
  pub struct FunctionUrlSchemeAllowedList => ListCheck<FunctionUrlSchemeAllowedList> {
    name: NAME,
    description: "Specify a list of allowed URL schemes",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for FunctionUrlSchemeAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
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
  fn reports_schemes_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (
          json!(["https"]),
          "a { background: url(http://example.com/a.png); }",
          &["1:21 Unexpected URL scheme \"http\""],
        ),
        (
          json!(["https"]),
          "a { background: url('HTTPS://example.com/a.png'); }",
          &[],
        ),
        (
          json!(["https"]),
          "a { background: url(data:image/png;base64,AAAA); }",
          &["1:21 Unexpected URL scheme \"data\""],
        ),
        (
          json!(["https"]),
          "a { background: url(//example.com/a.png); }",
          &[],
        ),
        (json!(["https"]), "a { background: url(a/b.png); }", &[]),
        (
          json!(["/^http/"]),
          "a { background: url(\"http://a.b/c\"); }",
          &[],
        ),
        (
          json!(["https"]),
          "@font-face { src: local(x), url(ftp://a.b/f.woff); }",
          &["1:33 Unexpected URL scheme \"ftp\""],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([[1]])),
      ["Invalid option value \"1\" for rule \"function-url-scheme-allowed-list\""]
    );
  }
}
