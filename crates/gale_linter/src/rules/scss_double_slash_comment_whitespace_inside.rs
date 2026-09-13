use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

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
pub struct ScssDoubleSlashCommentWhitespaceInside;

impl Rule for ScssDoubleSlashCommentWhitespaceInside {
  fn name(&self) -> &'static str {
    "scss/double-slash-comment-whitespace-inside"
  }

  fn description(&self) -> &'static str {
    "Require or disallow whitespace after // in line comments"
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

    let option = ctx.primary_option_str().unwrap_or("always");

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
            Diagnostic::new(self.name(), "Expected a space after //".to_string())
              .severity(self.default_severity())
              .span(Span::new(after_slashes, 1)),
          ]
        } else {
          vec![]
        }
      }
      "never" => {
        if starts_with_space {
          vec![
            Diagnostic::new(self.name(), "Unexpected whitespace after //".to_string())
              .severity(self.default_severity())
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
  use gale_css_parser::{Comment, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn line_comment(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      is_line: true,
      text: text.to_string(),
      span: ParserSpan::new(0, text.len()),
    })
  }

  fn block_comment(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      is_line: false,
      text: text.to_string(),
      span: ParserSpan::new(0, text.len()),
    })
  }

  #[test]
  fn reports_missing_space_always() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//comment"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn allows_space_always() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("// comment"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_empty_comment() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_block_comments() {
    let d =
      ScssDoubleSlashCommentWhitespaceInside.check(&block_comment("/*comment*/"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_triple_slash_sassdoc() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("/// @param"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_triple_slash_bare() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("///"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_bang_comment() {
    let d =
      ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//! important"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_equals() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//========"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_dashes() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//--------"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_stars() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//****"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_separator_hashes() {
    let d = ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//####"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn still_reports_regular_no_space() {
    let d =
      ScssDoubleSlashCommentWhitespaceInside.check(&line_comment("//TODO fix this"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn skips_non_scss() {
    let css_ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssDoubleSlashCommentWhitespaceInside
        .check(&line_comment("//comment"), &css_ctx)
        .is_empty()
    );
  }
}
