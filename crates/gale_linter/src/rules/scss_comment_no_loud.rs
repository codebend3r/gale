use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/comment-no-loud";

declare_rule! {
  /// Disallow loud comments (`/* ... */`) in SCSS files.
  ///
  /// SCSS files should use silent comments (`//`) instead of CSS block comments.
  /// This rule reports all block comments in SCSS/Sass files.
  ///
  /// Equivalent to `scss/comment-no-loud`.
  pub struct ScssCommentNoLoud => ScssCommentNoLoudCheck {
    name: NAME,
    description: "Disallow loud comments (/* ... */) in SCSS files",
    default_severity: Warning,
  }
}

/// `scss/comment-no-loud`, which takes no options.
pub struct ScssCommentNoLoudCheck;

impl ScssCommentNoLoudCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssCommentNoLoudCheck {
  /// Flags `/* … */` comments in SCSS, where `//` is preferred.
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
        NAME,
        "Unexpected loud comment (/* ... */). Use // instead in SCSS",
      )
      .span(comment.span),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Comment;
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  #[test]
  fn reports_block_comment_in_scss() {
    let rule = configured(&ScssCommentNoLoud, None);
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* some comment */".to_string(),
      span: Span::new(0, 18),
    });
    let d = rule.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("loud comment"));
  }

  #[test]
  fn allows_line_comment_in_scss() {
    let rule = configured(&ScssCommentNoLoud, None);
    let node = CssNode::Comment(Comment {
      is_line: true,
      text: "// some comment".to_string(),
      span: Span::new(0, 15),
    });
    assert!(rule.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssCommentNoLoud, None);
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* some comment */".to_string(),
      span: Span::new(0, 18),
    });
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn reports_empty_block_comment_in_scss() {
    let rule = configured(&ScssCommentNoLoud, None);
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* */".to_string(),
      span: Span::new(0, 5),
    });
    let d = rule.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_multiline_block_comment() {
    let rule = configured(&ScssCommentNoLoud, None);
    let node = CssNode::Comment(Comment {
      is_line: false,
      text: "/* multi\n   line */".to_string(),
      span: Span::new(0, 18),
    });
    let d = rule.check(&node, &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/comment-no-loud\""]
    );
  }
}
