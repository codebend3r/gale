use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports empty CSS comments (`/* */` or `/**/`).
///
/// Equivalent to Stylelint's `comment-no-empty` rule.
pub struct CommentNoEmpty;

impl Rule for CommentNoEmpty {
  fn name(&self) -> &'static str {
    "comment-no-empty"
  }

  fn description(&self) -> &'static str {
    "Disallow empty comments"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    match node {
      CssNode::Comment(comment) => {
        // In SCSS/Less/Sass, skip double-slash comments (`//`).
        // `//` and `// ` are commonly used as visual separators and
        // should not be flagged. The SCSS-specific rule
        // `scss/comment-no-empty` handles them if needed.
        if comment.is_line {
          return vec![];
        }

        // The text field contains the full comment including delimiters.
        // Strip `/*` and `*/` and check if the content is empty/whitespace.
        let inner = comment.text.trim_start_matches("/*").trim_end_matches("*/");
        if inner.trim().is_empty() {
          // Build a fix that removes the empty comment.
          // Also consume trailing whitespace/newline so we don't leave blank lines.
          let start = comment.span.offset;
          let mut end = start + comment.span.length;
          let src = context.source.as_bytes();
          // Consume trailing whitespace and one newline.
          while end < src.len() && (src[end] == b' ' || src[end] == b'\t') {
            end += 1;
          }
          if end < src.len() && src[end] == b'\n' {
            end += 1;
          } else if end < src.len() && src[end] == b'\r' {
            end += 1;
            if end < src.len() && src[end] == b'\n' {
              end += 1;
            }
          }

          let fix = Fix::new(
            "Remove empty comment",
            vec![Edit::new(Span::from_range(start, end), "")],
          );

          vec![
            Diagnostic::new(self.name(), "Unexpected empty comment")
              .severity(self.default_severity())
              .span(Span::new(comment.span.offset, comment.span.length))
              .fix(fix),
          ]
        } else {
          vec![]
        }
      }
      _ => vec![],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Comment, Span as ParserSpan, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_empty_comment() {
    let rule = CommentNoEmpty;
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: ParserSpan::new(0, 5),
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected empty comment");
  }

  #[test]
  fn reports_minimal_empty_comment() {
    let rule = CommentNoEmpty;
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/**/".to_string(),
      span: ParserSpan::new(0, 4),
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn ignores_non_empty_comment() {
    let rule = CommentNoEmpty;
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* hello */".to_string(),
      span: ParserSpan::new(0, 11),
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn skips_scss_double_slash_comments() {
    let rule = CommentNoEmpty;
    let ctx = RuleContext {
      file_path: "test.scss",
      source: "//\n// \n",
      syntax: Syntax::Scss,
      options: None,
    };
    // Empty double-slash comment
    let node = CssNode::Comment(gale_css_parser::Comment {
      is_line: true,
      text: "".to_string(),
      span: ParserSpan::new(0, 2),
    });
    let diags = rule.check(&node, &ctx);
    assert!(diags.is_empty());

    // Double-slash comment with only a space
    let node2 = CssNode::Comment(gale_css_parser::Comment {
      is_line: true,
      text: " ".to_string(),
      span: ParserSpan::new(3, 3),
    });
    let diags2 = rule.check(&node2, &ctx);
    assert!(diags2.is_empty());
  }

  #[test]
  fn fix_removes_empty_comment() {
    let source = "/* */\na { color: red; }";
    let rule = CommentNoEmpty;
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: ParserSpan::new(0, 5),
    });
    let diags = rule.check(&node, &ctx);
    assert_eq!(diags.len(), 1);
    let fix = diags[0].fix.as_ref().expect("should have a fix");
    assert_eq!(fix.edits.len(), 1);
    // Should remove "/* */\n" (6 bytes)
    assert_eq!(fix.edits[0].span.offset, 0);
    assert_eq!(fix.edits[0].span.length, 6);
    assert_eq!(fix.edits[0].new_text, "");
  }
}
