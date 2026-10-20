use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-invalid-double-slash-comments";

declare_rule! {
  /// Reports `//` comments in CSS files (valid in SCSS but not CSS).
  ///
  /// Equivalent to Stylelint's `no-invalid-double-slash-comments` rule.
  pub struct NoInvalidDoubleSlashComments => NoInvalidDoubleSlashCommentsCheck {
    name: NAME,
    description: "Disallow double-slash comments (//...) which are not supported in CSS",
    default_severity: Warning,
  }
}

/// `no-invalid-double-slash-comments`, which takes no options.
pub struct NoInvalidDoubleSlashCommentsCheck;

impl NoInvalidDoubleSlashCommentsCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for NoInvalidDoubleSlashCommentsCheck {
  /// Flags `//` comments, which are invalid in plain CSS. String literals are
  /// skipped so URLs are not mistaken for comments.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    // Only flag in plain CSS mode.
    if context.syntax != Syntax::Css {
      return vec![];
    }

    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;

    while i < len {
      let b = bytes[i];

      // Skip string literals.
      if b == b'"' || b == b'\'' {
        let quote = b;
        i += 1;
        while i < len {
          if bytes[i] == b'\\' {
            i += 2; // skip escaped character
            continue;
          }
          if bytes[i] == quote {
            i += 1;
            break;
          }
          i += 1;
        }
        continue;
      }

      // Check for slash.
      if b == b'/' && i + 1 < len {
        if bytes[i + 1] == b'*' {
          // Block comment — skip to closing `*/`.
          i += 2;
          while i + 1 < len {
            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
              i += 2;
              break;
            }
            i += 1;
          }
          continue;
        }

        if bytes[i + 1] == b'/' {
          // Double-slash comment found.
          let start = i;
          // Find end of line.
          let end = source[i..].find('\n').map(|pos| i + pos).unwrap_or(len);
          diagnostics.push(
            Diagnostic::new(NAME, "Unexpected double-slash CSS comment")
              .span(Span::new(start, end - start)),
          );
          i = end;
          continue;
        }
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, scss_ctx_with_source};

  #[test]
  fn reports_double_slash_comment_in_css() {
    let rule = configured(&NoInvalidDoubleSlashComments, None);
    let context = ctx_with_source("a { color: red; } // bad comment");
    let diags = rule.check_root(&[], &context);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected double-slash CSS comment");
  }

  #[test]
  fn ignores_double_slash_in_scss() {
    let rule = configured(&NoInvalidDoubleSlashComments, None);
    let context = scss_ctx_with_source("// this is fine in SCSS");
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_block_comments() {
    let rule = configured(&NoInvalidDoubleSlashComments, None);
    let context = ctx_with_source("/* this is fine */ a { color: red; }");
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_double_slash_inside_string() {
    let rule = configured(&NoInvalidDoubleSlashComments, None);
    let context = ctx_with_source("a { content: \"//not-a-comment\"; }");
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_double_slash_inside_block_comment() {
    let rule = configured(&NoInvalidDoubleSlashComments, None);
    let context = ctx_with_source("/* // inside block */ a { color: red; }");
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options("no-invalid-double-slash-comments", serde_json::json!(true)).is_empty()
    );
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "no-invalid-double-slash-comments",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        "no-invalid-double-slash-comments",
        serde_json::json!("always")
      ),
      ["Unexpected option value \"always\" for rule \"no-invalid-double-slash-comments\""]
    );
  }
}
