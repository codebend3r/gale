use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Enforce lowercase hex colors (e.g. `#ABC` → `#abc`).
///
/// Equivalent to Stylelint's `color-hex-case` rule with "lower" option.
pub struct ColorHexCase;

impl Rule for ColorHexCase {
  fn name(&self) -> &'static str {
    "color-hex-case"
  }

  fn description(&self) -> &'static str {
    "Enforce lowercase for hex colors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();
    for decl in &rule.declarations {
      let decl_start = decl.span.offset;
      let decl_end = decl_start + decl.span.length;
      let search_area = if decl_end <= ctx.source.len() && decl_start < decl_end {
        &ctx.source[decl_start..decl_end]
      } else {
        &decl.value
      };

      for (rel_offset, hex) in find_hex_colors(search_area) {
        if has_uppercase(&hex) {
          let lower = hex.to_ascii_lowercase();
          let abs_offset = if decl_end <= ctx.source.len() && decl_start < decl_end {
            decl_start + rel_offset
          } else {
            decl_start
          };
          diags.push(
            Diagnostic::new(self.name(), format!("Expected \"{hex}\" to be \"{lower}\""))
              .severity(self.default_severity())
              .span(Span::new(abs_offset, hex.len()))
              .fix(Fix::new(
                format!("Convert to lowercase \"{lower}\""),
                vec![Edit::new(Span::new(abs_offset, hex.len()), &lower)],
              )),
          );
        }
      }
    }
    diags
  }
}

fn find_hex_colors(value: &str) -> Vec<(usize, String)> {
  let mut colors = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i < len {
    if bytes[i] == b'#' {
      let start = i;
      i += 1;
      while i < len && (bytes[i] as char).is_ascii_hexdigit() {
        i += 1;
      }
      if i > start + 1 {
        colors.push((start, value[start..i].to_string()));
      }
    } else {
      i += 1;
    }
  }
  colors
}

fn has_uppercase(hex: &str) -> bool {
  hex[1..].chars().any(|c| c.is_ascii_uppercase())
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
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_uppercase_hex() {
    let d = ColorHexCase.check(&style_with_value("#FFFFFF"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffff"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_lowercase_hex() {
    let d = ColorHexCase.check(&style_with_value("#abc"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_mixed_case_hex() {
    let d = ColorHexCase.check(&style_with_value("#aBcDeF"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#abcdef"));
  }
}
