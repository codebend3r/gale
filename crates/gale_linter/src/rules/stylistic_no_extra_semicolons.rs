use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/no-extra-semicolons";

declare_rule! {
  /// Disallow extra semicolons.
  ///
  /// Equivalent to `@stylistic/no-extra-semicolons`.
  pub struct StylisticNoExtraSemicolons => StylisticNoExtraSemicolonsCheck {
    name: NAME,
    description: "Disallow extra semicolons",
    default_severity: Warning,
  }
}

/// `@stylistic/no-extra-semicolons`, which takes no options.
pub struct StylisticNoExtraSemicolonsCheck;

impl StylisticNoExtraSemicolonsCheck {
  /// The plugin validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for StylisticNoExtraSemicolonsCheck {
  /// Flags semicolons that terminate nothing.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
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

      // Skip comments
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

      if bytes[i] == b';' {
        // Check for ;; (double semicolons)
        if i + 1 < len && bytes[i + 1] == b';' {
          diagnostics
            .push(Diagnostic::new(NAME, "Unexpected extra semicolon").span(Span::new(i + 1, 1)));
          i += 1;
          continue;
        }

        // Check for ; followed by only whitespace then }
        let mut j = i + 1;
        while j < len
          && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r')
        {
          j += 1;
        }
        if j < len && bytes[j] == b'}' {
          // This is fine -- a trailing semicolon before } is normal.
          // But check if there's ONLY a semicolon (no property) -- i.e.
          // the semicolon itself is extra if there's another semicolon
          // right before it (already caught above).
        }
      }

      i += 1;
    }

    // Second pass: find standalone ; before } (extra semicolons at top level or
    // semicolons that are the only content before })
    i = 0;
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

      // Skip comments
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

      // Look for ; at the top level (outside blocks) which is extra
      if bytes[i] == b';' {
        // Check backwards to see if there's another ; before this one
        // with only whitespace in between
        let mut k = i.wrapping_sub(1);
        if i > 0 {
          k = i - 1;
          while k > 0
            && (bytes[k] == b' ' || bytes[k] == b'\t' || bytes[k] == b'\n' || bytes[k] == b'\r')
          {
            k -= 1;
          }
          if bytes[k] == b';' {
            // Already reported as ;; or separated by whitespace
            let already_reported = diagnostics.iter().any(|d| d.span.offset == i);
            if !already_reported {
              diagnostics
                .push(Diagnostic::new(NAME, "Unexpected extra semicolon").span(Span::new(i, 1)));
            }
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

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_normal_semicolons() {
    let source = "a { color: red; display: block; }";
    let d = configured(&StylisticNoExtraSemicolons, None).check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_double_semicolons() {
    let source = "a { color: red;; }";
    let d = configured(&StylisticNoExtraSemicolons, None).check_root(&[], &ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("extra semicolon"));
  }

  #[test]
  fn reports_semicolons_separated_by_whitespace() {
    let source = "a { color: red; ; }";
    let d = configured(&StylisticNoExtraSemicolons, None).check_root(&[], &ctx_with_source(source));
    assert!(!d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"@stylistic/no-extra-semicolons\""]
    );
  }
}
