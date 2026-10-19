use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::is_block_single_line;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/block-closing-brace-newline-after";

declare_rule! {
  /// Require or disallow a newline after the closing brace of blocks.
  ///
  /// Equivalent to Stylelint's `@stylistic/block-closing-brace-newline-after` rule.
  pub struct StylisticBlockClosingBraceNewlineAfter => StylisticBlockClosingBraceNewlineAfterCheck {
    name: NAME,
    description: "Require or disallow a newline after the closing brace of blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/block-closing-brace-newline-after` as configured.
pub struct StylisticBlockClosingBraceNewlineAfterCheck {
  /// The primary option: `"always"`, `"always-single-line"`,
  /// `"never-single-line"`, `"always-multi-line"`, `"never-multi-line"` or
  /// gale's own `"never"`.
  option: &'static str,
}

impl StylisticBlockClosingBraceNewlineAfterCheck {
  /// The plugin's options: `"always"`, `"always-single-line"`,
  /// `"never-single-line"`, `"always-multi-line"` or `"never-multi-line"`.  Gale
  /// also accepts `"never"`, which the plugin does not.  Gale reads no option as
  /// `"always"`.  `"never-single-line"`, `"never-multi-line"` and
  /// `ignoreAtRules` are accepted but not yet applied.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&[
      "always",
      "always-single-line",
      "never-single-line",
      "always-multi-line",
      "never-multi-line",
      "never",
    ]);
    options.strings("ignoreAtRules");
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticBlockClosingBraceNewlineAfterCheck {
  /// Flags a closing brace whose trailing newline does not match the option.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;

    while i < len {
      // Skip strings
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        i += 1;
        continue;
      }

      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
        continue;
      }

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip SCSS interpolation #{...}
      if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      if bytes[i] == b'}' {
        let brace_pos = i;
        i += 1;

        // Skip whitespace after the closing brace (but not newlines yet for checking)
        let after_pos = i;

        // Check if there's anything after the brace (not EOF)
        if after_pos >= len {
          // EOF after brace is fine
          continue;
        }

        // Skip `};` — a common SCSS pattern (e.g., end of @function body
        // or SCSS map assignment). Stylelint does not flag these.
        if bytes[after_pos] == b';' {
          continue;
        }

        // Determine if there's a newline immediately after (allowing spaces/tabs before it)
        let mut j = after_pos;
        let mut found_newline = false;
        let mut found_non_ws = false;
        while j < len {
          if bytes[j] == b'\n' || bytes[j] == b'\r' {
            found_newline = true;
            break;
          } else if bytes[j] == b' ' || bytes[j] == b'\t' {
            j += 1;
          } else {
            found_non_ws = true;
            break;
          }
        }

        // Determine if the block is single-line by looking backward for the opening brace
        let is_single_line = is_block_single_line(source, brace_pos);

        // Check if the next non-whitespace content after } is @else or @else if
        // which is valid SCSS: `} @else {` or `} @else if ... {`
        let is_followed_by_else = is_next_else(bytes, after_pos);

        match option {
          "always" => {
            // Expect newline after every closing brace (unless followed by another }, EOF, or @else)
            if found_non_ws && !is_next_closing_brace(bytes, after_pos) && !is_followed_by_else {
              diagnostics.push(
                Diagnostic::new(NAME, "Expected newline after \"}\"".to_string())
                  .span(Span::new(brace_pos, 1)),
              );
            }
          }
          "never" => {
            if found_newline {
              diagnostics.push(
                Diagnostic::new(NAME, "Unexpected newline after \"}\"".to_string())
                  .span(Span::new(brace_pos, 1)),
              );
            }
          }
          "always-single-line" => {
            if is_single_line
              && found_non_ws
              && !is_next_closing_brace(bytes, after_pos)
              && !is_followed_by_else
            {
              diagnostics.push(
                Diagnostic::new(
                  NAME,
                  "Expected newline after \"}\" of a single-line block".to_string(),
                )
                .span(Span::new(brace_pos, 1)),
              );
            }
          }
          "always-multi-line" => {
            if !is_single_line
              && found_non_ws
              && !is_next_closing_brace(bytes, after_pos)
              && !is_followed_by_else
            {
              diagnostics.push(
                Diagnostic::new(
                  NAME,
                  "Expected newline after \"}\" of a multi-line block".to_string(),
                )
                .span(Span::new(brace_pos, 1)),
              );
            }
          }
          _ => {}
        }
      } else {
        i += 1;
      }
    }

    diagnostics
  }
}

/// Check if the next non-whitespace content after a position is `@else` (SCSS).
fn is_next_else(bytes: &[u8], pos: usize) -> bool {
  let mut j = pos;
  while j < bytes.len()
    && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r')
  {
    j += 1;
  }
  // Check for @else
  if j + 4 < bytes.len()
    && bytes[j] == b'@'
    && bytes[j + 1] == b'e'
    && bytes[j + 2] == b'l'
    && bytes[j + 3] == b's'
    && bytes[j + 4] == b'e'
  {
    // Make sure it's `@else` followed by space, `{`, or end (not `@elsewhere`)
    if j + 5 >= bytes.len()
      || bytes[j + 5] == b' '
      || bytes[j + 5] == b'\t'
      || bytes[j + 5] == b'{'
      || bytes[j + 5] == b'\n'
    {
      return true;
    }
  }
  false
}

/// Whether the next non-whitespace byte from `pos` is `}`.
fn is_next_closing_brace(bytes: &[u8], pos: usize) -> bool {
  let mut j = pos;
  while j < bytes.len() {
    if bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r' {
      j += 1;
    } else {
      return bytes[j] == b'}';
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_newline_after_brace() {
    let rule = configured(&StylisticBlockClosingBraceNewlineAfter, None);
    let source = "a { color: red; }\nb { color: blue; }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_missing_newline_after_brace() {
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticBlockClosingBraceNewlineAfter, opt.clone());
    let source = "a { color: red; } b { color: blue; }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn never_reports_newline_after_brace() {
    let opt = serde_json::Value::String("never".to_string());
    let rule = configured(&StylisticBlockClosingBraceNewlineAfter, opt.clone());
    let source = "a { color: red; }\nb { }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected newline"));
  }

  #[test]
  fn allows_else_after_closing_brace() {
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticBlockClosingBraceNewlineAfter, opt.clone());
    let source = "@if $cond { color: red; } @else { color: blue; }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    // The first } before @else should NOT be flagged
    assert!(
      d.is_empty(),
      "Should not flag }} before @else, got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn allows_else_if_after_closing_brace() {
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticBlockClosingBraceNewlineAfter, opt.clone());
    let source = "@if $a { color: red; } @else if $b { color: blue; } @else { color: green; }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(
      d.is_empty(),
      "Should not flag }} before @else if, got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn allows_semicolon_after_closing_brace() {
    // SCSS pattern: `@function foo() { ... };` or map closings
    let opt = serde_json::Value::String("always".to_string());
    let rule = configured(&StylisticBlockClosingBraceNewlineAfter, opt.clone());
    let source = "@function foo() {\n  @return 1;\n};\n";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(
      d.is_empty(),
      "Should not flag }}; pattern, got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never-multi-line", { "ignoreAtRules": ["if", "else"] }])
      )
      .is_empty()
    );
    // `"never"` is gale's own value; the plugin rejects it.
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/block-closing-brace-newline-after\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["always", { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"@stylistic/block-closing-brace-newline-after\""]
    );
  }
}
