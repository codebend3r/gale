use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow trailing zeros in numbers.
///
/// Equivalent to `@stylistic/number-no-trailing-zeros`.
pub struct StylisticNumberNoTrailingZeros;

impl Rule for StylisticNumberNoTrailingZeros {
  fn name(&self) -> &'static str {
    "@stylistic/number-no-trailing-zeros"
  }

  fn description(&self) -> &'static str {
    "Disallow trailing zeros in numbers"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      find_trailing_zeros(&decl.value, decl.span.offset, self, &mut diags);
    }
    diags
  }
}

fn find_trailing_zeros(
  value: &str,
  base_offset: usize,
  rule: &StylisticNumberNoTrailingZeros,
  diags: &mut Vec<Diagnostic>,
) {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Skip url() contents — they contain encoded strings, not CSS numbers
    if i + 4 <= len && bytes[i..].starts_with(b"url(") {
      i += 4;
      let mut depth = 1;
      while i < len && depth > 0 {
        match bytes[i] {
          b'(' => depth += 1,
          b')' => depth -= 1,
          b'"' | b'\'' => {
            let q = bytes[i];
            i += 1;
            while i < len && bytes[i] != q {
              if bytes[i] == b'\\' && i + 1 < len {
                i += 1;
              }
              i += 1;
            }
          }
          _ => {}
        }
        if depth > 0 {
          i += 1;
        }
      }
      continue;
    }

    // Look for start of a number
    if bytes[i].is_ascii_digit()
      || (bytes[i] == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit())
    {
      let num_start = i;

      // Skip integer part
      while i < len && bytes[i].is_ascii_digit() {
        i += 1;
      }

      // Check for decimal point
      if i < len && bytes[i] == b'.' {
        i += 1; // skip the dot
        let frac_start = i;

        // Skip fractional digits
        while i < len && bytes[i].is_ascii_digit() {
          i += 1;
        }

        let frac_end = i;
        if frac_end > frac_start {
          let number = &value[num_start..frac_end];

          // Check for trailing zeros
          let trimmed = number.trim_end_matches('0');
          // Also remove trailing dot if all fractional digits were zeros
          let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);

          if trimmed.len() < number.len() {
            let fixed = if trimmed.contains('.') {
              trimmed.to_string()
            } else {
              trimmed.to_string()
            };

            diags.push(
              Diagnostic::new(
                rule.name(),
                format!("Unexpected trailing zero(s) in \"{number}\""),
              )
              .severity(rule.default_severity())
              .span(Span::new(base_offset + num_start, number.len()))
              .fix(Fix::new(
                "Remove trailing zeros",
                vec![Edit::new(
                  Span::new(base_offset + num_start, number.len()),
                  &fixed,
                )],
              )),
            );
          }
        }
      }
      continue;
    }
    i += 1;
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
        property: "opacity".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, val.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_trailing_zero() {
    let rule = StylisticNumberNoTrailingZeros;
    let d = rule.check(&style_with_value("1.0"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1.0"));
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits[0].new_text, "1");
  }

  #[test]
  fn reports_multiple_trailing_zeros() {
    let rule = StylisticNumberNoTrailingZeros;
    let d = rule.check(&style_with_value("0.500"), &ctx());
    assert_eq!(d.len(), 1);
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits[0].new_text, "0.5");
  }

  #[test]
  fn allows_no_trailing_zeros() {
    let rule = StylisticNumberNoTrailingZeros;
    let d = rule.check(&style_with_value("0.5"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_integer() {
    let rule = StylisticNumberNoTrailingZeros;
    let d = rule.check(&style_with_value("10"), &ctx());
    assert!(d.is_empty());
  }
}
