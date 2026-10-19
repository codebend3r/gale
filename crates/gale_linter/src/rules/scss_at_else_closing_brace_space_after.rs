use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::braces::counted_block_close;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-else-closing-brace-space-after";

declare_rule! {
  /// Require or disallow a space after the closing brace of `@else` when
  /// followed by `@else if`.
  ///
  /// Default: `"always-intermediate"`.
  ///
  /// Equivalent to `scss/at-else-closing-brace-space-after`.
  pub struct ScssAtElseClosingBraceSpaceAfter => ScssAtElseClosingBraceSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the closing brace of @else before @else",
    default_severity: Warning,
  }
}

/// `scss/at-else-closing-brace-space-after` as configured.
pub struct ScssAtElseClosingBraceSpaceAfterCheck {
  /// The primary option; `"always-intermediate"` when none is given.
  option: &'static str,
}

impl ScssAtElseClosingBraceSpaceAfterCheck {
  /// stylelint-scss's options: `"always-intermediate"` or
  /// `"never-intermediate"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always-intermediate", "never-intermediate"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always-intermediate"),
    })
  }
}

impl Check for ScssAtElseClosingBraceSpaceAfterCheck {
  /// Scans the SCSS source for `@else` chains and flags a closing brace whose
  /// following space does not match the option.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.option;
    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();

    let mut i = 0;
    while i < len {
      if bytes[i] == b'@' && source[i..].starts_with("@else") {
        let after_else = i + 5;
        let mut j = after_else;
        while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
          j += 1;
        }

        // Check if this is `@else if` — it has its own closing brace
        let is_else_if = j < len && source[j..].starts_with("if ");

        if is_else_if {
          // Find closing brace of this `@else if`
          if let Some(close_brace) = counted_block_close(source, i) {
            let after = close_brace + 1;
            let mut k = after;
            let mut space_count = 0;
            while k < len && bytes[k] == b' ' {
              space_count += 1;
              k += 1;
            }

            let followed_by_else = k < len && source[k..].starts_with("@else");

            if followed_by_else {
              match option {
                "always-intermediate" => {
                  if space_count != 1 {
                    diagnostics.push(
                      Diagnostic::new(
                        NAME,
                        "Expected single space after closing brace of @else if before @else"
                          .to_string(),
                      )
                      .span(Span::new(close_brace, 1)),
                    );
                  }
                }
                "never-intermediate" => {
                  if space_count > 0 {
                    diagnostics.push(
                      Diagnostic::new(
                        NAME,
                        "Unexpected space after closing brace of @else if before @else".to_string(),
                      )
                      .span(Span::new(close_brace, 1)),
                    );
                  }
                }
                _ => {}
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

  use crate::testing::{configured, ctx_with_source, scss_ctx_with_source};

  #[test]
  fn allows_space_between_else_if_and_else() {
    let rule = configured(&ScssAtElseClosingBraceSpaceAfter, None);
    let ctx = scss_ctx_with_source("@if $a { a: b; } @else if $b { c: d; } @else { e: f; }");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_no_space_between_else_if_and_else() {
    let rule = configured(&ScssAtElseClosingBraceSpaceAfter, None);
    let ctx = scss_ctx_with_source("@if $a { a: b; } @else if $b { c: d; }@else { e: f; }");
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssAtElseClosingBraceSpaceAfter, None);
    let ctx = ctx_with_source("@else if { }@else { }");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint_scss() {
    use crate::testing::invalid_options;
    assert!(invalid_options(NAME, serde_json::json!("never-intermediate")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"scss/at-else-closing-brace-space-after\""]
    );
  }
}
