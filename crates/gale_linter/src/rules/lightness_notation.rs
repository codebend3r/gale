use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require number or percentage notation for lightness values in color functions.
///
/// In "percentage" mode (default), flags plain numbers used as lightness in
/// `hsl()`, `hsla()`, `lch()`, `oklch()`, `lab()`, `oklab()`.
///
/// Equivalent to Stylelint's `lightness-notation` rule with "percentage" option.
pub struct LightnessNotation;

/// Color functions and the (0-based) index of the lightness argument.
/// For `hsl`/`hsla`: lightness is the 3rd argument (index 2).
/// For `lch`/`oklch`/`lab`/`oklab`: lightness is the 1st argument (index 0).
const LIGHTNESS_FUNCTIONS: &[(&str, usize)] = &[
  ("hsl(", 2),
  ("hsla(", 2),
  ("lch(", 0),
  ("oklch(", 0),
  ("lab(", 0),
  ("oklab(", 0),
];

impl Rule for LightnessNotation {
  fn name(&self) -> &'static str {
    "lightness-notation"
  }

  fn description(&self) -> &'static str {
    "Specify number or percentage notation for lightness values"
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
      let lower = decl.value.to_ascii_lowercase();
      for &(func, lightness_idx) in LIGHTNESS_FUNCTIONS {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(func) {
          let abs_pos = search_from + pos;
          // Avoid matching `lch(` inside `oklch(` or `lab(` inside `oklab(`
          if abs_pos > 0 && lower.as_bytes()[abs_pos - 1].is_ascii_alphabetic() {
            search_from = abs_pos + 1;
            continue;
          }
          let args_start = abs_pos + func.len();
          if let Some(close) = lower[args_start..].find(')') {
            let args = &decl.value[args_start..args_start + close];
            // Split arguments: comma-separated (legacy) or space-separated (modern).
            // For modern syntax with `/`, only consider the part before `/`.
            let args_before_slash = if let Some(slash_pos) = args.find('/') {
              &args[..slash_pos]
            } else {
              args
            };
            let parts: Vec<&str> = if args_before_slash.contains(',') {
              args_before_slash.split(',').map(|s| s.trim()).collect()
            } else {
              args_before_slash.split_whitespace().collect()
            };

            if lightness_idx < parts.len() {
              let lightness_val = parts[lightness_idx].trim();
              // In "percentage" mode: flag if it's a plain number (no %).
              if !lightness_val.is_empty()
                && !lightness_val.ends_with('%')
                && is_numeric(lightness_val)
              {
                let fn_name = &func[..func.len() - 1];
                diags.push(
                  Diagnostic::new(
                    self.name(),
                    format!("Expected percentage notation for lightness in {fn_name}()"),
                  )
                  .severity(self.default_severity())
                  .span(Span::new(decl.span.offset, decl.span.length)),
                );
              }
            }
          }
          search_from = abs_pos + 1;
        }
      }
    }
    diags
  }
}

/// Check if a string is a plain numeric value (integer or decimal, possibly with sign).
fn is_numeric(s: &str) -> bool {
  if s.is_empty() {
    return false;
  }
  let mut has_digit = false;
  let mut has_dot = false;
  for (i, c) in s.chars().enumerate() {
    if c == '.' {
      if has_dot {
        return false;
      }
      has_dot = true;
    } else if c.is_ascii_digit() {
      has_digit = true;
    } else if c == '-' || c == '+' {
      if i != 0 {
        return false;
      }
    } else {
      return false;
    }
  }
  has_digit
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

  fn style_with_value(value: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: value.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_number_lightness_in_hsl() {
    let d = LightnessNotation.check(&style_with_value("hsl(0, 100%, 50)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("hsl()"));
  }

  #[test]
  fn allows_percentage_lightness_in_hsl() {
    let d = LightnessNotation.check(&style_with_value("hsl(0, 100%, 50%)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_number_lightness_in_lch() {
    let d = LightnessNotation.check(&style_with_value("lch(50 30 120)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("lch()"));
  }

  #[test]
  fn allows_percentage_lightness_in_lch() {
    let d = LightnessNotation.check(&style_with_value("lch(50% 30 120)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_number_lightness_in_oklch() {
    let d = LightnessNotation.check(&style_with_value("oklch(0.5 0.2 120)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("oklch()"));
  }

  #[test]
  fn allows_percentage_lightness_in_oklch() {
    let d = LightnessNotation.check(&style_with_value("oklch(50% 0.2 120)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_number_lightness_in_lab() {
    let d = LightnessNotation.check(&style_with_value("lab(50 -20 40)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("lab()"));
  }

  #[test]
  fn allows_percentage_lightness_in_oklab() {
    let d = LightnessNotation.check(&style_with_value("oklab(50% -0.1 0.1)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_hsl_modern_syntax_with_percentage() {
    let d = LightnessNotation.check(&style_with_value("hsl(0 100% 50%)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_hsl_modern_syntax_without_percentage() {
    let d = LightnessNotation.check(&style_with_value("hsl(0 100% 50)"), &ctx());
    assert_eq!(d.len(), 1);
  }
}
