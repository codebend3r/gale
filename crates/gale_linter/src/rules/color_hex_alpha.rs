use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow alpha channel in hex colors.
///
/// Options: `"always"` requires alpha (8 or 4 digit hex), `"never"` (default)
/// disallows alpha (flags 4 and 8 digit hex colors).
///
/// Equivalent to Stylelint's `color-hex-alpha` rule.
pub struct ColorHexAlpha;

impl Rule for ColorHexAlpha {
  fn name(&self) -> &'static str {
    "color-hex-alpha"
  }

  fn description(&self) -> &'static str {
    "Require or disallow alpha channel in hex colors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mode = ctx.primary_option_str().unwrap_or("never");

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
                    self.name(),
                    format!("Unexpected alpha channel in hex color \"{hex_str}\""),
                  )
                  .severity(self.default_severity())
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
                    self.name(),
                    format!("Expected alpha channel in hex color \"{hex_str}\""),
                  )
                  .severity(self.default_severity())
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_options(options: Option<serde_json::Value>) -> RuleContext<'static> {
    let opts: Option<&'static serde_json::Value> = options.map(|v| &*Box::leak(Box::new(v)));
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: opts,
    }
  }

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
    let ctx = ctx_with_options(Some(serde_json::json!("never")));
    let d = ColorHexAlpha.check(&style_with_value("#ffff"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffff"));
  }

  #[test]
  fn never_flags_eight_digit_hex() {
    let ctx = ctx_with_options(Some(serde_json::json!("never")));
    let d = ColorHexAlpha.check(&style_with_value("#ffffffff"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffffff"));
  }

  #[test]
  fn never_allows_three_digit_hex() {
    let ctx = ctx_with_options(Some(serde_json::json!("never")));
    let d = ColorHexAlpha.check(&style_with_value("#fff"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn never_allows_six_digit_hex() {
    let ctx = ctx_with_options(Some(serde_json::json!("never")));
    let d = ColorHexAlpha.check(&style_with_value("#ffffff"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn always_flags_three_digit_hex() {
    let ctx = ctx_with_options(Some(serde_json::json!("always")));
    let d = ColorHexAlpha.check(&style_with_value("#fff"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected alpha"));
  }

  #[test]
  fn always_allows_eight_digit_hex() {
    let ctx = ctx_with_options(Some(serde_json::json!("always")));
    let d = ColorHexAlpha.check(&style_with_value("#ffffffff"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn default_is_never() {
    let ctx = ctx_with_options(None);
    let d = ColorHexAlpha.check(&style_with_value("#ffff"), &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(ColorHexAlpha.name(), "color-hex-alpha");
  }
}
