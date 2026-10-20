use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-mixin-parentheses-space-before";

declare_rule! {
  /// Require or disallow a space before `@mixin` parentheses.
  ///
  /// Primary option: `"never"` (default) or `"always"`.
  ///
  /// ```scss
  /// // Good (never)
  /// @mixin foo($x) {}
  ///
  /// // Good (always)
  /// @mixin foo ($x) {}
  /// ```
  ///
  /// Equivalent to `scss/at-mixin-parentheses-space-before`.
  pub struct ScssAtMixinParenthesesSpaceBefore => ScssAtMixinParenthesesSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before @mixin parentheses",
    default_severity: Warning,
  }
}

/// `scss/at-mixin-parentheses-space-before` as configured.
pub struct ScssAtMixinParenthesesSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssAtMixinParenthesesSpaceBeforeCheck {
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

impl Check for ScssAtMixinParenthesesSpaceBeforeCheck {
  /// Flags a `@mixin` whose space before the opening parenthesis does not match
  /// the option.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.expectation;
    let source = ctx.source;
    let mut diagnostics = Vec::new();

    // Scan source for @mixin declarations with parentheses
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      // Look for `@`
      if bytes[i] != b'@' {
        i += 1;
        continue;
      }

      let at_pos = i;
      i += 1;

      // Check for "mixin" keyword (case-insensitive)
      let remaining = &source[i..];
      if !remaining
        .get(..5)
        .map(|s| s.eq_ignore_ascii_case("mixin"))
        .unwrap_or(false)
      {
        continue;
      }

      i += 5;

      // Must be followed by whitespace (to not match e.g. @mixin-foo)
      if i >= len || (!bytes[i].is_ascii_whitespace()) {
        continue;
      }

      // Skip whitespace to find the mixin name
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }

      // Collect mixin name
      let name_start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }

      if i == name_start {
        // No name found
        continue;
      }

      // Check if there's a `(` (possibly with a space before it)
      if i >= len {
        continue;
      }

      if bytes[i] == b'(' {
        // No space before paren: `@mixin name(`
        if option == "always" {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              "Expected a space before parentheses in @mixin declaration",
            )
            .span(Span::new(at_pos, i - at_pos + 1)),
          );
        }
      } else if bytes[i] == b' ' && i + 1 < len && bytes[i + 1] == b'(' {
        // One space before paren: `@mixin name (`
        if option == "never" {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              "Unexpected space before parentheses in @mixin declaration",
            )
            .span(Span::new(at_pos, i - at_pos + 2)),
          );
        }
        i += 1; // skip past the space
      }
      // else: no parentheses (argumentless mixin) — skip
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    configured, ctx_with_source, invalid_options, scss_ctx_with_source,
    scss_ctx_with_source_and_options,
  };

  #[test]
  fn never_allows_no_space() {
    let rule = configured(&ScssAtMixinParenthesesSpaceBefore, None);
    let src = "@mixin foo($x) { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space() {
    let rule = configured(&ScssAtMixinParenthesesSpaceBefore, None);
    let src = "@mixin foo ($x) { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_allows_space() {
    let src = "@mixin foo ($x) { }";
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtMixinParenthesesSpaceBefore, opts.clone());
    let ctx = scss_ctx_with_source_and_options(src, &opts);
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_space() {
    let src = "@mixin foo($x) { }";
    let opts = serde_json::json!("always");
    let rule = configured(&ScssAtMixinParenthesesSpaceBefore, opts.clone());
    let ctx = scss_ctx_with_source_and_options(src, &opts);
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn skips_argumentless_mixin() {
    let rule = configured(&ScssAtMixinParenthesesSpaceBefore, None);
    let src = "@mixin foo { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtMixinParenthesesSpaceBefore, None);
    let ctx = ctx_with_source("@mixin foo($x) { }");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn handles_multiple_mixins() {
    let rule = configured(&ScssAtMixinParenthesesSpaceBefore, None);
    let src = "@mixin foo($x) { }\n@mixin bar ($y) { }";
    let d = rule.check_root(&[], &scss_ctx_with_source(src));
    // default "never": second one has space => 1 report
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/at-mixin-parentheses-space-before\""]
    );
  }
}
