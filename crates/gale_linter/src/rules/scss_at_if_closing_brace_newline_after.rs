use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::scss_block_close;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-if-closing-brace-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the closing brace of
  /// `@if` statements that are NOT followed by `@else`.
  ///
  /// Default: `"always-last-in-chain"` — require a newline after the closing
  /// brace of the last `@if`/`@else if` in a chain (i.e. when not followed
  /// by `@else`).
  ///
  /// Equivalent to `scss/at-if-closing-brace-newline-after`.
  pub struct ScssAtIfClosingBraceNewlineAfter => ScssAtIfClosingBraceNewlineAfterCheck {
    name: NAME,
    description: "Require or disallow a newline after the closing brace of @if statements",
    default_severity: Warning,
  }
}

/// `scss/at-if-closing-brace-newline-after` as configured.
pub struct ScssAtIfClosingBraceNewlineAfterCheck {
  /// The primary option: `"always-last-in-chain"`.
  expectation: &'static str,
}

impl ScssAtIfClosingBraceNewlineAfterCheck {
  /// Stylelint's options: `"always-last-in-chain"` (also when not given), then
  /// `disableFix`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&["always-last-in-chain"])
      .unwrap_or("always-last-in-chain");
    // Upstream validates `{ disableFix: isBoolean }`, so any other key is
    // unknown.
    options.disable_fix();
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssAtIfClosingBraceNewlineAfterCheck {
  /// Flags an `@if` closing brace, when no `@else` follows, whose trailing
  /// newline does not match the option.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.expectation;
    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();

    // Find `@if` occurrences and check what follows their closing `}`
    let mut i = 0;
    while i < len {
      // Look for `@if` or `@else if`
      if bytes[i] == b'@' && i + 2 < len {
        let rest = &source[i..];
        let is_if = rest.starts_with("@if ") || rest.starts_with("@if(");
        let is_else_if = rest.starts_with("@else if ") || rest.starts_with("@else if(");

        if is_if || is_else_if {
          // Find the matching closing `}`
          if let Some(close_brace) = scss_block_close(source, i) {
            let after = close_brace + 1;
            // Skip whitespace/spaces (not newlines) after `}`
            let mut j = after;
            while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
              j += 1;
            }

            // Check what follows
            let followed_by_else = j < len && source[j..].starts_with("@else");

            if option == "always-last-in-chain" && !followed_by_else {
              // Must be followed by newline
              if j < len && bytes[j] != b'\n' && bytes[j] != b'\r' {
                diagnostics.push(
                  Diagnostic::new(
                    NAME,
                    "Expected newline after closing brace of @if".to_string(),
                  )
                  .span(Span::new(close_brace, 1)),
                );
              }
            }

            i = after;
            continue;
          }
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

  use crate::testing::{
    CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
  };

  #[test]
  fn allows_newline_after_if() {
    let rule = configured(&ScssAtIfClosingBraceNewlineAfter, None);
    let ctx = scss_ctx_with_source("@if $cond { color: red; }\n.foo {}");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_else_on_same_line() {
    let rule = configured(&ScssAtIfClosingBraceNewlineAfter, None);
    let ctx = scss_ctx_with_source("@if $cond { color: red; } @else { color: blue; }\n");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_interpolation_inside_if() {
    let rule = configured(&ScssAtIfClosingBraceNewlineAfter, None);
    // @if $breakpoint != "base" {
    //   $breakpoint-name: -on-#{$breakpoint};
    // }
    let src = "@if $breakpoint != \"base\" {\n  $breakpoint-name: -on-#{$breakpoint};\n}\n";
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_if_else_chain_with_interpolation() {
    let rule = configured(&ScssAtIfClosingBraceNewlineAfter, None);
    let src =
      "@if $x { $name: #{$x}-foo; } @else if $y { $name: #{$y}-bar; } @else { $name: baz; }\n";
    let ctx = scss_ctx_with_source(src);
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtIfClosingBraceNewlineAfter, None);
    let ctx = ctx_with_source("@if { } .foo {}");
    assert!(rule.check_source(&ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always-last-in-chain")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/at-if-closing-brace-newline-after\""]
    );
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["always-last-in-chain", { "disableFix": true }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always-last-in-chain", { "ignore": ["x"] }])
      ),
      ["Invalid option name \"ignore\" for rule \"scss/at-if-closing-brace-newline-after\""]
    );
  }
}
