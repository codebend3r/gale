use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "comment-no-empty";

declare_rule! {
  /// Reports empty CSS comments (`/* */` or `/**/`).
  ///
  /// Equivalent to Stylelint's `comment-no-empty` rule.
  pub struct CommentNoEmpty => CommentNoEmptyCheck {
    name: NAME,
    description: "Disallow empty comments",
    default_severity: Warning,
  }
}

/// `comment-no-empty`, which takes no options.
pub struct CommentNoEmptyCheck;

impl CommentNoEmptyCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for CommentNoEmptyCheck {
  /// Flags block comments with no content, fixing by deleting the comment and
  /// its trailing newline.  SCSS/Less `//` comments are left alone, as
  /// Stylelint's `isStandardSyntaxComment` leaves them: `//` and `// ` are
  /// common visual separators, and `scss/comment-no-empty` covers them.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for comment in tree.walk_comments() {
      if comment.is_inline() || !comment.text().is_empty() {
        continue;
      }
      let span = comment.span();
      // Also consume trailing whitespace and one newline so the fix does not
      // leave a blank line behind.
      let start = span.offset;
      let mut end = span.end();
      let src = context.source.as_bytes();
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
      diags.push(
        Diagnostic::new(NAME, "Unexpected empty comment")
          .span(span)
          .fix(fix),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "comment-no-empty";

  #[test]
  fn reports_empty_comments() {
    for css in ["/* */", "/**/", "a { color: red; /*\n */ }"] {
      let d = lint(RULE, json!(true), css, Syntax::Css);
      assert_eq!(d.len(), 1, "{css}");
      assert_eq!(d[0].message, "Unexpected empty comment");
    }
    let d = lint(RULE, json!(true), "a {}\n/* */", Syntax::Css);
    assert_eq!((d[0].span.offset, d[0].span.length), (5, 5));
  }

  #[test]
  fn ignores_non_empty_comments() {
    assert!(lint(RULE, json!(true), "/* hello */", Syntax::Css).is_empty());
  }

  #[test]
  fn skips_double_slash_comments() {
    assert!(lint(RULE, json!(true), "//\n// \n", Syntax::Scss).is_empty());
    assert!(lint(RULE, json!(true), "//\na {}", Syntax::Less).is_empty());
    assert_eq!(
      lint(RULE, json!(true), "// x\n/* */", Syntax::Scss).len(),
      1
    );
  }

  #[test]
  fn ignores_comments_inside_values() {
    // PostCSS keeps these in the declaration's raws, not as comment nodes.
    let scss = "a { margin: 0 /* */ 1px; }";
    assert!(lint(RULE, json!(true), scss, Syntax::Scss).is_empty());
  }

  #[test]
  fn fix_removes_empty_comment_and_its_newline() {
    assert_eq!(
      fix(RULE, json!(true), "/* */\na { color: red; }", Syntax::Css),
      "a { color: red; }"
    );
  }
}
