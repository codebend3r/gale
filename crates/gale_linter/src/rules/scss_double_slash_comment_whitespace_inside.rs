use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

/// Returns `true` if `content` (the text after `//`) looks like a decorative
/// separator — i.e. it consists entirely of a single repeated non-alphanumeric,
/// non-whitespace character (e.g. `====`, `----`, `****`, `####`).
fn is_separator_comment(content: &str) -> bool {
  let mut chars = content.chars();
  let first = match chars.next() {
    Some(c) => c,
    None => return false,
  };
  // The repeated character must not be alphanumeric or whitespace.
  if first.is_alphanumeric() || first.is_whitespace() {
    return false;
  }
  chars.all(|c| c == first)
}

const NAME: &str = "scss/double-slash-comment-whitespace-inside";

declare_rule! {
  /// Require or disallow whitespace after `//` in SCSS line comments.
  ///
  /// By default expects `"always"` — a space after `//`:
  ///
  /// ```scss
  /// // Good
  /// // comment text
  ///
  /// // Bad
  /// //comment text
  /// ```
  ///
  /// Equivalent to `scss/double-slash-comment-whitespace-inside`.
  pub struct ScssDoubleSlashCommentWhitespaceInside => ScssDoubleSlashCommentWhitespaceInsideCheck {
    name: NAME,
    description: "Require or disallow whitespace after // in line comments",
    default_severity: Warning,
  }
}

/// `scss/double-slash-comment-whitespace-inside` as configured.
pub struct ScssDoubleSlashCommentWhitespaceInsideCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssDoubleSlashCommentWhitespaceInsideCheck {
  /// Stylelint's options: `"always"` or `"never"`, `"always"` when not given.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always", "never"])
      .unwrap_or("always");
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssDoubleSlashCommentWhitespaceInsideCheck {
  /// Flags a `//` comment whose whitespace after the slashes does not match the
  /// option.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Comment(comment) = node else {
      return vec![];
    };

    // Only applies to line comments (`//`)
    if !comment.is_line {
      return vec![];
    }

    // Skip `//` comments embedded inside selector lists (preceding non-empty
    // line ends with `,`). postcss-scss doesn't expose these as comment nodes
    // so Stylelint never checks them.
    if comment.span.offset > 0 {
      let before = &ctx.source[..comment.span.offset];
      // Find the previous non-empty line
      if let Some(prev_line) = before.lines().rev().find(|l| !l.trim().is_empty()) {
        if prev_line.trim_end().ends_with(',') {
          return vec![];
        }
      }
    }

    let option = self.expectation;

    // The comment text should be the content after `//`.
    // Some parsers include the `//` prefix, some don't.
    let content = if comment.text.starts_with("//") {
      &comment.text[2..]
    } else {
      &comment.text
    };

    // Empty comments (`//` with nothing after) are always allowed.
    if content.is_empty() {
      return vec![];
    }

    // Triple-slash comments (`///`) are SassDoc and should be ignored.
    if content.starts_with('/') {
      return vec![];
    }

    // `//!` comments are special directive comments and should be ignored.
    if content.starts_with('!') {
      return vec![];
    }

    // Comment separators like `//====`, `//----`, `//****`, `//####`, etc.
    // If the content after `//` consists entirely of a single repeated
    // non-alphanumeric, non-whitespace character, treat it as a separator.
    if is_separator_comment(content) {
      return vec![];
    }

    let starts_with_space = content.starts_with(' ') || content.starts_with('\t');

    // Stylelint points to the character right after `//`, not to `//` itself.
    // comment.span.offset always points to the `//` in source (raffia stores
    // text content without `//`, but span.start still includes the `//`).
    let after_slashes = comment.span.offset + 2;

    match option {
      "always" => {
        if !starts_with_space {
          vec![
            Diagnostic::new(NAME, "Expected a space after //".to_string())
              .span(Span::new(after_slashes, 1)),
          ]
        } else {
          vec![]
        }
      }
      "never" => {
        if starts_with_space {
          vec![
            Diagnostic::new(NAME, "Unexpected whitespace after //".to_string())
              .span(Span::new(after_slashes, 1)),
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
  use gale_css_parser::Comment;

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn line_comment(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      is_line: true,
      text: text.to_string(),
      span: Span::new(0, text.len()),
    })
  }

  fn block_comment(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      is_line: false,
      text: text.to_string(),
      span: Span::new(0, text.len()),
    })
  }

  #[test]
  fn reports_missing_space_always() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//comment"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn allows_space_always() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("// comment"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_empty_comment() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_block_comments() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&block_comment("/*comment*/"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_triple_slash_sassdoc() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("/// @param"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_triple_slash_bare() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("///"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_bang_comment() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//! important"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_equals() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//========"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_dashes() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//--------"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_stars() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//****"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_hashes() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//####"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn still_reports_regular_no_space() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let d = rule.check(&line_comment("//TODO fix this"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssDoubleSlashCommentWhitespaceInside, None);
    let css_ctx = ctx();
    assert!(rule.check(&line_comment("//comment"), &css_ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      [
        "Invalid option value \"sideways\" for rule \"scss/double-slash-comment-whitespace-inside\""
      ]
    );
  }
}
