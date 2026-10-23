use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "comment-pattern";

declare_rule! {
  /// Specify a pattern for comments.
  ///
  /// Equivalent to Stylelint's `comment-pattern` rule.
  /// Options: a regex string that comment text must match.
  pub struct CommentPattern => CommentPatternCheck {
    name: NAME,
    description: "Specify a pattern for comments",
    default_severity: Warning,
  }
}

/// `comment-pattern` as configured.
pub struct CommentPatternCheck {
  /// The pattern comment text must match; `None` when the config gives
  /// none, and the rule checks nothing.
  pattern: Option<Pattern>,
}

impl CommentPatternCheck {
  /// Stylelint's option: a regular expression, as a string or `/regex/`.
  /// One that does not compile is an invalid option.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.finish()?;
    Ok(Self { pattern })
  }
}

impl Check for CommentPatternCheck {
  /// Flags comment text (`comment.text`, trimmed) that does not match the
  /// configured regex.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(pattern) = &self.pattern else {
      return vec![];
    };
    let pattern_str = &pattern.source;
    tree
      .walk_comments()
      .filter(|comment| !pattern.is_match(comment.text()))
      .map(|comment| {
        Diagnostic::new(
          NAME,
          format!("Expected comment to match pattern \"/{pattern_str}/\""),
        )
        .span(comment.span())
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options, lint};

  #[test]
  fn reports_comment_not_matching_pattern() {
    let d = lint(
      NAME,
      json!("^TODO:"),
      "/* some random comment */",
      Syntax::Css,
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("TODO:"));
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 25));
    let d = lint(NAME, json!("^TODO:"), "a {}\n// other", Syntax::Scss);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_comment_matching_pattern() {
    let css = "/*  TODO: fix this */ a { /* TODO: and this */ }";
    assert!(lint(NAME, json!("^TODO:"), css, Syntax::Css).is_empty());
    assert!(lint(NAME, json!("^TODO:"), "// TODO: x", Syntax::Scss).is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let rule = configured(&CommentPattern, None);
    assert!(
      rule
        .check_source(&ctx_with_source("/* anything */"))
        .is_empty()
    );
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(CommentPattern::META.name, "comment-pattern");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("/^TODO/")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"comment-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-\" for rule \"comment-pattern\": "),
      "{broken:?}"
    );
  }
}
