use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow hex colors entirely.
///
/// Any hex color (`#` followed by hex digits) in a declaration value is flagged.
///
/// Equivalent to Stylelint's `color-no-hex` rule.
pub struct ColorNoHex;

impl Rule for ColorNoHex {
  fn name(&self) -> &'static str {
    "color-no-hex"
  }

  fn description(&self) -> &'static str {
    "Disallow hex colors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

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

          // Only flag valid hex color lengths: 3, 4, 6, 8
          if matches!(hex_len, 3 | 4 | 6 | 8) {
            let hex_str = &value[start..start + 1 + hex_len];
            diags.push(
              Diagnostic::new(self.name(), format!("Unexpected hex color \"{hex_str}\""))
                .severity(self.default_severity())
                .span(Span::new(decl.span.offset + start, 1 + hex_len)),
            );
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

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
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
  fn flags_three_digit_hex() {
    let d = ColorNoHex.check(&style_with_value("#fff"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#fff"));
  }

  #[test]
  fn flags_six_digit_hex() {
    let d = ColorNoHex.check(&style_with_value("#ff0000"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ff0000"));
  }

  #[test]
  fn allows_named_colors() {
    let d = ColorNoHex.check(&style_with_value("red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_rgb_function() {
    let d = ColorNoHex.check(&style_with_value("rgb(255, 0, 0)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(ColorNoHex.name(), "color-no-hex");
  }
}
