use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

pub struct ColorNoInvalidHex;

impl Rule for ColorNoInvalidHex {
  fn name(&self) -> &'static str {
    "color-no-invalid-hex"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid hex colors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();
    for decl in &rule.declarations {
      for hex in find_hex_colors(&decl.value) {
        if !is_valid_hex(&hex) {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!("Unexpected invalid hex color \"{hex}\""),
            )
            .severity(self.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
          );
        }
      }
    }
    diags
  }
}

/// Find hex color candidates in a value string (# followed by hex-like chars).
/// Skips:
/// - Content inside `url(…)` (URLs may contain `#fragment` references)
/// - Content after `//` (SCSS line comments that may leak into the value)
/// - `#{}` SCSS interpolation
fn find_hex_colors(value: &str) -> Vec<String> {
  let mut colors = Vec::new();
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();

  let mut i = 0;
  while i < len {
    // Skip SCSS line comments (// to end of line)
    if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
      while i < len && chars[i] != '\n' {
        i += 1;
      }
      continue;
    }

    // Skip url(…)
    if i + 3 < len
      && chars[i] == 'u'
      && chars[i + 1] == 'r'
      && chars[i + 2] == 'l'
      && chars[i + 3] == '('
    {
      i += 4;
      let mut depth = 1;
      while i < len && depth > 0 {
        match chars[i] {
          '(' => depth += 1,
          ')' => depth -= 1,
          _ => {}
        }
        i += 1;
      }
      continue;
    }

    if chars[i] == '#' {
      // Skip SCSS interpolation #{…}
      if i + 1 < len && chars[i + 1] == '{' {
        i += 2;
        let mut depth = 1;
        while i < len && depth > 0 {
          match chars[i] {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
          }
          i += 1;
        }
        continue;
      }

      let start = i;
      i += 1;
      while i < len && chars[i].is_ascii_alphanumeric() {
        i += 1;
      }
      if i > start + 1 {
        let hex: String = chars[start..i].iter().collect();
        colors.push(hex);
      }
    } else {
      i += 1;
    }
  }

  colors
}

/// Check if a hex color is valid (#RGB, #RGBA, #RRGGBB, #RRGGBBAA).
fn is_valid_hex(hex: &str) -> bool {
  let digits = &hex[1..]; // strip #
  let valid_len = matches!(digits.len(), 3 | 4 | 6 | 8);
  let valid_chars = digits.chars().all(|c| c.is_ascii_hexdigit());
  valid_len && valid_chars
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{CssNode, Declaration, Span as ParserSpan, StyleRule, Syntax};

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
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_invalid_hex_chars() {
    let d = ColorNoInvalidHex.check(&style_with_value("#gg0000"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#gg0000"));
  }

  #[test]
  fn reports_invalid_hex_length() {
    let d = ColorNoInvalidHex.check(&style_with_value("#12345"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_valid_hex() {
    assert!(
      ColorNoInvalidHex
        .check(&style_with_value("#fff"), &ctx())
        .is_empty()
    );
    assert!(
      ColorNoInvalidHex
        .check(&style_with_value("#ff00ff"), &ctx())
        .is_empty()
    );
    assert!(
      ColorNoInvalidHex
        .check(&style_with_value("#ff00ff80"), &ctx())
        .is_empty()
    );
  }
}
