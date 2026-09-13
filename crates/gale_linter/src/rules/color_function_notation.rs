use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Prefer modern color function notation (space-separated) over legacy (comma-separated).
///
/// Equivalent to Stylelint's `color-function-notation` rule with "modern" option.
/// Detects comma-separated arguments in rgb/rgba/hsl/hsla.
pub struct ColorFunctionNotation;

const COLOR_FUNCTIONS: &[&str] = &["rgb(", "rgba(", "hsl(", "hsla("];

/// Find the position of the matching closing paren, handling nesting.
fn find_matching_paren(s: &str) -> Option<usize> {
  let mut depth: i32 = 1;
  for (i, ch) in s.char_indices() {
    match ch {
      '(' => depth += 1,
      ')' => {
        depth -= 1;
        if depth == 0 {
          return Some(i);
        }
      }
      _ => {}
    }
  }
  None
}

impl Rule for ColorFunctionNotation {
  fn name(&self) -> &'static str {
    "color-function-notation"
  }

  fn description(&self) -> &'static str {
    "Specify modern or legacy notation for color functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    // Read secondary options for `ignore: ["with-var-inside"]`
    let ignore_with_var = ctx
      .secondary_options()
      .or(ctx.options)
      .and_then(|v| v.get("ignore"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .any(|item| item.as_str() == Some("with-var-inside"))
      })
      .unwrap_or(false);

    let primary = ctx.primary_option_str().unwrap_or("modern");

    let mut diags = Vec::new();
    let mut seen_offsets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for decl in decls {
      // Use source text to avoid lightningcss normalization
      let decl_start = decl.span.offset;
      let decl_end = (decl_start + decl.span.length).min(ctx.source.len());
      let search_area = if decl_end > decl_start && decl_end <= ctx.source.len() {
        &ctx.source[decl_start..decl_end]
      } else {
        &decl.value
      };
      let lower = search_area.to_ascii_lowercase();

      for &func in COLOR_FUNCTIONS {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(func) {
          let abs_pos = search_from + pos;
          // Skip if preceded by an ident char (e.g. `hsv-to-rgb(` is not `rgb(`).
          if abs_pos > 0 {
            let prev = lower.as_bytes()[abs_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_' {
              search_from = abs_pos + 1;
              continue;
            }
          }
          let args_start = abs_pos + func.len();
          if let Some(close) = find_matching_paren(&lower[args_start..]) {
            let args = &search_area[args_start..args_start + close];
            let has_commas = args.contains(',');

            let should_report = match primary {
              "legacy" => !has_commas && !args.trim().is_empty(),
              // "modern" (default)
              _ => has_commas,
            };

            if should_report {
              // Skip if ignore: ["with-var-inside"] and args contain var(
              if ignore_with_var && args.to_ascii_lowercase().contains("var(") {
                search_from = abs_pos + 1;
                continue;
              }
              let abs_offset = decl_start + abs_pos;
              // Deduplicate: keyframe declarations may have
              // overlapping source spans, producing duplicates.
              if seen_offsets.insert(abs_offset) {
                let fn_name = &func[..func.len() - 1];
                let msg = if primary == "legacy" {
                  "Expected legacy color-function notation".to_string()
                } else {
                  "Expected modern color-function notation".to_string()
                };
                diags.push(
                  Diagnostic::new(self.name(), msg)
                    .severity(self.default_severity())
                    .span(Span::new(abs_offset, fn_name.len())),
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
  fn reports_legacy_rgb() {
    let d = ColorFunctionNotation.check(&style_with_value("rgb(0, 0, 0)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color-function notation"));
  }

  #[test]
  fn allows_modern_rgb() {
    let d = ColorFunctionNotation.check(&style_with_value("rgb(0 0 0)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_legacy_hsl() {
    let d = ColorFunctionNotation.check(&style_with_value("hsl(0, 100%, 50%)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color-function notation"));
  }

  #[test]
  fn allows_modern_hsl() {
    let d = ColorFunctionNotation.check(&style_with_value("hsl(0 100% 50%)"), &ctx());
    assert!(d.is_empty());
  }
}

#[cfg(test)]
#[test]
fn debug_detect_rgb_comma() {
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};
  let rule = ColorFunctionNotation;
  let source = ":root {\n  --my-color: rgb(248, 248, 247);\n  color: rgb(100, 200, 50);\n}\n";
  let node = CssNode::Style(StyleRule {
    selector: ":root".to_string(),
    declarations: vec![
      Declaration {
        property: "--my-color".to_string(),
        value: "#f8f8f7".to_string(),
        span: ParserSpan::new(10, 31),
        important: false,
      },
      Declaration {
        property: "color".to_string(),
        value: "#64c832".to_string(),
        span: ParserSpan::new(44, 25),
        important: false,
      },
    ],
    span: ParserSpan::new(0, source.len()),
    ..Default::default()
  });
  let ctx = RuleContext {
    file_path: "t.css",
    source,
    syntax: Syntax::Css,
    options: None,
  };
  let d = rule.check(&node, &ctx);
  eprintln!("Diagnostics: {:?}", d.len());
  for diag in &d {
    eprintln!("  {}: {}", diag.rule_name, diag.message);
  }
  assert!(
    d.len() >= 2,
    "Expected at least 2 diagnostics, got {}",
    d.len()
  );
}
