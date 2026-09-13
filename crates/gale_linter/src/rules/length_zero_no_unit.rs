use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports units on zero lengths (e.g. `0px` → `0`).
///
/// Equivalent to Stylelint's `length-zero-no-unit` rule.
///
/// Secondary options:
///   - `ignore`: array, may include `"custom-properties"` to skip `--foo: 0px`.
///   - `ignoreFunctions`: array of function name strings/regexes. Units inside
///     matching functions are not checked (e.g. `var`, `/^--/`).
pub struct LengthZeroNoUnit;

const LENGTH_UNITS: &[&str] = &[
  "px", "em", "rem", "ex", "ch", "vw", "vh", "vmin", "vmax", "cm", "mm", "in", "pt", "pc", "q",
  "cap", "ic", "rlh", "lh", "vi", "vb", "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax", "dvw", "dvh",
  "lvw", "lvh", "svw", "svh",
];

/// Duration and other units that are NOT lengths — `0s`, `0ms`, `0%` etc.
/// are not reported by this rule (Stylelint only reports zero *lengths*).
/// Angle, time, frequency, resolution units are excluded.

/// Properties where `0<unit>` is intentionally allowed because removing the
/// unit changes the semantics. Stylelint excludes these:
/// - `line-height`: `0` is a multiplier, `0px` is a length — different meaning.
/// - `flex` / `flex-basis`: `0` means "flex factor", `0px` means "0 length".
/// - `font` shorthand: contains a `line-height` component where `0px` is meaningful.
const ZERO_UNIT_EXEMPT_PROPERTIES: &[&str] = &[
  "line-height",
  "flex",
  "flex-basis",
  "font",
  "grid-template-columns",
  "grid-template-rows",
  "grid-auto-columns",
  "grid-auto-rows",
  "transition",
  "transition-delay",
  "transition-duration",
  "animation",
  "animation-delay",
  "animation-duration",
];

fn is_exempt_property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  ZERO_UNIT_EXEMPT_PROPERTIES.iter().any(|&p| lower == p)
}

/// Math functions where `0<unit>` must keep its unit because the function
/// requires typed values for dimensional analysis.
const MATH_FUNCTIONS: &[&str] = &[
  "calc",
  "min",
  "max",
  "clamp",
  "abs",
  "sign",
  "round",
  "mod",
  "rem",
  "sin",
  "cos",
  "tan",
  "asin",
  "acos",
  "atan",
  "atan2",
  "pow",
  "sqrt",
  "hypot",
  "log",
  "exp",
  "-webkit-calc",
  "-moz-calc",
];

fn is_math_function(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  MATH_FUNCTIONS.iter().any(|&f| lower == f)
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

struct Options {
  ignore_custom_properties: bool,
  ignore_functions: Vec<String>,
}

fn parse_options(ctx: &RuleContext) -> Options {
  let secondary = ctx.secondary_options();
  let mut ignore_custom_properties = false;
  let mut ignore_functions = Vec::new();

  if let Some(sec) = secondary {
    if let Some(v) = sec.get("ignore") {
      if let Some(arr) = v.as_array() {
        for item in arr {
          if item.as_str() == Some("custom-properties") {
            ignore_custom_properties = true;
          }
        }
      }
    }
    if let Some(v) = sec.get("ignoreFunctions") {
      ignore_functions = parse_string_list(v);
    }
  }

  Options {
    ignore_custom_properties,
    ignore_functions,
  }
}

fn parse_string_list(val: &serde_json::Value) -> Vec<String> {
  match val {
    serde_json::Value::Array(arr) => arr
      .iter()
      .filter_map(|v| v.as_str().map(|s| s.to_string()))
      .collect(),
    serde_json::Value::String(s) => vec![s.clone()],
    _ => Vec::new(),
  }
}

fn matches_pattern_ci(value: &str, pattern: &str) -> bool {
  if pattern.starts_with('/') {
    if let Some(inner) = pattern.strip_prefix('/') {
      if let Some(inner) = inner.strip_suffix("/i") {
        let re_str = format!("(?i){}", inner);
        if let Ok(re) = regex::Regex::new(&re_str) {
          return re.is_match(value);
        }
        return false;
      }
      if let Some(inner) = inner.strip_suffix('/') {
        if let Ok(re) = regex::Regex::new(inner) {
          return re.is_match(value);
        }
        return false;
      }
    }
    false
  } else {
    value.eq_ignore_ascii_case(pattern)
  }
}

fn function_is_ignored(func_name: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|p| matches_pattern_ci(func_name, p))
}

impl Rule for LengthZeroNoUnit {
  fn name(&self) -> &'static str {
    "length-zero-no-unit"
  }

  fn description(&self) -> &'static str {
    "Disallow units for zero lengths"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let opts = parse_options(ctx);
    let mut diags = Vec::new();

    match node {
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          // Check ignore: ["custom-properties"]
          if decl.property.starts_with("--") {
            if opts.ignore_custom_properties {
              continue;
            }
            // Custom properties are checked only if not ignored
          }

          // Skip exempt properties
          if is_exempt_property(&decl.property) {
            continue;
          }

          let decl_start = decl.span.offset;
          let decl_end = decl_start + decl.span.length;
          let search_area = if decl_end <= ctx.source.len() && decl_start < decl_end {
            &ctx.source[decl_start..decl_end]
          } else {
            &decl.value
          };

          find_zero_units_contextual(
            search_area,
            decl_start,
            decl_end <= ctx.source.len() && decl_start < decl_end,
            &opts,
            self.name(),
            self.default_severity(),
            &mut diags,
          );
        }
      }
      CssNode::AtRule(at_rule) => {
        // Check @media params for `0px` etc.
        if !at_rule.params.is_empty() {
          let at_name = at_rule.name.to_ascii_lowercase();
          if at_name != "font-face" {
            find_zero_units_contextual(
              &at_rule.params,
              at_rule.span.offset,
              false,
              &opts,
              self.name(),
              self.default_severity(),
              &mut diags,
            );
          }
        }
      }
      CssNode::Declaration(decl) => {
        if decl.property.starts_with("--") && opts.ignore_custom_properties {
          return vec![];
        }
        if is_exempt_property(&decl.property) {
          return vec![];
        }

        let decl_start = decl.span.offset;
        let decl_end = decl_start + decl.span.length;
        let search_area = if decl_end <= ctx.source.len() && decl_start < decl_end {
          &ctx.source[decl_start..decl_end]
        } else {
          &decl.value
        };

        find_zero_units_contextual(
          search_area,
          decl_start,
          decl_end <= ctx.source.len() && decl_start < decl_end,
          &opts,
          self.name(),
          self.default_severity(),
          &mut diags,
        );
      }
      _ => {}
    }
    diags
  }
}

/// Find `0<unit>` patterns in a value string, respecting function context.
///
/// Skips:
/// - Inside math functions (calc, min, max, clamp, etc.)
/// - Inside quoted strings
/// - Inside CSS comments
/// - Inside functions matched by `ignoreFunctions`
fn find_zero_units_contextual(
  value: &str,
  base_offset: usize,
  has_source_mapping: bool,
  opts: &Options,
  rule_name: &str,
  severity: Severity,
  diags: &mut Vec<Diagnostic>,
) {
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut func_stack: Vec<String> = Vec::new();

  // We need byte offsets for spans, so also track byte position.
  let bytes = value.as_bytes();
  let byte_len = bytes.len();

  // Build a char-index → byte-offset mapping
  let mut char_to_byte: Vec<usize> = Vec::with_capacity(len + 1);
  {
    let mut byte_pos = 0;
    for ch in value.chars() {
      char_to_byte.push(byte_pos);
      byte_pos += ch.len_utf8();
    }
    char_to_byte.push(byte_pos);
  }

  while i < len {
    // --- Skip quoted strings ---
    if chars[i] == '"' || chars[i] == '\'' {
      let quote = chars[i];
      i += 1;
      while i < len && chars[i] != quote {
        if chars[i] == '\\' {
          i += 1;
        }
        i += 1;
      }
      if i < len {
        i += 1;
      }
      continue;
    }

    // --- Skip CSS comments ---
    if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
      i += 2;
      while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
        i += 1;
      }
      if i + 1 < len {
        i += 2;
      }
      continue;
    }

    // --- Skip SCSS/Less interpolation blocks ---
    if i + 1 < len && ((chars[i] == '#' || chars[i] == '@') && chars[i + 1] == '{') {
      i += 2;
      let mut depth = 1;
      while i < len && depth > 0 {
        if chars[i] == '{' {
          depth += 1;
        } else if chars[i] == '}' {
          depth -= 1;
        }
        i += 1;
      }
      continue;
    }

    // --- Track function calls ---
    if chars[i] == '(' {
      let fn_end = i;
      let mut fn_start = i;
      if fn_end > 0 {
        fn_start = fn_end;
        let mut j = fn_end as isize - 1;
        while j >= 0
          && (chars[j as usize].is_ascii_alphanumeric()
            || chars[j as usize] == '-'
            || chars[j as usize] == '_'
            || chars[j as usize] == '.')
        {
          fn_start = j as usize;
          j -= 1;
        }
      }
      let func_name: String = if fn_start < fn_end {
        chars[fn_start..fn_end]
          .iter()
          .collect::<String>()
          .to_ascii_lowercase()
      } else {
        String::new()
      };
      func_stack.push(func_name);
      i += 1;
      continue;
    }

    if chars[i] == ')' {
      func_stack.pop();
      i += 1;
      continue;
    }

    // --- Skip SCSS variables ---
    if chars[i] == '$' {
      i += 1;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // --- Skip custom property names ---
    if i + 1 < len && chars[i] == '-' && chars[i + 1] == '-' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // --- Check for zero followed by a length unit ---
    if chars[i] == '0' {
      // Ensure it's not preceded by a digit or dot (part of a larger number)
      let is_start = i == 0 || (!chars[i - 1].is_ascii_digit() && chars[i - 1] != '.');

      if is_start {
        // Skip decimals: `0.5px` is not zero
        let mut j = i + 1;

        // Handle `0.000` patterns: skip additional zeros and dots
        // `0.000px` IS zero, `0.5px` is not
        let mut is_zero = true;
        if j < len && chars[j] == '.' {
          j += 1;
          while j < len && chars[j] == '0' {
            j += 1;
          }
          // If we stopped at a non-zero digit, this is not zero
          if j < len && chars[j].is_ascii_digit() && chars[j] != '0' {
            is_zero = false;
          }
        }

        // Also check for `.0rem` pattern: the number is just `.0`
        // which equals zero. But this is handled by checking if the
        // "zero" is exactly `0` at position i.

        if !is_zero {
          i = j;
          // skip past the rest of the number
          while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
          }
          // skip past the unit too
          while i < len && (chars[i].is_ascii_alphabetic() || chars[i] == '%') {
            i += 1;
          }
          continue;
        }

        // j now points to the first char after the zero (and any trailing decimal zeros)
        // Check if followed by a length unit
        let unit_start = j;
        if unit_start < len && chars[unit_start].is_ascii_alphabetic() {
          let mut unit_end = unit_start;
          while unit_end < len && chars[unit_end].is_ascii_alphabetic() {
            unit_end += 1;
          }

          // Check it's not part of an identifier (followed by `-`, `_`, `(`, or alphanum)
          if unit_end < len
            && (chars[unit_end] == '-'
              || chars[unit_end] == '_'
              || chars[unit_end] == '('
              || chars[unit_end].is_ascii_alphanumeric())
          {
            i = unit_end;
            continue;
          }

          let unit: String = chars[unit_start..unit_end].iter().collect::<String>();
          let unit_lower = unit.to_ascii_lowercase();

          // Only report length units (not %, s, ms, deg, etc.)
          if LENGTH_UNITS.iter().any(|&u| u == unit_lower) {
            // Check if inside a math function — skip
            let in_math = func_stack.iter().any(|f| is_math_function(f));
            if in_math {
              i = unit_end;
              continue;
            }

            // Check if inside a SCSS module function (namespace.func) — skip.
            // Stylelint doesn't parse inside these.
            let in_scss_module = func_stack.iter().any(|f| f.contains('.'));
            if in_scss_module {
              i = unit_end;
              continue;
            }

            // Check ignoreFunctions
            let in_ignored_func = func_stack
              .iter()
              .any(|f| function_is_ignored(f, &opts.ignore_functions));
            if in_ignored_func {
              i = unit_end;
              continue;
            }

            // Found a reportable `0<unit>`
            let byte_start = char_to_byte[i];
            let byte_end = char_to_byte[unit_end];
            let zero_unit_byte_len = byte_end - byte_start;

            let abs_offset = if has_source_mapping {
              base_offset + byte_start
            } else {
              base_offset
            };

            // Stylelint points to the unit part (after the zero)
            let unit_byte_start = char_to_byte[unit_start];
            let unit_abs_offset = if has_source_mapping {
              base_offset + unit_byte_start
            } else {
              base_offset
            };
            let unit_byte_len = byte_end - unit_byte_start;

            diags.push(
              Diagnostic::new(rule_name, "Unexpected unit".to_string())
                .severity(severity)
                .span(Span::new(unit_abs_offset, unit_byte_len))
                .fix(Fix::new(
                  "Remove unit",
                  vec![Edit::new(Span::new(abs_offset, zero_unit_byte_len), "0")],
                )),
            );
            i = unit_end;
            continue;
          }
        }
        i = j.max(i + 1);
        continue;
      }
    }

    // --- Handle `.0rem` pattern (starts with dot) ---
    if chars[i] == '.' && i + 1 < len && chars[i + 1] == '0' {
      // Check it's not preceded by a digit (would be part of 1.0)
      let is_start = i == 0 || !chars[i - 1].is_ascii_digit();
      if is_start {
        let mut j = i + 1; // skip the dot
        // skip zeros
        while j < len && chars[j] == '0' {
          j += 1;
        }
        // If we reach a non-zero digit, this is not zero
        if j < len && chars[j].is_ascii_digit() && chars[j] != '0' {
          i = j;
          continue;
        }
        // Check for unit
        if j < len && chars[j].is_ascii_alphabetic() {
          let unit_start = j;
          let mut unit_end = j;
          while unit_end < len && chars[unit_end].is_ascii_alphabetic() {
            unit_end += 1;
          }
          if unit_end < len
            && (chars[unit_end] == '-' || chars[unit_end] == '_' || chars[unit_end] == '(')
          {
            i = unit_end;
            continue;
          }
          let unit: String = chars[unit_start..unit_end].iter().collect();
          let unit_lower = unit.to_ascii_lowercase();
          if LENGTH_UNITS.iter().any(|&u| u == unit_lower) {
            let in_math = func_stack.iter().any(|f| is_math_function(f));
            if in_math {
              i = unit_end;
              continue;
            }
            let in_scss_module = func_stack.iter().any(|f| f.contains('.'));
            if in_scss_module {
              i = unit_end;
              continue;
            }
            let in_ignored_func = func_stack
              .iter()
              .any(|f| function_is_ignored(f, &opts.ignore_functions));
            if in_ignored_func {
              i = unit_end;
              continue;
            }

            let byte_start = char_to_byte[i];
            let byte_end = char_to_byte[unit_end];
            let zero_unit_byte_len = byte_end - byte_start;

            let abs_offset = if has_source_mapping {
              base_offset + byte_start
            } else {
              base_offset
            };

            let unit_byte_start = char_to_byte[unit_start];
            let unit_abs_offset = if has_source_mapping {
              base_offset + unit_byte_start
            } else {
              base_offset
            };
            let unit_byte_len = byte_end - unit_byte_start;

            diags.push(
              Diagnostic::new(rule_name, "Unexpected unit".to_string())
                .severity(severity)
                .span(Span::new(unit_abs_offset, unit_byte_len))
                .fix(Fix::new(
                  "Remove unit",
                  vec![Edit::new(Span::new(abs_offset, zero_unit_byte_len), "0")],
                )),
            );
            i = unit_end;
            continue;
          }
        }
      }
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

  fn style_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_zero_with_unit() {
    let d = LengthZeroNoUnit.check(&style_decl("margin", "0px"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_zero_without_unit() {
    assert!(
      LengthZeroNoUnit
        .check(&style_decl("margin", "0"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_non_zero_with_unit() {
    assert!(
      LengthZeroNoUnit
        .check(&style_decl("margin", "10px"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_custom_properties_when_ignored() {
    // Without ignore option, custom properties ARE checked
    let d = LengthZeroNoUnit.check(&style_decl("--my-var", "0px"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_zero_in_calc() {
    let d = LengthZeroNoUnit.check(&style_decl("padding", "calc(0px + 10px)"), &ctx());
    assert!(
      d.is_empty(),
      "Expected no diagnostics inside calc(), got: {:?}",
      d
    );
  }

  #[test]
  fn allows_zero_in_line_height() {
    let d = LengthZeroNoUnit.check(&style_decl("line-height", "0px"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_zero_in_flex() {
    let d = LengthZeroNoUnit.check(&style_decl("flex", "0px"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_zero_unit_before_scss_interpolation() {
    let d = LengthZeroNoUnit.check(&style_decl("padding", "0px #{$var}"), &ctx());
    assert_eq!(
      d.len(),
      1,
      "Should report 0px even when SCSS interpolation follows"
    );
  }

  #[test]
  fn reports_zero_unit_after_scss_interpolation() {
    let d = LengthZeroNoUnit.check(&style_decl("margin", "#{$var} 0px"), &ctx());
    assert_eq!(
      d.len(),
      1,
      "Should report 0px even when SCSS interpolation precedes"
    );
  }
}
