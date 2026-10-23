use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/dollar-variable-colon-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the colon in `$variable`
  /// declarations.
  ///
  /// Default: `"always"` (a single space after the colon).
  ///
  /// ```scss
  /// // Good (always)
  /// $var: value;
  ///
  /// // Bad (always)
  /// $var:value;
  /// $var:  value;
  /// ```
  ///
  /// Equivalent to `scss/dollar-variable-colon-space-after`.
  pub struct ScssDollarVariableColonSpaceAfter => ScssDollarVariableColonSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the colon in $variable declarations",
    default_severity: Warning,
  }
}

/// `scss/dollar-variable-colon-space-after` as configured.
pub struct ScssDollarVariableColonSpaceAfterCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"`, `"at-least-one-space"`.
  expectation: &'static str,
}

impl ScssDollarVariableColonSpaceAfterCheck {
  /// Stylelint's options: `"always"`, `"never"`, `"always-single-line"`, `"at-least-one-space"`, `"always"` when not given.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options
      .primary_one_of(&[
        "always",
        "never",
        "always-single-line",
        "at-least-one-space",
      ])
      .unwrap_or("always");
    options.finish()?;
    Ok(Self { expectation })
  }
}

impl Check for ScssDollarVariableColonSpaceAfterCheck {
  /// Flags a `$variable` declaration whose space after the colon does not match
  /// the option.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = self.expectation;
    let mut diagnostics = Vec::new();

    // Scan source text for $variable: patterns
    let source = ctx.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut paren_depth: usize = 0;

    while i < len {
      // Skip line comments (`// ...`)
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }
      // Skip block comments (`/* ... */`)
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        continue;
      }
      // Skip string literals
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

      // Track parentheses depth. $variable: inside parentheses is a
      // mixin/function parameter default or named argument — not a declaration.
      if bytes[i] == b'(' {
        paren_depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b')' {
        paren_depth = paren_depth.saturating_sub(1);
        i += 1;
        continue;
      }

      // Look for `$`
      if bytes[i] != b'$' {
        i += 1;
        continue;
      }

      // Collect variable name
      let dollar_pos = i;
      i += 1;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }

      // Check for `:` after variable name (skip whitespace before colon)
      let mut j = i;
      while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
      }

      if j >= len || bytes[j] != b':' {
        i = j;
        continue;
      }

      // Skip $var: inside parentheses (mixin parameters, named arguments)
      if paren_depth > 0 {
        i = j + 1;
        continue;
      }

      // Found `$var:` — now check space after colon
      let colon_pos = j;
      let after_colon = colon_pos + 1;

      match option {
        "always" => {
          if after_colon >= len || bytes[after_colon] != b' ' {
            diagnostics.push(
              Diagnostic::new(
                NAME,
                "Expected single space after \":\" in $variable declaration".to_string(),
              )
              .span(Span::new(dollar_pos, colon_pos - dollar_pos + 1)),
            );
          } else if after_colon + 1 < len && bytes[after_colon + 1] == b' ' {
            // Multiple spaces
            diagnostics.push(
              Diagnostic::new(
                NAME,
                "Expected single space after \":\" in $variable declaration".to_string(),
              )
              .span(Span::new(dollar_pos, colon_pos - dollar_pos + 1)),
            );
          }
        }
        "never" => {
          if after_colon < len && (bytes[after_colon] == b' ' || bytes[after_colon] == b'\t') {
            diagnostics.push(
              Diagnostic::new(
                NAME,
                "Unexpected space after \":\" in $variable declaration".to_string(),
              )
              .span(Span::new(dollar_pos, colon_pos - dollar_pos + 1)),
            );
          }
        }
        "always-single-line" | "at-least-one-space" => {
          // Simplified: just require at least one space
          if after_colon >= len || bytes[after_colon] != b' ' {
            diagnostics.push(
              Diagnostic::new(
                NAME,
                "Expected space after \":\" in $variable declaration".to_string(),
              )
              .span(Span::new(dollar_pos, colon_pos - dollar_pos + 1)),
            );
          }
        }
        _ => {}
      }

      i = after_colon + 1;
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
  fn allows_single_space() {
    let rule = configured(&ScssDollarVariableColonSpaceAfter, None);
    let ctx = scss_ctx_with_source("$color: red;");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_no_space() {
    let rule = configured(&ScssDollarVariableColonSpaceAfter, None);
    let ctx = scss_ctx_with_source("$color:red;");
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_multiple_spaces() {
    let rule = configured(&ScssDollarVariableColonSpaceAfter, None);
    let ctx = scss_ctx_with_source("$color:  red;");
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssDollarVariableColonSpaceAfter, None);
    let ctx = ctx_with_source("$color:red;");
    assert!(rule.check_source(&ctx).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("at-least-one-space")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sideways")),
      ["Invalid option value \"sideways\" for rule \"scss/dollar-variable-colon-space-after\""]
    );
  }
}
