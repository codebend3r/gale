use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "color-hex-alpha";

declare_rule! {
  /// Require or disallow alpha channel in hex colors.
  ///
  /// Options: `"always"` requires alpha (8 or 4 digit hex), `"never"` (default)
  /// disallows alpha (flags 4 and 8 digit hex colors).
  ///
  /// Equivalent to Stylelint's `color-hex-alpha` rule.
  pub struct ColorHexAlpha => ColorHexAlphaCheck {
    name: NAME,
    description: "Require or disallow alpha channel in hex colors",
    default_severity: Warning,
  }
}

/// `color-hex-alpha` as configured.
pub struct ColorHexAlphaCheck {
  /// `"always"` or `"never"`.
  mode: &'static str,
}

impl ColorHexAlphaCheck {
  /// Stylelint's option: `"always"` or `"never"`.  Gale reads no option as
  /// `"never"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let mode = options.primary_one_of(&["always", "never"]);
    options.finish()?;
    Ok(Self {
      mode: mode.unwrap_or("never"),
    })
  }
}

impl Check for ColorHexAlphaCheck {
  /// Flags hex colors whose digit count carries an alpha channel under "never",
  /// or lacks one under "always".
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mode = self.mode;

    let mut diags = Vec::new();

    for decl in tree.walk_decls() {
      let value = &*decl.raw_value();
      let value_start = decl.value_start();
      let bytes = value.as_bytes();
      let len = bytes.len();
      let mut i = 0;

      while i < len {
        if bytes[i] == b'#' {
          let start = i;
          i += 1;
          let hex_start = i;
          while i < len && bytes[i].is_ascii_hexdigit() {
            i += 1;
          }
          let hex_len = i - hex_start;

          // Valid hex lengths: 3, 4, 6, 8
          match mode {
            "never" => {
              // 4 and 8 digit hex have alpha — flag them
              if hex_len == 4 || hex_len == 8 {
                let hex_str = &value[start..start + 1 + hex_len];
                diags.push(
                  Diagnostic::new(
                    NAME,
                    format!("Unexpected alpha channel in hex color \"{hex_str}\""),
                  )
                  .span(Span::new(value_start + start, 1 + hex_len)),
                );
              }
            }
            "always" => {
              // 3 and 6 digit hex lack alpha — flag them
              if hex_len == 3 || hex_len == 6 {
                let hex_str = &value[start..start + 1 + hex_len];
                diags.push(
                  Diagnostic::new(
                    NAME,
                    format!("Expected alpha channel in hex color \"{hex_str}\""),
                  )
                  .span(Span::new(value_start + start, 1 + hex_len)),
                );
              }
            }
            _ => {}
          }
        } else {
          i += 1;
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(options: Option<serde_json::Value>, value: &str) -> Vec<Diagnostic> {
    let source = format!("a {{ color: {value}; }}");
    configured(&ColorHexAlpha, options).check_source(&ctx_with_source(&source))
  }

  #[test]
  fn never_flags_four_digit_hex() {
    let d = check(Some(serde_json::json!("never")), "#ffff");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffff"));
    assert_eq!(d[0].span, Span::new(11, 5));
  }

  #[test]
  fn never_flags_eight_digit_hex() {
    let d = check(Some(serde_json::json!("never")), "#ffffffff");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffffff"));
  }

  #[test]
  fn never_allows_three_digit_hex() {
    assert!(check(Some(serde_json::json!("never")), "#fff").is_empty());
  }

  #[test]
  fn never_allows_six_digit_hex() {
    assert!(check(Some(serde_json::json!("never")), "#ffffff").is_empty());
  }

  #[test]
  fn always_flags_three_digit_hex() {
    let d = check(Some(serde_json::json!("always")), "#fff");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected alpha"));
  }

  #[test]
  fn always_allows_eight_digit_hex() {
    assert!(check(Some(serde_json::json!("always")), "#ffffffff").is_empty());
  }

  #[test]
  fn default_is_never() {
    assert_eq!(check(None, "#ffff").len(), 1);
  }

  #[test]
  fn reads_the_value_as_written() {
    // The CSS parser used to shorten `#ffffffff` and spell `white` as `#fff`.
    assert_eq!(check(Some(serde_json::json!("always")), "white").len(), 0);
    let d = check(Some(serde_json::json!("never")), "#FFFFFFFF");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#FFFFFFFF"));
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(ColorHexAlpha::META.name, "color-hex-alpha");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"color-hex-alpha\""]
    );
  }
}
