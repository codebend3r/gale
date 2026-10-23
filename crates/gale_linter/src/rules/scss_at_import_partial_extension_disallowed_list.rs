use std::borrow::Cow;

use gale_css_parser::postcss_tree::PostcssTree;

use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "scss/at-import-partial-extension-disallowed-list";

declare_rule! {
  /// Disallow the file extensions listed in the partials `@import` names.
  ///
  /// Options: an extension without the dot (`"scss"`) or `/regex/`, or a
  /// list of them, matched against the lowercased extension.  CSS imports
  /// (`.css`, `url()`, URLs, media queries) are not checked.
  ///
  /// Equivalent to `scss/at-import-partial-extension-disallowed-list`.
  pub struct ScssAtImportPartialExtensionDisallowedList => ListCheck<ScssAtImportPartialExtensionDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed file extensions for partial names in @import",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for ScssAtImportPartialExtensionDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::import_extensions(tree, found);
  }

  fn normalize(extension: &str) -> Cow<'_, str> {
    Cow::Owned(extension.to_ascii_lowercase())
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!(
      "Unexpected extension \".{}\" in @import \"{}\"",
      found.name, found.detail
    )
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_extensions() {
    check(
      NAME,
      Syntax::Scss,
      &[
        (
          json!(["scss"]),
          "@import \"foo.scss\";",
          &["1:14 Unexpected extension \".scss\" in @import \"foo.scss\""],
        ),
        (
          json!(["/^s[ac]ss$/"]),
          "@import 'a.SASS', 'b';",
          &["1:12 Unexpected extension \".SASS\" in @import \"a.SASS\""],
        ),
        (json!(["scss"]), "@import \"foo\";", &[]),
        (json!(["css"]), "@import \"foo.css\";", &[]),
        (json!(["scss"]), "@import url(foo.scss);", &[]),
        (json!(["scss"]), "@import \"http://a.b/foo.scss\";", &[]),
        (json!(["scss"]), "@use \"foo.scss\";", &[]),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!(["scss", "/^less$/"])).is_empty());
  }
}
