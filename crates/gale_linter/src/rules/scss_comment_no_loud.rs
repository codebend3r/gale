use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow loud comments (`/* ... */`) in SCSS files.
///
/// SCSS files should use silent comments (`//`) instead of CSS block comments.
/// This rule reports all block comments in SCSS/Sass files.
///
/// Equivalent to `scss/comment-no-loud`.
pub struct ScssCommentNoLoud;

impl Rule for ScssCommentNoLoud {
  fn name(&self) -> &'static str {
    "scss/comment-no-loud"
  }

  fn description(&self) -> &'static str {
    "Disallow loud comments (/* ... */) in SCSS files"
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

    // Line comments (//) are fine — only block comments (/* */) are "loud"
    if comment.is_line {
      return vec![];
    }

    // Skip stylelint/gale disable/enable control comments — these are not
    // regular loud comments but tooling directives that must use /* */ syntax.
    let trimmed = comment.text.trim();
    // Strip the leading /* and trailing */ if present, then check the inner text
    let inner = trimmed
      .strip_prefix("/*")
      .unwrap_or(trimmed)
      .strip_suffix("*/")
      .unwrap_or(trimmed)
      .trim();
    if inner.starts_with("stylelint-disable")
      || inner.starts_with("stylelint-enable")
      || inner.starts_with("gale-disable")
      || inner.starts_with("gale-enable")
    {
      return vec![];
    }

    vec![
      Diagnostic::new(
        self.name(),
        "Unexpected loud comment (/* ... */). Use // instead in SCSS",
      )
      .severity(self.default_severity())
      .span(Span::new(comment.span.offset, comment.span.length)),
    ]
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
  fn reports_block_comment_in_scss() {
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* some comment */".to_string(),
      span: ParserSpan::new(0, 18),
    });
    let d = ScssCommentNoLoud.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("loud comment"));
  }

  #[test]
  fn allows_line_comment_in_scss() {
    let node = CssNode::Comment(Comment {
      is_line: true,
      text: "// some comment".to_string(),
      span: ParserSpan::new(0, 15),
    });
    assert!(ScssCommentNoLoud.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* some comment */".to_string(),
      span: ParserSpan::new(0, 18),
    });
    assert!(ScssCommentNoLoud.check(&node, &css_ctx()).is_empty());
  }

  #[test]
  fn reports_empty_block_comment_in_scss() {
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: ParserSpan::new(0, 5),
    });
    let d = ScssCommentNoLoud.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_multiline_block_comment() {
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* multi\n   line */".to_string(),
      span: ParserSpan::new(0, 18),
    });
    let d = ScssCommentNoLoud.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
  }
}
