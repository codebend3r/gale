use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// SCSS-specific `comment-no-empty`.
///
/// Disallows empty block comments (`/* */`) but exempts SCSS line comments
/// (`//`). Only active in SCSS/Sass files.
pub struct ScssCommentNoEmpty;

impl Rule for ScssCommentNoEmpty {
  fn name(&self) -> &'static str {
    "scss/comment-no-empty"
  }

  fn description(&self) -> &'static str {
    "Disallow empty comments (SCSS-aware)"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Comment(comment) = node else {
      return vec![];
    };

    if comment.is_line {
      // Flag empty line comments: `//` with nothing (or only whitespace) after.
      // Raffia stores text after `//` in comment.text (without `//` prefix).
      let content = if comment.text.starts_with("//") {
        &comment.text[2..]
      } else {
        &comment.text
      };
      return if content.trim().is_empty() {
        vec![
          Diagnostic::new(self.name(), "Unexpected empty comment")
            .severity(self.default_severity())
            .span(Span::new(comment.span.offset, comment.span.length)),
        ]
      } else {
        vec![]
      };
    }

    let inner = comment.text.trim_start_matches("/*").trim_end_matches("*/");
    if inner.trim().is_empty() {
      vec![
        Diagnostic::new(self.name(), "Unexpected empty comment")
          .severity(self.default_severity())
          .span(Span::new(comment.span.offset, comment.span.length)),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Comment, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn css_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn skips_non_scss() {
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: ParserSpan::new(0, 5),
    });
    assert!(ScssCommentNoEmpty.check(&node, &css_ctx()).is_empty());
  }

  #[test]
  fn reports_empty_block_comment() {
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: ParserSpan::new(0, 5),
    });
    let d = ScssCommentNoEmpty.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Unexpected empty comment");
  }

  #[test]
  fn reports_empty_line_comments() {
    // A `//` with no content after it is an empty comment and should be flagged.
    let node = CssNode::Comment(Comment {
      is_line: true,
      text: "".to_string(),
      span: ParserSpan::new(0, 2),
    });
    let d = ScssCommentNoEmpty.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_empty_line_comments() {
    let node = CssNode::Comment(Comment {
      is_line: true,
      text: " some text".to_string(),
      span: ParserSpan::new(0, 12),
    });
    assert!(ScssCommentNoEmpty.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn allows_non_empty_block_comment() {
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* hello */".to_string(),
      span: ParserSpan::new(0, 11),
    });
    assert!(ScssCommentNoEmpty.check(&node, &scss_ctx()).is_empty());
  }
}
