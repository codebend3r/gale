use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
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
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for comment in tree.walk_comments() {
      // Line comments (//) are fine — only block comments (/* */) are "loud"
      if comment.is_inline() {
        continue;
      }
      // Skip stylelint/gale disable/enable control comments — these are not
      // regular loud comments but tooling directives that must use /* */
      // syntax.
      let inner = comment.text();
      if inner.starts_with("stylelint-disable")
        || inner.starts_with("stylelint-enable")
        || inner.starts_with("gale-disable")
        || inner.starts_with("gale-enable")
      {
        continue;
      }
      diags.push(
        Diagnostic::new(
          NAME,
          "Unexpected loud comment (/* ... */). Use // instead in SCSS",
        )
        .span(comment.span()),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  fn scss(source: &str) -> Vec<Diagnostic> {
    lint(NAME, json!(true), source, Syntax::Scss)
  }

  #[test]
  fn reports_block_comments_in_scss() {
    let d = scss("/* some comment */\na { /* */ }\n/* multi\n   line */");
    assert_eq!(d.len(), 3);
    assert!(d[0].message.contains("loud comment"));
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 18));
  }

  #[test]
  fn allows_line_comments_and_directives() {
    assert!(scss("// some comment\n/* stylelint-disable color-named */").is_empty());
  }

  #[test]
  fn ignores_comments_inside_values() {
    // stylelint-scss walks comment nodes, and PostCSS keeps these in raws.
    assert!(scss("a { width: 1px /* c */; }").is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(lint(NAME, json!(true), "/* some comment */", Syntax::Css).is_empty());
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
