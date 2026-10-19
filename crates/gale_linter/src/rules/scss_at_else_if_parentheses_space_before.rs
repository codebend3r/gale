use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-else-if-parentheses-space-before";

declare_rule! {
  /// Require or disallow a space before `@else if` parentheses.
  ///
  /// Primary option: `"never"` (default) or `"always"`.
  ///
  /// ```scss
  /// // Good (never)
  /// @else if($condition) {}
  ///
  /// // Good (always)
  /// @else if ($condition) {}
  /// ```
  ///
  /// This is distinct from `scss/at-rule-conditional-no-parentheses` which
  /// disallows parentheses entirely. This rule only controls the space before
  /// the opening parenthesis.
  ///
  /// Equivalent to `scss/at-else-if-parentheses-space-before`.
  pub struct ScssAtElseIfParenthesesSpaceBefore => ScssAtElseIfParenthesesSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before @else if parentheses",
    default_severity: Warning,
  }
}

/// `scss/at-else-if-parentheses-space-before` as configured.
pub struct ScssAtElseIfParenthesesSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssAtElseIfParenthesesSpaceBeforeCheck {
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

impl Check for ScssAtElseIfParenthesesSpaceBeforeCheck {
  /// Flags an `@else if` whose space before the opening parenthesis does not
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

      // Check for "else" keyword
      let remaining = &source[i..];
      if !remaining
        .get(..4)
        .map(|s| s.eq_ignore_ascii_case("else"))
        .unwrap_or(false)
      {
        continue;
      }

      i += 4;

      // Must be followed by whitespace
      if i >= len || !bytes[i].is_ascii_whitespace() {
        continue;
      }

      // Skip whitespace between "else" and "if"
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }

      // Check for "if" keyword
      if !source
        .get(i..i + 2)
        .map(|s| s.eq_ignore_ascii_case("if"))
        .unwrap_or(false)
      {
        continue;
      }

      i += 2;

      // Now we're right after `@else if`
      if i >= len {
        continue;
      }

      // Check what follows: space then `(`, or directly `(`
      if bytes[i] == b'(' {
        // No space before paren: `@else if(`
        if option == "always" {
          diagnostics.push(
            Diagnostic::new(NAME, "Expected a space before parentheses in @else if")
              .span(Span::new(at_pos, i - at_pos + 1)),
          );
        }
      } else if bytes[i] == b' ' && i + 1 < len && bytes[i + 1] == b'(' {
        // Space before paren: `@else if (`
        if option == "never" {
          diagnostics.push(
            Diagnostic::new(NAME, "Unexpected space before parentheses in @else if")
              .span(Span::new(at_pos, i - at_pos + 2)),
          );
        }
        i += 1;
      }
      // else: no parentheses at all — not our concern
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
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, None);
    let src = "@if $x { } @else if($y) { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space() {
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, None);
    let src = "@if $x { } @else if ($y) { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_allows_space() {
    let src = "@if $x { } @else if ($y) { }";
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_space() {
    let src = "@if $x { } @else if($y) { }";
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, opts.clone());
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn skips_plain_else() {
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, None);
    // @else without if — should not trigger
    let src = "@if $x { } @else { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_else_if_without_parens() {
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, None);
    // @else if without parentheses — not our concern
    let src = "@if $x { } @else if $y { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, None);
    let ctx = ctx_with_source("@else if($y) { }");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn handles_multiple_else_if() {
    let rule = configured(&ScssAtElseIfParenthesesSpaceBefore, None);
    let src = "@if $a { } @else if($b) { } @else if ($c) { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    // default "never": second @else if has space => 1 report
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/at-else-if-parentheses-space-before\""]
    );
  }
}
