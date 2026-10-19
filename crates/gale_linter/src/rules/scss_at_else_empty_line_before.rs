use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-else-empty-line-before";

declare_rule! {
  /// Disallow or require an empty line before `@else`.
  ///
  /// Primary option: `"never"` (default) or `"always"`.
  ///
  /// Equivalent to `scss/at-else-empty-line-before`.
  pub struct ScssAtElseEmptyLineBefore => ScssAtElseEmptyLineBeforeCheck {
    name: NAME,
    description: "Disallow or require an empty line before @else",
    default_severity: Warning,
  }
}

/// `scss/at-else-empty-line-before` as configured.
pub struct ScssAtElseEmptyLineBeforeCheck {
  /// The primary option: `"never"` or `"always"`.
  expectation: &'static str,
}

impl ScssAtElseEmptyLineBeforeCheck {
  /// Stylelint's options: `"never"` (also when not given).  Gale also takes
  /// the `"always"` it has always implemented, which upstream does not.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["never", "always"])
      .unwrap_or("never");
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssAtElseEmptyLineBeforeCheck {
  /// Flags an `@else` whose preceding blank line does not match the option.
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

      // Skip line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Look for `@else`
      if bytes[i] == b'@' && source[i..].starts_with("@else") {
        let at_else_offset = i;

        // Check if there's an empty line before this @else
        let has_empty_line = has_empty_line_before(source, at_else_offset);

        match option {
          "never" => {
            if has_empty_line {
              diagnostics.push(
                Diagnostic::new(NAME, "Unexpected empty line before @else")
                  .span(Span::new(at_else_offset, 5)),
              );
            }
          }
          "always" => {
            if !has_empty_line {
              diagnostics.push(
                Diagnostic::new(NAME, "Expected empty line before @else")
                  .span(Span::new(at_else_offset, 5)),
              );
            }
          }
          _ => {}
        }

        i += 5;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

/// Check if there is an empty line (a line containing only whitespace)
/// between the previous content and the position at `offset`.
fn has_empty_line_before(source: &str, offset: usize) -> bool {
  let before = &source[..offset];

  // Walk backwards through newlines. We need to find at least one blank line.
  // A blank line means two consecutive newlines with only whitespace between them.
  let mut pos = before.len();

  // Skip whitespace (spaces/tabs) immediately before @else
  while pos > 0 && matches!(before.as_bytes()[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }

  // Skip the first newline (the line break right before @else's line)
  if pos > 0 && before.as_bytes()[pos - 1] == b'\n' {
    pos -= 1;
    if pos > 0 && before.as_bytes()[pos - 1] == b'\r' {
      pos -= 1;
    }
  } else {
    // @else is on the same line as previous content or at start of file
    return false;
  }

  // Now skip whitespace again
  while pos > 0 && matches!(before.as_bytes()[pos - 1], b' ' | b'\t') {
    pos -= 1;
  }

  // If we hit another newline (or start of file), that means there was an empty line
  if pos == 0 {
    return true;
  }
  before.as_bytes()[pos - 1] == b'\n'
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options, scss_ctx_with_source};

  #[test]
  fn never_allows_no_empty_line() {
    let rule = configured(&ScssAtElseEmptyLineBefore, None);
    let src = "@if $a { color: red; }\n@else { color: blue; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_empty_line() {
    let rule = configured(&ScssAtElseEmptyLineBefore, None);
    let src = "@if $a { color: red; }\n\n@else { color: blue; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected empty line"));
  }

  #[test]
  fn always_allows_empty_line() {
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtElseEmptyLineBefore, opts.clone());
    let src = "@if $a { color: red; }\n\n@else { color: blue; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_empty_line() {
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtElseEmptyLineBefore, opts.clone());
    let src = "@if $a { color: red; }\n@else { color: blue; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected empty line"));
  }

  #[test]
  fn never_same_line_no_report() {
    let rule = configured(&ScssAtElseEmptyLineBefore, None);
    let src = "@if $a { color: red; } @else { color: blue; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtElseEmptyLineBefore, None);
    let ctx = ctx_with_source("@if $a {}\n\n@else {}");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/at-else-empty-line-before\""]
    );
    // Gale keeps the `"always"` it has always implemented.
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
  }
}
