use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/function-max-empty-lines";

declare_rule! {
  /// Limit the number of adjacent empty lines within functions.
  ///
  /// Equivalent to `@stylistic/function-max-empty-lines`.
  pub struct StylisticFunctionMaxEmptyLines => StylisticFunctionMaxEmptyLinesCheck {
    name: NAME,
    description: "Limit the number of adjacent empty lines within functions",
    default_severity: Warning,
  }
}

/// `@stylistic/function-max-empty-lines` as configured.
pub struct StylisticFunctionMaxEmptyLinesCheck {
  /// The most empty lines allowed in a row.
  max: usize,
}

impl StylisticFunctionMaxEmptyLinesCheck {
  /// The plugin's options: a number (`possible: isNumber`).  Gale reads no
  /// number as 0, and a number that is not a whole number of zero or more as 0
  /// too, as it always has.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(serde_json::Value::is_number);
    options.finish()?;
    Ok(Self {
      max: max.and_then(serde_json::Value::as_u64).unwrap_or(0) as usize,
    })
  }
}

impl Check for StylisticFunctionMaxEmptyLinesCheck {
  /// Flags runs of blank lines inside function arguments longer than the maximum.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

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
        i += 1;
        continue;
      }

      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
        continue;
      }

      // Detect function call: identifier followed by '('
      if bytes[i] == b'('
        && i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        let open_paren = i;
        let mut depth = 1;
        let mut j = i + 1;
        // Scan inside the function for empty lines
        while j < len && depth > 0 {
          if bytes[j] == b'(' {
            depth += 1;
          } else if bytes[j] == b')' {
            depth -= 1;
            if depth == 0 {
              break;
            }
          } else if bytes[j] == b'\'' || bytes[j] == b'"' {
            let q = bytes[j];
            j += 1;
            while j < len && bytes[j] != q {
              if bytes[j] == b'\\' {
                j += 1;
              }
              j += 1;
            }
          }
          j += 1;
        }
        let close_paren = j;

        // Now scan the content between parens for consecutive empty lines
        let mut k = open_paren + 1;
        while k < close_paren {
          if bytes[k] == b'\n' {
            let line_start = k;
            k += 1;
            let mut consecutive_empty = 0;
            while k < close_paren {
              let mut m = k;
              while m < close_paren && (bytes[m] == b' ' || bytes[m] == b'\t' || bytes[m] == b'\r')
              {
                m += 1;
              }
              if m < close_paren && bytes[m] == b'\n' {
                consecutive_empty += 1;
                k = m + 1;
              } else {
                break;
              }
            }

            if consecutive_empty > max {
              diagnostics.push(
                Diagnostic::new(NAME, format!("Expected no more than {} empty line(s)", max))
                  .span(Span::new(line_start, k - line_start)),
              );
            }
            continue;
          }
          k += 1;
        }

        i = close_paren + 1;
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(source: &str, max: u64) -> Vec<Diagnostic> {
    let opts = serde_json::json!(max);
    let rule = configured(&StylisticFunctionMaxEmptyLines, opts.clone());
    let ctx = ctx_with_source(source);
    rule.check_source(&ctx)
  }

  #[test]
  fn allows_no_empty_lines() {
    let d = check("a { transform: translate(\n1px,\n2px\n); }", 0);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_empty_line_in_function_when_max_zero() {
    let d = check("a { transform: translate(\n1px,\n\n2px\n); }", 0);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 0"));
  }

  #[test]
  fn allows_one_empty_line_when_max_one() {
    let d = check("a { transform: translate(\n1px,\n\n2px\n); }", 1);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_two_empty_lines_when_max_one() {
    let d = check("a { transform: translate(\n1px,\n\n\n2px\n); }", 1);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(0)).is_empty());
    assert!(invalid_options(NAME, serde_json::json!(2)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("2")),
      ["Invalid option \"2\" for rule \"@stylistic/function-max-empty-lines\""]
    );
  }
}
