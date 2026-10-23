use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/comment-no-empty";

declare_rule! {
  /// SCSS-specific `comment-no-empty`.
  ///
  /// Disallows empty comments, block (`/* */`) and line (`//`) alike, as
  /// stylelint-scss does: a comment whose `comment.text` is empty.  Only
  /// active in SCSS/Sass files.
  pub struct ScssCommentNoEmpty => ScssCommentNoEmptyCheck {
    name: NAME,
    description: "Disallow empty comments (SCSS-aware)",
    default_severity: Warning,
  }
}

/// `scss/comment-no-empty`, which takes no options.
pub struct ScssCommentNoEmptyCheck;

impl ScssCommentNoEmptyCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssCommentNoEmptyCheck {
  /// Flags empty block and `//` comments in SCSS files.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    tree
      .walk_comments()
      .filter(|comment| comment.text().is_empty())
      .map(|comment| Diagnostic::new(NAME, "Unexpected empty comment").span(comment.span()))
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  fn scss(source: &str) -> Vec<Diagnostic> {
    lint(NAME, json!(true), source, Syntax::Scss)
  }

  #[test]
  fn skips_non_scss() {
    assert!(lint(NAME, json!(true), "/* */", Syntax::Css).is_empty());
  }

  #[test]
  fn reports_empty_block_comment() {
    let d = scss("/* */");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Unexpected empty comment");
  }

  #[test]
  fn reports_empty_line_comments() {
    // `//` with nothing (or only whitespace, a CR included) after it.
    assert_eq!(scss("//\na {}\n//  \r\n").len(), 2);
  }

  #[test]
  fn skips_non_empty_comments() {
    assert!(scss("// some text\n/* hello */").is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/comment-no-empty\""]
    );
  }
}
