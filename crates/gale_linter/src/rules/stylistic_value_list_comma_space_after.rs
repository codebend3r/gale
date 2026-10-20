use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/value-list-comma-space-after";

declare_rule! {
  /// Require or disallow a space after the commas of value lists.
  ///
  /// Primary: "always" | "never" | "always-single-line"
  pub struct StylisticValueListCommaSpaceAfter => StylisticValueListCommaSpaceAfterCheck {
    name: NAME,
    description: "Require or disallow a space after the commas of value lists",
    default_severity: Warning,
  }
}

/// `@stylistic/value-list-comma-space-after` as configured.
pub struct StylisticValueListCommaSpaceAfterCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticValueListCommaSpaceAfterCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.  Gale reads no option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
    })
  }
}

impl Check for StylisticValueListCommaSpaceAfterCheck {
  /// Flags a value list comma whose trailing space does not match the option.
  /// Commas inside function arguments belong to the function-comma rules.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    // Track brace depth to only check inside declaration blocks
    let mut brace_depth = 0;
    // Track paren depth to skip function arguments (handled by function-comma rules)
    let mut paren_depth = 0;
    // Track whether we're inside a declaration value (after `:`, before `;` or `}`)
    let mut in_value = false;

    while i < len {
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

      match bytes[i] {
        b'{' => {
          brace_depth += 1;
          in_value = false;
        }
        b'}' => {
          if brace_depth > 0 {
            brace_depth -= 1;
          }
          in_value = false;
        }
        b';' | b'\n' => {
          in_value = false;
        }
        b':' if brace_depth > 0 && paren_depth == 0 => {
          // Only set in_value if this colon is preceded by a
          // property name, not a pseudo-selector like :hover.
          // Check: is the previous non-ws char part of an identifier
          // that is NOT preceded by another `:`?
          let mut is_property = false;
          if i > 0 {
            let mut k = i - 1;
            while k > 0 && (bytes[k] == b' ' || bytes[k] == b'\t') {
              k -= 1;
            }
            if bytes[k].is_ascii_alphanumeric() || bytes[k] == b'-' || bytes[k] == b'_' {
              // Walk back through identifier
              while k > 0
                && (bytes[k - 1].is_ascii_alphanumeric()
                  || bytes[k - 1] == b'-'
                  || bytes[k - 1] == b'_')
              {
                k -= 1;
              }
              // Skip whitespace before the identifier
              let mut pre = if k > 0 { k - 1 } else { 0 };
              while pre > 0
                && (bytes[pre] == b' '
                  || bytes[pre] == b'\t'
                  || bytes[pre] == b'\n'
                  || bytes[pre] == b'\r')
              {
                pre -= 1;
              }
              // It's a property if preceded by '{', ';', or start
              is_property = k == 0 || bytes[pre] == b'{' || bytes[pre] == b';' || pre == 0;
            }
          }
          if is_property {
            in_value = true;
          }
        }
        b'(' => paren_depth += 1,
        b')' => {
          if paren_depth > 0 {
            paren_depth -= 1;
          }
        }
        // Only check commas inside declaration values, not selector commas
        b',' if in_value && paren_depth == 0 => {
          let comma_pos = i;
          let after = comma_pos + 1;
          let has_space = after < len && bytes[after] == b' ';

          // For always-single-line, check if current declaration is single-line
          let is_single_line = if option == "always-single-line" {
            // Find the declaration boundaries (from last ; or { to next ; or })
            let mut start = comma_pos;
            while start > 0 && bytes[start] != b';' && bytes[start] != b'{' {
              start -= 1;
            }
            let mut end = comma_pos;
            while end < len && bytes[end] != b';' && bytes[end] != b'}' {
              end += 1;
            }
            !ctx.source[start..end.min(len)].contains('\n')
          } else {
            true
          };

          let violation = match option {
            "always" => !has_space,
            "never" => has_space,
            "always-single-line" => is_single_line && !has_space,
            _ => false,
          };

          if violation {
            let msg = match option {
              "always" | "always-single-line" => "Expected a space after the comma in value list",
              "never" => "Unexpected space after the comma in value list",
              _ => "Expected a space after the comma in value list",
            };
            diagnostics.push(Diagnostic::new(NAME, msg).span(Span::new(comma_pos, 1)));
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

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StylisticValueListCommaSpaceAfter, opts.clone());
    let ctx = ctx_with_source_and_options(source, &opts);
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn always_accepts_space_after_comma() {
    let d = check("a { font-family: Arial, sans-serif; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_after_comma() {
    let d = check("a { font-family: Arial,sans-serif; }", "always");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn never_accepts_no_space() {
    let d = check("a { font-family: Arial,sans-serif; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space() {
    let d = check("a { font-family: Arial, sans-serif; }", "never");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_function_commas() {
    // Commas inside functions should not be flagged by this rule
    let d = check("a { background: rgb(0,0,0); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-single-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/value-list-comma-space-after\""]
    );
  }
}
