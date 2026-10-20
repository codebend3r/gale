use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

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
  /// Flags comment text that does not match the configured regex.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Comment(comment) = node else {
      return vec![];
    };

    let Some(pattern) = &self.pattern else {
      return vec![];
    };
    let pattern_str = &pattern.source;

    let text = comment.text.trim();
    if !pattern.is_match(text) {
      return vec![
        Diagnostic::new(
          NAME,
          format!("Expected comment to match pattern \"/{pattern_str}/\""),
        )
        .span(Span::new(comment.span.offset, comment.span.length)),
      ];
    }
    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Comment, Span as ParserSpan};

  use crate::testing::{configured, ctx, invalid_options};

  fn comment_node(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      text: text.to_string(),
      span: ParserSpan::new(0, text.len() + 4),
      is_line: false,
    })
  }

  #[test]
  fn reports_comment_not_matching_pattern() {
    let rule = configured(&CommentPattern, serde_json::json!("^TODO:"));
    let ctx = ctx();
    let d = rule.check(&comment_node("some random comment"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("TODO:"));
  }

  #[test]
  fn allows_comment_matching_pattern() {
    let rule = configured(&CommentPattern, serde_json::json!("^TODO:"));
    let ctx = ctx();
    let d = rule.check(&comment_node("TODO: fix this"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let rule = configured(&CommentPattern, None);
    let d = rule.check(&comment_node("anything"), &ctx());
    assert!(d.is_empty());
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
