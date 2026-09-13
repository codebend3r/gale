use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Specify a pattern for comments.
///
/// Equivalent to Stylelint's `comment-pattern` rule.
/// Options: a regex string that comment text must match.
pub struct CommentPattern;

impl Rule for CommentPattern {
  fn name(&self) -> &'static str {
    "comment-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for comments"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Comment(comment) = node else {
      return vec![];
    };

    let pattern_str = match ctx.primary_option_str() {
      Some(s) => s,
      None => return vec![],
    };

    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    let text = comment.text.trim();
    if !re.is_match(text) {
      return vec![
        Diagnostic::new(
          self.name(),
          format!("Expected comment to match pattern \"/{pattern_str}/\""),
        )
        .severity(self.default_severity())
        .span(Span::new(comment.span.offset, comment.span.length)),
      ];
    }
    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Comment, Span as ParserSpan, Syntax};

  fn ctx_with_options(options: serde_json::Value) -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(Box::leak(Box::new(options))),
    }
  }

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn comment_node(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      text: text.to_string(),
      span: ParserSpan::new(0, text.len() + 4),
      is_line: false,
    })
  }

  #[test]
  fn reports_comment_not_matching_pattern() {
    let ctx = ctx_with_options(serde_json::json!("^TODO:"));
    let d = CommentPattern.check(&comment_node("some random comment"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("TODO:"));
  }

  #[test]
  fn allows_comment_matching_pattern() {
    let ctx = ctx_with_options(serde_json::json!("^TODO:"));
    let d = CommentPattern.check(&comment_node("TODO: fix this"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = CommentPattern.check(&comment_node("anything"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(CommentPattern.name(), "comment-pattern");
  }
}
