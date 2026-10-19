use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/number-leading-zero";

declare_rule! {
  /// Require or disallow a leading zero for fractional numbers less than 1.
  ///
  /// Equivalent to `@stylistic/number-leading-zero`.
  pub struct StylisticNumberLeadingZero => StylisticNumberLeadingZeroCheck {
    name: NAME,
    description: "Require or disallow a leading zero for fractional numbers less than 1",
    default_severity: Warning,
  }
}

/// `@stylistic/number-leading-zero` as configured.
pub struct StylisticNumberLeadingZeroCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticNumberLeadingZeroCheck {
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

impl Check for StylisticNumberLeadingZeroCheck {
  /// Flags a fractional number whose leading zero does not match the option.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;

    while i < len {
      // Skip strings
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

      // Skip block comments
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

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      match option {
        "always" => {
          // Look for bare .N pattern (missing leading zero)
          if bytes[i] == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit() {
            // Check that the char before is not a digit (which would be e.g. 1.5)
            let prev_is_digit = i > 0 && bytes[i - 1].is_ascii_digit();
            // Also not after a letter or hyphen (could be part of identifier)
            let prev_is_ident = i > 0
              && (bytes[i - 1].is_ascii_alphanumeric()
                || bytes[i - 1] == b'-'
                || bytes[i - 1] == b'_');
            if !prev_is_digit && !prev_is_ident {
              diagnostics
                .push(Diagnostic::new(NAME, "Expected a leading zero").span(Span::new(i, 1)));
            }
          }
        }
        "never" => {
          // Look for 0.N pattern (unnecessary leading zero)
          if bytes[i] == b'0'
            && i + 1 < len
            && bytes[i + 1] == b'.'
            && i + 2 < len
            && bytes[i + 2].is_ascii_digit()
          {
            // Make sure it's not part of a larger number like 10.5
            let prev_is_digit = i > 0 && bytes[i - 1].is_ascii_digit();
            if !prev_is_digit {
              diagnostics
                .push(Diagnostic::new(NAME, "Unexpected leading zero").span(Span::new(i, 1)));
            }
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

  use crate::testing::{configured, ctx_with_source_and_options, invalid_options};

  #[test]
  fn always_allows_leading_zero() {
    let opt = serde_json::json!("always");
    let source = "a { opacity: 0.5; }";
    let d = configured(&StylisticNumberLeadingZero, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_missing_leading_zero() {
    let opt = serde_json::json!("always");
    let source = "a { opacity: .5; }";
    let d = configured(&StylisticNumberLeadingZero, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a leading zero"));
  }

  #[test]
  fn never_reports_leading_zero() {
    let opt = serde_json::json!("never");
    let source = "a { opacity: 0.5; }";
    let d = configured(&StylisticNumberLeadingZero, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected leading zero"));
  }

  #[test]
  fn never_allows_bare_dot() {
    let opt = serde_json::json!("never");
    let source = "a { opacity: .5; }";
    let d = configured(&StylisticNumberLeadingZero, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_flag_normal_decimals() {
    let opt = serde_json::json!("always");
    let source = "a { width: 1.5px; }";
    let d = configured(&StylisticNumberLeadingZero, opt.clone())
      .check_root(&[], &ctx_with_source_and_options(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/number-leading-zero\""]
    );
  }
}
