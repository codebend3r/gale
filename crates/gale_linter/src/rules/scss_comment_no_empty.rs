use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/comment-no-empty";

declare_rule! {
  /// SCSS-specific `comment-no-empty`.
  ///
  /// Disallows empty block comments (`/* */`) but exempts SCSS line comments
  /// (`//`). Only active in SCSS/Sass files.
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
          Diagnostic::new(NAME, "Unexpected empty comment")
            .span(Span::new(comment.span.offset, comment.span.length)),
        ]
      } else {
        vec![]
      };
    }

    let inner = comment.text.trim_start_matches("/*").trim_end_matches("*/");
    if inner.trim().is_empty() {
      vec![
        Diagnostic::new(NAME, "Unexpected empty comment")
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
  use gale_css_parser::{Comment, Span as ParserSpan};

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssCommentNoEmpty, None);
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: ParserSpan::new(0, 5),
    });
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn reports_empty_block_comment() {
    let rule = configured(&ScssCommentNoEmpty, None);
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: ParserSpan::new(0, 5),
    });
    let d = rule.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Unexpected empty comment");
  }

  #[test]
  fn reports_empty_line_comments() {
    let rule = configured(&ScssCommentNoEmpty, None);
    // A `//` with no content after it is an empty comment and should be flagged.
    let node = CssNode::Comment(Comment {
      is_line: true,
      text: "".to_string(),
      span: ParserSpan::new(0, 2),
    });
    let d = rule.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_empty_line_comments() {
    let rule = configured(&ScssCommentNoEmpty, None);
    let node = CssNode::Comment(Comment {
      is_line: true,
      text: " some text".to_string(),
      span: ParserSpan::new(0, 12),
    });
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn allows_non_empty_block_comment() {
    let rule = configured(&ScssCommentNoEmpty, None);
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* hello */".to_string(),
      span: ParserSpan::new(0, 11),
    });
    assert!(rule.check(&node, &scss_ctx()).is_empty());
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
