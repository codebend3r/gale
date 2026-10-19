use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/function-whitespace-after";

declare_rule! {
  /// Require or disallow whitespace after functions.
  ///
  /// Primary: "always" | "never"
  pub struct StylisticFunctionWhitespaceAfter => StylisticFunctionWhitespaceAfterCheck {
    name: NAME,
    description: "Require or disallow whitespace after functions",
    default_severity: Warning,
  }
}

/// `@stylistic/function-whitespace-after` as configured.
pub struct StylisticFunctionWhitespaceAfterCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticFunctionWhitespaceAfterCheck {
  /// The plugin's options: `"always"` or `"never"`.  Gale reads no option as
  /// `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticFunctionWhitespaceAfterCheck {
  /// Flags a closing function parenthesis whose trailing whitespace does not
  /// match the option.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
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
      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
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

      // Detect function call — but skip pseudo-class functions like :not(), :is(), etc.
      if bytes[i] == b'('
        && i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        // Walk back to find the function name start
        let mut fname_start = i - 1;
        while fname_start > 0
          && (bytes[fname_start - 1].is_ascii_alphanumeric()
            || bytes[fname_start - 1] == b'-'
            || bytes[fname_start - 1] == b'_')
        {
          fname_start -= 1;
        }
        // Skip if preceded by `:` (pseudo-class/element) or `::` (pseudo-element)
        if fname_start > 0 && bytes[fname_start - 1] == b':' {
          // Skip the entire pseudo-function parenthesized content
          let mut depth_p = 1;
          let mut j = i + 1;
          while j < len && depth_p > 0 {
            if bytes[j] == b'(' {
              depth_p += 1;
            } else if bytes[j] == b')' {
              depth_p -= 1;
            }
            j += 1;
          }
          i = j;
          continue;
        }
        let mut depth = 1;
        let mut j = i + 1;
        while j < len && depth > 0 {
          if bytes[j] == b'(' {
            depth += 1;
          } else if bytes[j] == b')' {
            depth -= 1;
          } else if bytes[j] == b'\'' || bytes[j] == b'"' {
            let q = bytes[j];
            j += 1;
            while j < len && bytes[j] != q {
              if bytes[j] == b'\\' {
                j += 1;
              }
              j += 1;
            }
          }
          if depth > 0 {
            j += 1;
          }
        }
        let close_paren = j;

        // Check the character after the closing paren.
        // Skip any block comment immediately following the paren
        // (e.g. `rotate(90deg)/* rtl:ignore */;`) and look at what
        // comes after the comment — if it is a natural terminator,
        // no whitespace is required.
        let mut after = close_paren + 1;
        if after + 1 < len && bytes[after] == b'/' && bytes[after + 1] == b'*' {
          after += 2;
          while after + 1 < len && !(bytes[after] == b'*' && bytes[after + 1] == b'/') {
            after += 1;
          }
          after += 2; // skip */
        }
        if after < len {
          let ch_after = bytes[after];
          // Don't flag if followed by `)`, `,`, `;`, `}`, `{`, or newline — those are
          // natural terminators where whitespace is not expected.
          if !matches!(ch_after, b')' | b',' | b';' | b'}' | b'{' | b'\n' | b'\r') {
            let has_ws = ch_after == b' ' || ch_after == b'\t';

            let violation = match option {
              "always" => !has_ws,
              "never" => has_ws,
              _ => false,
            };

            if violation {
              let msg = match option {
                "always" => "Expected whitespace after function",
                "never" => "Unexpected whitespace after function",
                _ => "Expected whitespace after function",
              };
              diagnostics.push(Diagnostic::new(NAME, msg).span(Span::new(close_paren, 1)));
            }
          }
        }

        i = close_paren + 1;
        continue;
      }
      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options};

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StylisticFunctionWhitespaceAfter, opts.clone());
    let ctx = ctx_with_source(source);
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn always_accepts_space_after_function() {
    let d = check("a { background: url(foo.png) no-repeat; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_accepts_comma_after_function() {
    // Comma after function is a natural terminator, no space needed
    let d = check("a { transform: translate(1px), scale(2); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_accepts_semicolon_after_function() {
    let d = check("a { background: url(foo.png); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_after_function() {
    let d = check("a { background: url(foo.png)no-repeat; }", "always");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn never_accepts_no_space() {
    let d = check("a { background: url(foo.png)no-repeat; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn always_accepts_comment_after_function_before_semicolon() {
    // `rotate(90deg)/* rtl:ignore */;` — comment immediately after `)` then `;`
    let d = check("a { transform: rotate(90deg)/* rtl:ignore */; }", "always");
    assert!(
      d.is_empty(),
      "a comment between function close and semicolon should not require whitespace"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/function-whitespace-after\""]
    );
  }
}
