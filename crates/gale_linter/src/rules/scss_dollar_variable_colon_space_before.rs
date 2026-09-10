use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

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
pub struct ScssDollarVariableColonSpaceBefore;

impl Rule for ScssDollarVariableColonSpaceBefore {
  fn name(&self) -> &'static str {
    "scss/dollar-variable-colon-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before the colon in $variable declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let option = ctx.primary_option_str().unwrap_or("never");
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
                self.name(),
                "Unexpected space before \":\" in $variable declaration".to_string(),
              )
              .severity(self.default_severity())
              .span(Span::new(dollar_pos, name_end - dollar_pos)),
            );
          }
        }
        "always" => {
          if spaces != 1 {
            diagnostics.push(
              Diagnostic::new(
                self.name(),
                "Expected single space before \":\" in $variable declaration".to_string(),
              )
              .severity(self.default_severity())
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
  use gale_css_parser::Syntax;

  fn scss_ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    }
  }

  #[test]
  fn allows_no_space_before_colon() {
    let ctx = scss_ctx_with_source("$color: red;");
    let d = ScssDollarVariableColonSpaceBefore.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_space_before_colon() {
    let ctx = scss_ctx_with_source("$color : red;");
    let d = ScssDollarVariableColonSpaceBefore.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_scss() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "$color : red;",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssDollarVariableColonSpaceBefore
        .check_root(&[], &ctx)
        .is_empty()
    );
  }
}
