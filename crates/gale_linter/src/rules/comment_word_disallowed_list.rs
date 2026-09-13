use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specified words in comments.
///
/// Options: an array of strings (or regex-like patterns) that are disallowed
/// inside CSS comments.
///
/// Equivalent to Stylelint's `comment-word-disallowed-list` rule.
pub struct CommentWordDisallowedList;

impl Rule for CommentWordDisallowedList {
  fn name(&self) -> &'static str {
    "comment-word-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed words within comments"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Comment(comment) = node else {
      return vec![];
    };

    let disallowed: Vec<String> = match ctx.options {
      Some(serde_json::Value::Array(arr)) => arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect(),
      _ => return vec![],
    };

    let text_lower = comment.text.to_ascii_lowercase();
    let mut diags = Vec::new();

    for word in &disallowed {
      let word_lower = word.to_ascii_lowercase();
      if text_lower.contains(&word_lower) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected word \"{word}\" in comment"),
          )
          .severity(self.default_severity())
          .span(Span::new(comment.span.offset, comment.span.length)),
        );
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Comment, Span as ParserSpan, Syntax};

  fn ctx_with_options(options: Option<serde_json::Value>) -> RuleContext<'static> {
    let opts: Option<&'static serde_json::Value> = options.map(|v| &*Box::leak(Box::new(v)));
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: opts,
    }
  }

  fn comment_node(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      text: text.to_string(),
      span: ParserSpan::new(0, text.len() + 4), // +4 for /* */
      is_line: false,
    })
  }

  #[test]
  fn allows_all_when_no_options() {
    let ctx = ctx_with_options(None);
    let d = CommentWordDisallowedList.check(&comment_node("TODO: fix this"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn flags_disallowed_word() {
    let ctx = ctx_with_options(Some(serde_json::json!(["TODO", "FIXME"])));
    let d = CommentWordDisallowedList.check(&comment_node("TODO: fix this"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("TODO"));
  }

  #[test]
  fn flags_multiple_disallowed_words() {
    let ctx = ctx_with_options(Some(serde_json::json!(["TODO", "HACK"])));
    let d = CommentWordDisallowedList.check(&comment_node("TODO: HACK around it"), &ctx);
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn allows_comment_without_disallowed_words() {
    let ctx = ctx_with_options(Some(serde_json::json!(["TODO"])));
    let d = CommentWordDisallowedList.check(&comment_node("This is fine"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn case_insensitive_match() {
    let ctx = ctx_with_options(Some(serde_json::json!(["todo"])));
    let d = CommentWordDisallowedList.check(&comment_node("TODO: fix this"), &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      CommentWordDisallowedList.name(),
      "comment-word-disallowed-list"
    );
  }
}
