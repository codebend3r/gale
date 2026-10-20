use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/function-comma-space-before";

declare_rule! {
  /// Require or disallow a space before the commas of functions.
  ///
  /// Primary: "always" | "never" | "always-single-line" | "never-single-line"
  pub struct StylisticFunctionCommaSpaceBefore => StylisticFunctionCommaSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the commas of functions",
    default_severity: Warning,
  }
}

/// `@stylistic/function-comma-space-before` as configured.
pub struct StylisticFunctionCommaSpaceBeforeCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticFunctionCommaSpaceBeforeCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`, then `ignoreFunctions`.  Gale reads no option as
  /// `"never"`.  Gale accepts but does not yet apply the secondary options.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.strings("ignoreFunctions");
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticFunctionCommaSpaceBeforeCheck {
  /// Flags a function argument comma whose preceding space does not match the option.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
        continue;
      }

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip SCSS interpolation #{...}
      if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
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

      // Detect function call
      if bytes[i] == b'('
        && i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        let paren_start = i;
        let mut depth = 1;
        let mut j = i + 1;
        while j < len && depth > 0 {
          if bytes[j] == b'(' {
            depth += 1;
          } else if bytes[j] == b')' {
            depth -= 1;
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
          if depth > 0 {
            j += 1;
          }
        }
        let paren_end = j;
        let is_single_line = !bytes[paren_start..paren_end.min(len)].contains(&b'\n');

        let mut k = paren_start + 1;
        let mut inner_depth = 0;
        while k < paren_end {
          if bytes[k] == b'(' {
            inner_depth += 1;
          } else if bytes[k] == b')' {
            if inner_depth > 0 {
              inner_depth -= 1;
            }
          } else if bytes[k] == b'\'' || bytes[k] == b'"' {
            let q = bytes[k];
            k += 1;
            while k < paren_end && bytes[k] != q {
              if bytes[k] == b'\\' {
                k += 1;
              }
              k += 1;
            }
          } else if bytes[k] == b',' && inner_depth == 0 {
            let comma_pos = k;
            let has_space_before = comma_pos > 0 && bytes[comma_pos - 1] == b' ';

            let violation = match option {
              "always" => !has_space_before,
              "never" => has_space_before,
              "always-single-line" => is_single_line && !has_space_before,
              "never-single-line" => is_single_line && has_space_before,
              _ => false,
            };

            if violation {
              let msg = match option {
                "always" => "Expected single space before \",\"",
                "never" => "Unexpected whitespace before \",\"",
                "always-single-line" => {
                  "Expected single space before \",\" in a single-line function"
                }
                "never-single-line" => {
                  "Unexpected whitespace before \",\" in a single-line function"
                }
                _ => "Expected single space before \",\"",
              };
              diagnostics.push(Diagnostic::new(NAME, msg).span(Span::new(comma_pos, 1)));
            }
          }
          k += 1;
        }
        // Advance past the opening paren only — do NOT skip to paren_end.
        // This lets nested function calls be individually detected and checked.
        i = paren_start + 1;
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

  use crate::testing::{configured, ctx_with_source_and_options, invalid_options};

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StylisticFunctionCommaSpaceBefore, opts.clone());
    let ctx = ctx_with_source_and_options(source, &opts);
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn never_accepts_no_space_before_comma() {
    let d = check("a { transform: translate(1px, 2px); }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_before_comma() {
    let d = check("a { transform: translate(1px , 2px); }", "never");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn always_accepts_space_before_comma() {
    let d = check("a { transform: translate(1px ,2px); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_before_comma() {
    let d = check("a { transform: translate(1px,2px); }", "always");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-single-line")).is_empty());
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["always", {"ignoreFunctions": ["/^my-/", "url"]}])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/function-comma-space-before\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", {"ignoreFunctionsX": ["url"]}])
      ),
      [
        "Invalid option name \"ignoreFunctionsX\" for rule \"@stylistic/function-comma-space-before\""
      ]
    );
  }

  #[test]
  fn single_line_options_check_single_line_functions_only() {
    use gale_css_parser::Syntax;

    let d = crate::testing::lint(
      NAME,
      serde_json::json!("always-single-line"),
      "a { color: rgb(0, 0 ,0); }",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
      messages,
      ["Expected single space before \",\" in a single-line function"] as [&str; 1]
    );
    let d = crate::testing::lint(
      NAME,
      serde_json::json!("always-single-line"),
      "a { color: rgb(0,\n  0, 0); }",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, [] as [&str; 0]);
    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-single-line"),
      "a { color: rgb(0 , 0,0); }",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
      messages,
      ["Unexpected whitespace before \",\" in a single-line function"] as [&str; 1]
    );
    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-single-line"),
      "a { color: rgb(0 ,\n  0, 0); }",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, [] as [&str; 0]);
  }
}
