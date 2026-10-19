use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-function-parentheses-space-before";

declare_rule! {
  /// Require or disallow a space before `@function` parentheses.
  ///
  /// Primary option: `"never"` (default) or `"always"`.
  ///
  /// ```scss
  /// // Good (never)
  /// @function foo($x) { @return $x; }
  ///
  /// // Good (always)
  /// @function foo ($x) { @return $x; }
  /// ```
  ///
  /// Equivalent to `scss/at-function-parentheses-space-before`.
  pub struct ScssAtFunctionParenthesesSpaceBefore => ScssAtFunctionParenthesesSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before @function parentheses",
    default_severity: Warning,
  }
}

/// `scss/at-function-parentheses-space-before` as configured.
pub struct ScssAtFunctionParenthesesSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssAtFunctionParenthesesSpaceBeforeCheck {
  /// Stylelint's options: `"always"` or `"never"`, `"never"` when not given.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always", "never"])
      .unwrap_or("never");
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssAtFunctionParenthesesSpaceBeforeCheck {
  /// Flags an `@function` whose space before the opening parenthesis does not
  /// match the option.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.expectation;
    let source = ctx.source;
    let mut diagnostics = Vec::new();

    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      if bytes[i] != b'@' {
        i += 1;
        continue;
      }

      let at_pos = i;
      i += 1;

      // Check for "function" keyword (case-insensitive)
      let remaining = &source[i..];
      if !remaining
        .get(..8)
        .map(|s| s.eq_ignore_ascii_case("function"))
        .unwrap_or(false)
      {
        continue;
      }

      i += 8;

      // Must be followed by whitespace
      if i >= len || !bytes[i].is_ascii_whitespace() {
        continue;
      }

      // Skip whitespace to find the function name
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }

      // Collect function name
      let name_start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }

      if i == name_start {
        continue;
      }

      if i >= len {
        continue;
      }

      if bytes[i] == b'(' {
        // No space before paren
        if option == "always" {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              "Expected a space before parentheses in @function declaration",
            )
            .span(Span::new(at_pos, i - at_pos + 1)),
          );
        }
      } else if bytes[i] == b' ' && i + 1 < len && bytes[i + 1] == b'(' {
        // Space before paren
        if option == "never" {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              "Unexpected space before parentheses in @function declaration",
            )
            .span(Span::new(at_pos, i - at_pos + 2)),
          );
        }
        i += 1;
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options, scss_ctx_with_source};

  #[test]
  fn never_allows_no_space() {
    let rule = configured(&ScssAtFunctionParenthesesSpaceBefore, None);
    let src = "@function foo($x) { @return $x; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space() {
    let rule = configured(&ScssAtFunctionParenthesesSpaceBefore, None);
    let src = "@function foo ($x) { @return $x; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_allows_space() {
    let src = "@function foo ($x) { @return $x; }";
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtFunctionParenthesesSpaceBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_space() {
    let src = "@function foo($x) { @return $x; }";
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtFunctionParenthesesSpaceBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtFunctionParenthesesSpaceBefore, None);
    let ctx = ctx_with_source("@function foo($x) { }");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn handles_multiple_functions() {
    let rule = configured(&ScssAtFunctionParenthesesSpaceBefore, None);
    let src = "@function foo($x) { @return $x; }\n@function bar ($y) { @return $y; }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert_eq!(d.len(), 1); // only bar has a space
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/at-function-parentheses-space-before\""]
    );
  }
}
