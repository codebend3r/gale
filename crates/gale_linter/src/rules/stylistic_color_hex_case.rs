use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify lowercase or uppercase for hex colors (@stylistic/ version).
///
/// Equivalent to `@stylistic/color-hex-case`. Supports both "lower" and "upper"
/// primary options, unlike the core `color-hex-case` which only enforces lowercase.
pub struct StylisticColorHexCase;

impl Rule for StylisticColorHexCase {
  fn name(&self) -> &'static str {
    "@stylistic/color-hex-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for hex colors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let expected_case = ctx.primary_option_str().unwrap_or("lower");

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
        let is_wrong = match expected_case {
          "lower" => hex[1..].chars().any(|c| c.is_ascii_uppercase()),
          "upper" => hex[1..].chars().any(|c| c.is_ascii_lowercase()),
          _ => false,
        };

        if is_wrong {
          let fixed = match expected_case {
            "lower" => hex.to_ascii_lowercase(),
            "upper" => {
              // Keep the '#' as-is, uppercase the rest
              let mut s = String::with_capacity(hex.len());
              s.push('#');
              for c in hex[1..].chars() {
                s.extend(c.to_uppercase());
              }
              s
            }
            _ => continue,
          };
          let abs_offset = if decl_end <= ctx.source.len() && decl_start < decl_end {
            decl_start + rel_offset
          } else {
            decl_start
          };
          diags.push(
            Diagnostic::new(self.name(), format!("Expected \"{hex}\" to be \"{fixed}\""))
              .severity(self.default_severity())
              .span(Span::new(abs_offset, hex.len()))
              .fix(Fix::new(
                format!("Convert to {expected_case}case"),
                vec![Edit::new(Span::new(abs_offset, hex.len()), &fixed)],
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
  fn reports_uppercase_hex_default_lower() {
    let d = StylisticColorHexCase.check(&style_with_value("#FFFFFF"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffff"));
  }

  #[test]
  fn allows_lowercase_hex() {
    let d = StylisticColorHexCase.check(&style_with_value("#abc"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_mixed_case() {
    let d = StylisticColorHexCase.check(&style_with_value("#aBcDeF"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#abcdef"));
  }
}
