use gale_css_parser::CssNode;
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
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mode = self.mode;

    let mut diags = Vec::new();

    for decl in declarations {
      let value = &decl.value;
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
                  .span(Span::new(decl.span.offset + start, 1 + hex_len)),
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
                  .span(Span::new(decl.span.offset + start, 1 + hex_len)),
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, val.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn never_flags_four_digit_hex() {
    let rule = configured(&ColorHexAlpha, serde_json::json!("never"));
    let ctx = ctx();
    let d = rule.check(&style_with_value("#ffff"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffff"));
  }

  #[test]
  fn never_flags_eight_digit_hex() {
    let rule = configured(&ColorHexAlpha, serde_json::json!("never"));
    let ctx = ctx();
    let d = rule.check(&style_with_value("#ffffffff"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffffff"));
  }

  #[test]
  fn never_allows_three_digit_hex() {
    let rule = configured(&ColorHexAlpha, serde_json::json!("never"));
    let ctx = ctx();
    let d = rule.check(&style_with_value("#fff"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn never_allows_six_digit_hex() {
    let rule = configured(&ColorHexAlpha, serde_json::json!("never"));
    let ctx = ctx();
    let d = rule.check(&style_with_value("#ffffff"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn always_flags_three_digit_hex() {
    let rule = configured(&ColorHexAlpha, serde_json::json!("always"));
    let ctx = ctx();
    let d = rule.check(&style_with_value("#fff"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected alpha"));
  }

  #[test]
  fn always_allows_eight_digit_hex() {
    let rule = configured(&ColorHexAlpha, serde_json::json!("always"));
    let ctx = ctx();
    let d = rule.check(&style_with_value("#ffffffff"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn default_is_never() {
    let rule = configured(&ColorHexAlpha, None);
    let ctx = ctx();
    let d = rule.check(&style_with_value("#ffff"), &ctx);
    assert_eq!(d.len(), 1);
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
