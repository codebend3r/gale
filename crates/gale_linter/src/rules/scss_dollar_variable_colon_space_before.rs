use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/dollar-variable-colon-space-before";

declare_rule! {
  /// Require or disallow whitespace before the colon in `$variable` declarations.
  ///
  /// Default: `"never"` (no space before the colon).
  ///
  /// ```scss
  /// // Good (never)
  /// $var: value;
  ///
  /// // Bad (never)
  /// $var : value;
  /// ```
  ///
  /// Equivalent to `scss/dollar-variable-colon-space-before`.
  pub struct ScssDollarVariableColonSpaceBefore => ScssDollarVariableColonSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the colon in $variable declarations",
    default_severity: Warning,
  }
}

/// `scss/dollar-variable-colon-space-before` as configured.
pub struct ScssDollarVariableColonSpaceBeforeCheck {
  /// The primary option: `"always"` or `"never"`.
  expectation: &'static str,
}

impl ScssDollarVariableColonSpaceBeforeCheck {
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

impl Check for ScssDollarVariableColonSpaceBeforeCheck {
  /// Flags a `$variable` declaration whose space before the colon does not match
  /// the option.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.expectation;
    let mut diagnostics = Vec::new();

    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      if bytes[i] != b'$' {
        i += 1;
        continue;
      }

      let dollar_pos = i;
      i += 1;
      // Collect variable name
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }

      let name_end = i;

      // Count spaces between name end and colon
      let mut spaces = 0;
      while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
        spaces += 1;
        i += 1;
      }

      if i >= len || bytes[i] != b':' {
        continue;
      }

      match option {
        "never" => {
          if spaces > 0 {
            diagnostics.push(
              Diagnostic::new(
                NAME,
                "Unexpected space before \":\" in $variable declaration".to_string(),
              )
              .span(Span::new(dollar_pos, name_end - dollar_pos)),
            );
          }
        }
        "always" => {
          if spaces != 1 {
            diagnostics.push(
              Diagnostic::new(
                NAME,
                "Expected single space before \":\" in $variable declaration".to_string(),
              )
              .span(Span::new(dollar_pos, name_end - dollar_pos)),
            );
          }
        }
        _ => {}
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options, scss_ctx_with_source};

  #[test]
  fn allows_no_space_before_colon() {
    let rule = configured(&ScssDollarVariableColonSpaceBefore, None);
    let ctx = scss_ctx_with_source("$color: red;");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_space_before_colon() {
    let rule = configured(&ScssDollarVariableColonSpaceBefore, None);
    let ctx = scss_ctx_with_source("$color : red;");
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssDollarVariableColonSpaceBefore, None);
    let ctx = ctx_with_source("$color : red;");
    assert!(rule.check_root(&[], &ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/dollar-variable-colon-space-before\""]
    );
  }
}
