use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Span;

use crate::list_rules::{Found, List, ListCheck, ListRule, Polarity, subjects};
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "comment-word-disallowed-list";

declare_rule! {
  /// Disallow words in comments.
  ///
  /// Options: a string or `/regex/`, or a list of them.  A comment is
  /// reported when its text equals a string or contains it, or when a
  /// regex matches it, case-sensitively as Stylelint does; the problem
  /// points at the word.  Source map comments (`/*# ... */`) are skipped.
  ///
  /// Equivalent to Stylelint's `comment-word-disallowed-list` rule.
  pub struct CommentWordDisallowedList => ListCheck<CommentWordDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed words within comments",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for CommentWordDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::comment_texts(tree, found);
  }

  /// `matchesStringOrRegExp(text, list) || containsString(text, list)`.
  fn listed<'l>(list: &'l List, _: &(), found: &Found, _: &PostcssTree) -> Option<&'l str> {
    list
      .find(&found.name)
      .or_else(|| list.find_within(&found.name))
  }

  fn message(_: &Found, entry: Option<&str>) -> String {
    format!("Unexpected word \"{}\" in comment", entry.unwrap_or(""))
  }

  /// The word the entry matched.
  fn span(list: &List, found: &Found, entry: Option<&str>) -> Span {
    match entry.and_then(|entry| list.matched_text(entry, &found.name)) {
      Some((at, word)) => Span::new(found.span.offset + at, word.len()),
      None => found.span,
    }
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::{check, check_css};
  use crate::testing::invalid_options;

  #[test]
  fn reports_comments_holding_a_listed_word() {
    check_css(
      NAME,
      &[
        (
          json!(["TODO", "FIXME"]),
          "/* TODO: fix */\na {}",
          &["1:4 Unexpected word \"TODO\" in comment"],
        ),
        (
          json!(["/^todo/i"]),
          "/* todo later */",
          &["1:4 Unexpected word \"/^todo/i\" in comment"],
        ),
        (json!(["todo"]), "/* TODO */", &[]),
        (json!(["TODO"]), "/* fine */", &[]),
        (
          json!(["sourceMappingURL"]),
          "/*# sourceMappingURL=a.map */",
          &[],
        ),
        (
          json!(["TODO", "HACK"]),
          "/* TODO and HACK */",
          &["1:4 Unexpected word \"TODO\" in comment"],
        ),
      ],
    );
    check(
      NAME,
      gale_css_parser::Syntax::Scss,
      &[(
        json!(["hack"]),
        "a {\n  // a hack\n}",
        &["2:8 Unexpected word \"hack\" in comment"],
      )],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, json!([3])),
      ["Invalid option value \"3\" for rule \"comment-word-disallowed-list\""]
    );
  }
}
