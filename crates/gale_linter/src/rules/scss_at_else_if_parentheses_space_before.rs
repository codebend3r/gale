use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

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
pub struct ScssAtElseIfParenthesesSpaceBefore;

impl Rule for ScssAtElseIfParenthesesSpaceBefore {
  fn name(&self) -> &'static str {
    "scss/at-else-if-parentheses-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before @else if parentheses"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = ctx.primary_option_str().unwrap_or("never");
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
            Diagnostic::new(
              self.name(),
              "Expected a space before parentheses in @else if",
            )
            .severity(self.default_severity())
            .span(Span::new(at_pos, i - at_pos + 1)),
          );
        }
      } else if bytes[i] == b' ' && i + 1 < len && bytes[i + 1] == b'(' {
        // Space before paren: `@else if (`
        if option == "never" {
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              "Unexpected space before parentheses in @else if",
            )
            .severity(self.default_severity())
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
  use gale_css_parser::Syntax;

  fn scss_ctx_with_options<'a>(source: &'a str, options: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "t.scss",
      source,
      syntax: Syntax::Scss,
      options: Some(options),
    }
  }

  fn scss_ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    }
  }

  #[test]
  fn never_allows_no_space() {
    let src = "@if $x { } @else if($y) { }";
    let d = ScssAtElseIfParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space() {
    let src = "@if $x { } @else if ($y) { }";
    let d = ScssAtElseIfParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_allows_space() {
    let src = "@if $x { } @else if ($y) { }";
    let opts = serde_json::json!("always");
    let ctx = scss_ctx_with_options(src, &opts);
    let d = ScssAtElseIfParenthesesSpaceBefore.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_space() {
    let src = "@if $x { } @else if($y) { }";
    let opts = serde_json::json!("always");
    let ctx = scss_ctx_with_options(src, &opts);
    let d = ScssAtElseIfParenthesesSpaceBefore.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn skips_plain_else() {
    // @else without if — should not trigger
    let src = "@if $x { } @else { }";
    let d = ScssAtElseIfParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_else_if_without_parens() {
    // @else if without parentheses — not our concern
    let src = "@if $x { } @else if $y { }";
    let d = ScssAtElseIfParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "@else if($y) { }",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssAtElseIfParenthesesSpaceBefore
        .check_root(&[], &ctx)
        .is_empty()
    );
  }

  #[test]
  fn handles_multiple_else_if() {
    let src = "@if $a { } @else if($b) { } @else if ($c) { }";
    let d = ScssAtElseIfParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
    // default "never": second @else if has space => 1 report
    assert_eq!(d.len(), 1);
  }
}
