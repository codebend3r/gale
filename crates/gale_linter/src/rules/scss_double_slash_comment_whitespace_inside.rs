use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
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
  /// option.  Only comment nodes are checked: a `//` comment inside a
  /// selector, value or params lives in that node's raws, where
  /// stylelint-scss's `walkComments` does not see it either.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for comment in tree.walk_comments() {
      // Only `//` comments, and not the empty ones (`//` with nothing but
      // whitespace after it), which stylelint-scss skips.
      if !comment.is_inline() || comment.text().is_empty() {
        continue;
      }
      // The text after `//`, as written.
      let content = comment.source_text().get(2..).unwrap_or("");

      // Triple-slash comments (`///`) are SassDoc and should be ignored.
      // `//!` comments are special directive comments and should be ignored.
      // Comment separators like `//====`, `//----`, `//****`, `//####`, etc.
      // If the content after `//` consists entirely of a single repeated
      // non-alphanumeric, non-whitespace character, treat it as a separator.
      if content.starts_with('/') || content.starts_with('!') || is_separator_comment(content) {
        continue;
      }

      let starts_with_space = content.starts_with(' ') || content.starts_with('\t');

      // Stylelint points to the character right after `//`, not to `//` itself.
      let after_slashes = comment.start + 2;
      let message = match self.expectation {
        "always" if !starts_with_space => "Expected a space after //",
        "never" if starts_with_space => "Unexpected whitespace after //",
        _ => continue,
      };
      diags.push(Diagnostic::new(NAME, message).span(Span::new(after_slashes, 1)));
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  fn always(source: &str) -> Vec<Diagnostic> {
    lint(NAME, json!("always"), source, Syntax::Scss)
  }

  #[test]
  fn reports_missing_space_always() {
    let d = always("a {}\n//comment");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
    assert_eq!((d[0].span.offset, d[0].span.length), (7, 1));
    assert_eq!(always("a {\n  //TODO fix this\n}").len(), 1);
  }

  #[test]
  fn reports_space_never() {
    let d = lint(NAME, json!("never"), "// comment\n//comment", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Unexpected whitespace after //");
    assert_eq!(d[0].span.offset, 2);
  }

  #[test]
  fn allows_space_and_empty_comments() {
    assert!(always("// comment\n//\n//   \r\n").is_empty());
  }

  #[test]
  fn skips_block_comments() {
    assert!(always("/*comment*/").is_empty());
  }

  #[test]
  fn allows_sassdoc_bang_and_separator_comments() {
    for comment in [
      "/// @param",
      "///",
      "//! important",
      "//========",
      "//--------",
      "//****",
      "//####",
    ] {
      assert!(always(comment).is_empty(), "{comment}");
    }
  }

  #[test]
  fn ignores_comments_inside_selectors_and_values() {
    assert!(always("a, //x\nb { c: d //y\n; }").is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(lint(NAME, json!("always"), "//comment\na {}", Syntax::Less).is_empty());
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
