use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/double-slash-comment-inline";

declare_rule! {
  /// Disallow or require double-slash comments to be inline (on the same line as code).
  ///
  /// Primary option: `"never"` (default) disallows inline `//` comments.
  /// `"always"` requires that `//` comments be inline.
  ///
  /// An inline comment is one where there is non-whitespace content before `//`
  /// on the same line.
  ///
  /// Equivalent to `scss/double-slash-comment-inline`.
  pub struct ScssDoubleSlashCommentInline => ScssDoubleSlashCommentInlineCheck {
    name: NAME,
    description: "Disallow or require double-slash comments to be inline",
    default_severity: Warning,
  }
}

/// `scss/double-slash-comment-inline` as configured.
pub struct ScssDoubleSlashCommentInlineCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
  /// `ignore: ["stylelint-commands"]`: comments starting `stylelint-` are
  /// left alone.
  ignore_stylelint_commands: bool,
}

impl ScssDoubleSlashCommentInlineCheck {
  /// Stylelint's options: `"always"` or `"never"` (`"never"` when not
  /// given), then `ignore: ["stylelint-commands"]`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always", "never"])
      .unwrap_or("never");
    let ignore = options.keywords("ignore", &["stylelint-commands"]);
    options.finish()?;
    Ok(Self {
      expectation,
      ignore_stylelint_commands: ignore.contains(&"stylelint-commands"),
    })
  }
}

impl Check for ScssDoubleSlashCommentInlineCheck {
  /// Flags a `//` comment whose inline placement does not match the option; a
  /// comment counts as inline when code precedes it on the same line.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.expectation;
    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();

    let mut i = 0;
    while i < len {
      // Skip string literals
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < len {
          if bytes[i] == b'\\' {
            i += 2;
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

      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
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

      // Found a `//` comment
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        let comment_start = i;

        // Determine if this is inline: check if there is non-whitespace
        // content before `//` on the same line
        let is_inline = has_non_whitespace_before_on_line(source, comment_start);

        // Find the end of this comment (end of line)
        let end = source[i..].find('\n').map(|p| i + p).unwrap_or(len);
        let comment_len = end - comment_start;

        // Upstream skips a comment whose text starts `stylelint-` when
        // asked to ignore stylelint commands.
        if self.ignore_stylelint_commands
          && source[comment_start + 2..end]
            .trim_start()
            .starts_with("stylelint-")
        {
          i = end;
          continue;
        }

        match option {
          "never" => {
            if is_inline {
              diagnostics.push(
                Diagnostic::new(NAME, "Unexpected inline double-slash comment")
                  .span(Span::new(comment_start, comment_len)),
              );
            }
          }
          "always" => {
            if !is_inline {
              diagnostics.push(
                Diagnostic::new(NAME, "Expected double-slash comment to be inline")
                  .span(Span::new(comment_start, comment_len)),
              );
            }
          }
          _ => {}
        }

        i = end;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

/// Returns `true` if there is non-whitespace content before `pos` on the same line.
fn has_non_whitespace_before_on_line(source: &str, pos: usize) -> bool {
  let bytes = source.as_bytes();
  let mut j = pos;
  while j > 0 {
    j -= 1;
    match bytes[j] {
      b'\n' | b'\r' => return false,
      b' ' | b'\t' => continue,
      _ => return true,
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    configured, ctx_with_source, invalid_options, scss_ctx_with_source,
    scss_ctx_with_source_and_options,
  };

  #[test]
  fn never_allows_standalone_comment() {
    let rule = configured(&ScssDoubleSlashCommentInline, None);
    let src = "// standalone comment\n.foo { color: red; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_inline_comment() {
    let rule = configured(&ScssDoubleSlashCommentInline, None);
    let src = ".foo { color: red; // inline comment\n}";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected inline"));
  }

  #[test]
  fn always_allows_inline_comment() {
    let opts = serde_json::json!("always");
    let rule = configured(&ScssDoubleSlashCommentInline, opts.clone());
    let src = ".foo { color: red; // inline comment\n}";
    let d = rule.check_root(&[], &scss_ctx_with_source_and_options(src, &opts));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_standalone_comment() {
    let opts = serde_json::json!("always");
    let rule = configured(&ScssDoubleSlashCommentInline, opts.clone());
    let src = "// standalone comment\n.foo { color: red; }";
    let d = rule.check_root(&[], &scss_ctx_with_source_and_options(src, &opts));
    assert_eq!(d.len(), 1);
    assert!(
      d[0]
        .message
        .contains("Expected double-slash comment to be inline")
    );
  }

  #[test]
  fn never_allows_indented_standalone() {
    let rule = configured(&ScssDoubleSlashCommentInline, None);
    let src = ".foo {\n  // indented standalone\n  color: red;\n}";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssDoubleSlashCommentInline, None);
    let ctx = ctx_with_source(".foo { color: red; } // inline");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn skips_double_slash_in_string() {
    let rule = configured(&ScssDoubleSlashCommentInline, None);
    let src = ".foo { content: \"// not a comment\"; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/double-slash-comment-inline\""]
    );
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never", { "ignore": ["stylelint-commands"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["never", { "ignore": ["comments"] }])
      ),
      [
        "Invalid value \"comments\" for option \"ignore\" of rule \"scss/double-slash-comment-inline\""
      ]
    );
  }

  #[test]
  fn ignore_stylelint_commands_skips_disable_comments() {
    let source =
      "a {\n  color: red; // stylelint-disable-line color-named\n  width: 1px; // note\n}\n";
    let lint = |options: serde_json::Value| {
      crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss).len()
    };
    assert_eq!(lint(serde_json::json!("never")), 2);
    assert_eq!(
      lint(serde_json::json!(["never", { "ignore": ["stylelint-commands"] }])),
      1
    );
  }
}
