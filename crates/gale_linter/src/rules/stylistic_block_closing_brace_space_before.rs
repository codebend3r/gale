use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/block-closing-brace-space-before";

declare_rule! {
  /// Require or disallow a space before the closing brace of blocks.
  ///
  /// Equivalent to `@stylistic/block-closing-brace-space-before`.
  pub struct StylisticBlockClosingBraceSpaceBefore => StylisticBlockClosingBraceSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the closing brace of blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/block-closing-brace-space-before` as configured.
pub struct StylisticBlockClosingBraceSpaceBeforeCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"`,
  /// `"never-single-line"`, `"always-multi-line"` or `"never-multi-line"`.
  option: &'static str,
}

impl StylisticBlockClosingBraceSpaceBeforeCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"`,
  /// `"never-single-line"`, `"always-multi-line"` or `"never-multi-line"`.  Gale
  /// reads no option as `"always-single-line"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&[
      "always",
      "never",
      "always-single-line",
      "never-single-line",
      "always-multi-line",
      "never-multi-line",
    ]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always-single-line"),
    })
  }
}

impl Check for StylisticBlockClosingBraceSpaceBeforeCheck {
  /// Flags a closing brace whose preceding space does not match the option.
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
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip block comments
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

        // Check the character immediately before the closing brace
        let has_space_before =
          brace_pos > 0 && (bytes[brace_pos - 1] == b' ' || bytes[brace_pos - 1] == b'\t');

        // Check if the block is single-line
        let is_single_line = is_block_single_line(source, brace_pos);

        let violation = match option {
          "always" => !has_space_before,
          "never" => has_space_before,
          "always-single-line" => is_single_line && !has_space_before,
          "never-single-line" => is_single_line && has_space_before,
          "always-multi-line" => !is_single_line && !has_space_before,
          "never-multi-line" => !is_single_line && has_space_before,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-single-line" | "always-multi-line" => {
              "Expected a space before \"}\""
            }
            "never" | "never-single-line" | "never-multi-line" => "Unexpected space before \"}\"",
            _ => {
              i += 1;
              continue;
            }
          };
          diagnostics.push(Diagnostic::new(NAME, msg).span(Span::new(brace_pos, 1)));
        }
      }

      i += 1;
    }

    diagnostics
  }
}

/// Whether the block closing at this brace opens on the same line.
fn is_block_single_line(source: &str, closing_brace_pos: usize) -> bool {
  let bytes = source.as_bytes();
  let mut depth = 1;
  let mut j = closing_brace_pos;
  while j > 0 {
    j -= 1;
    if bytes[j] == b'}' {
      depth += 1;
    } else if bytes[j] == b'{' {
      depth -= 1;
      if depth == 0 {
        let segment = &source[j..closing_brace_pos];
        return !segment.contains('\n');
      }
    }
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source_and_options, invalid_options};

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StylisticBlockClosingBraceSpaceBefore, opts.clone());
    let ctx = ctx_with_source_and_options(source, &opts);
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn always_single_line_accepts_space() {
    let d = check("a { color: red; }", "always-single-line");
    assert!(d.is_empty());
  }

  #[test]
  fn always_single_line_rejects_no_space() {
    let d = check("a { color: red;}", "always-single-line");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn never_single_line_accepts_no_space() {
    let d = check("a { color: red;}", "never-single-line");
    assert!(d.is_empty());
  }

  #[test]
  fn never_single_line_rejects_space() {
    let d = check("a { color: red; }", "never-single-line");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_single_line_ignores_multiline() {
    let d = check("a {\n  color: red;\n}", "always-single-line");
    assert!(d.is_empty());
  }

  #[test]
  fn always_accepts_space_on_multiline() {
    let d = check("a { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space() {
    let d = check("a { color: red;}", "always");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn never_accepts_no_space() {
    let d = check("a { color: red;}", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space() {
    let d = check("a { color: red; }", "never");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("never-multi-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/block-closing-brace-space-before\""
      ]
    );
  }
}
