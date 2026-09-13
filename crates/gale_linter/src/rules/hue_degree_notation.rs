use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Prefer degree notation for hue values in `hsl()`/`hsla()`/`hwb()`/`lch()`/`oklch()`.
///
/// Equivalent to Stylelint's `hue-degree-notation` rule with "angle" option.
/// Detects bare numbers used as hue values without the `deg` unit.
pub struct HueDegreeNotation;

/// Functions where hue is the FIRST component argument.
const HUE_FIRST_FUNCTIONS: &[&str] = &["hsl", "hsla", "hwb"];
/// Functions where hue is the THIRD component argument.
const HUE_THIRD_FUNCTIONS: &[&str] = &["lch", "oklch"];

impl Rule for HueDegreeNotation {
  fn name(&self) -> &'static str {
    "hue-degree-notation"
  }

  fn description(&self) -> &'static str {
    "Specify number or angle notation for degree hues"
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

    let primary = ctx.primary_option_str().unwrap_or("angle");

    let mut diags = Vec::new();
    let mut seen_offsets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for decl in decls {
      // Use source text for accurate detection (lightningcss normalizes values).
      let decl_start = decl.span.offset;
      let decl_end = decl_start + decl.span.length;
      let search_area = if decl_end <= ctx.source.len() && decl_start < decl_end {
        &ctx.source[decl_start..decl_end]
      } else {
        &decl.value
      };

      check_hue_in_text(self, search_area, decl_start, primary, &mut diags);
    }
    // Deduplicate: keyframe declarations may have overlapping source spans.
    diags.retain(|d| seen_offsets.insert(d.span.offset));
    diags
  }
}

fn check_hue_in_text(
  rule: &HueDegreeNotation,
  text: &str,
  base_offset: usize,
  primary: &str,
  diags: &mut Vec<Diagnostic>,
) {
  let lower = text.to_ascii_lowercase();

  // Check all hue-containing functions
  for &func_name in HUE_FIRST_FUNCTIONS.iter().chain(HUE_THIRD_FUNCTIONS.iter()) {
    let pattern = format!("{func_name}(");
    let is_hue_first = HUE_FIRST_FUNCTIONS.contains(&func_name);
    let hue_index = if is_hue_first { 0 } else { 2 }; // 3rd component for lch/oklch

    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find(&pattern) {
      let abs_pos = search_from + pos;
      let args_start = abs_pos + pattern.len();

      if let Some(close_offset) = find_matching_paren(text, args_start) {
        let args = &text[args_start..close_offset];

        // Check for relative color syntax: `hsl(from ...)` - skip the `from` keyword
        // and the origin color, so channels start from index 2 instead of 0.
        let args_lower = args.trim_start().to_ascii_lowercase();
        let is_relative = args_lower.starts_with("from ");

        let effective_index = if is_relative {
          hue_index + 2
        } else {
          hue_index
        };

        if let Some((hue_val, hue_offset_in_args)) = extract_arg_at_index(args, effective_index) {
          let trimmed = hue_val.trim();
          if !trimmed.is_empty() && !contains_variable(trimmed) {
            let should_report = match primary {
              "number" => {
                // Expect bare number, flag if has deg unit
                has_deg_unit(trimmed)
              }
              // "angle" (default)
              _ => {
                // Expect deg unit, flag if bare number
                is_bare_number(trimmed)
              }
            };

            if should_report {
              let abs_offset = base_offset + args_start + hue_offset_in_args;
              let (unfixed, fixed) = if primary == "number" {
                // Remove deg suffix
                let num = trimmed
                  .strip_suffix("deg")
                  .or_else(|| trimmed.strip_suffix("Deg"))
                  .or_else(|| trimmed.strip_suffix("DEG"))
                  .unwrap_or(trimmed);
                (trimmed.to_string(), num.to_string())
              } else {
                (trimmed.to_string(), format!("{trimmed}deg"))
              };
              diags.push(
                Diagnostic::new(
                  rule.name(),
                  format!("Expected \"{unfixed}\" to be \"{fixed}\""),
                )
                .severity(rule.default_severity())
                .span(Span::new(abs_offset, trimmed.len())),
              );
            }
          }
        }
        search_from = close_offset;
      } else {
        search_from = abs_pos + 1;
      }
    }
  }
}

/// Extract the argument at a given index from function args.
/// Arguments are separated by commas (legacy syntax) or whitespace (modern syntax).
/// For modern syntax with `/`, only consider tokens before the `/`.
/// Returns (value_str, offset_within_args).
fn extract_arg_at_index(args: &str, index: usize) -> Option<(&str, usize)> {
  if args.contains(',') {
    // Legacy comma syntax: split by commas at depth 0
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, b) in args.bytes().enumerate() {
      match b {
        b'(' => depth += 1,
        b')' => depth -= 1,
        b',' if depth == 0 => {
          parts.push((start, &args[start..i]));
          start = i + 1;
        }
        _ => {}
      }
    }
    parts.push((start, &args[start..]));

    if let Some(&(offset, part)) = parts.get(index) {
      let trimmed = part.trim();
      let trim_offset = part.len() - part.trim_start().len();
      return Some((trimmed, offset + trim_offset));
    }
  } else {
    // Modern space-separated syntax
    // Split by whitespace, but stop at `/` (alpha separator)
    let mut tokens = Vec::new();
    let bytes = args.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut depth = 0;

    while i < len {
      // Skip whitespace
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }
      if i >= len {
        break;
      }
      // Stop at `/` (alpha separator) at depth 0
      if bytes[i] == b'/' && depth == 0 {
        break;
      }
      let token_start = i;
      while i < len && !bytes[i].is_ascii_whitespace() {
        if bytes[i] == b'(' {
          depth += 1;
        } else if bytes[i] == b')' {
          depth -= 1;
        } else if bytes[i] == b'/' && depth == 0 {
          break;
        }
        i += 1;
      }
      if i > token_start {
        tokens.push((token_start, &args[token_start..i]));
      }
    }

    if let Some(&(offset, token)) = tokens.get(index) {
      return Some((token.trim(), offset));
    }
  }
  None
}

/// Find the matching closing parenthesis, handling nested parens.
fn find_matching_paren(s: &str, from: usize) -> Option<usize> {
  let bytes = s.as_bytes();
  let mut depth = 1;
  let mut i = from;
  while i < bytes.len() {
    match bytes[i] {
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 {
          return Some(i);
        }
      }
      _ => {}
    }
    i += 1;
  }
  None
}

/// Check if a value contains CSS variables, SCSS variables, or Less variables.
fn contains_variable(s: &str) -> bool {
  s.contains("var(") || s.contains("env(") || s.contains('$') || s.contains('@')
}

/// Check if a value has a `deg` unit.
fn has_deg_unit(s: &str) -> bool {
  s.to_ascii_lowercase().ends_with("deg")
}

/// Check if a value is a bare number (no angle unit suffix).
fn is_bare_number(s: &str) -> bool {
  if s.is_empty() {
    return false;
  }
  // If it ends with an angle unit, it's not bare.
  let lower = s.to_ascii_lowercase();
  for unit in &["deg", "rad", "grad", "turn"] {
    if lower.ends_with(unit) {
      return false;
    }
  }
  // If it ends with %, it's not a bare number (it's a percentage)
  if s.ends_with('%') {
    return false;
  }
  // Must parse as a number.
  let s = s.trim();
  let mut has_digit = false;
  for (i, c) in s.chars().enumerate() {
    if c.is_ascii_digit() {
      has_digit = true;
    } else if c == '.' || ((c == '-' || c == '+') && i == 0) {
      // ok
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
  fn reports_bare_number_hue_in_hsl() {
    let d = HueDegreeNotation.check(&style_with_value("hsl(0 100% 50%)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected \"0\" to be \"0deg\""));
  }

  #[test]
  fn allows_degree_notation() {
    let d = HueDegreeNotation.check(&style_with_value("hsl(0deg 100% 50%)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_bare_number_in_hwb() {
    let d = HueDegreeNotation.check(&style_with_value("hwb(120 0% 0%)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected \"120\" to be \"120deg\""));
  }

  #[test]
  fn allows_rad_unit() {
    let d = HueDegreeNotation.check(&style_with_value("hsl(3.14rad 100% 50%)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_legacy_comma_syntax() {
    let d = HueDegreeNotation.check(&style_with_value("hsla(257, 100%, 9%, 1)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected \"257\" to be \"257deg\""));
  }

  #[test]
  fn reports_lch_hue_as_third_arg() {
    let d = HueDegreeNotation.check(&style_with_value("lch(50% 30 120)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected \"120\" to be \"120deg\""));
  }

  #[test]
  fn allows_lch_with_deg() {
    let d = HueDegreeNotation.check(&style_with_value("lch(50% 30 120deg)"), &ctx());
    assert!(d.is_empty());
  }
}
