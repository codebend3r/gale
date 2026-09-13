use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

pub struct UnitAllowedList;

// ---------------------------------------------------------------------------
// Unit extraction (identical to unit_disallowed_list)
// ---------------------------------------------------------------------------

struct UnitOccurrence {
  unit: String,
  function: Option<String>,
  number_byte_offset: usize,
  unit_byte_len: usize,
}

fn extract_units_with_context(value: &str) -> Vec<UnitOccurrence> {
  let mut results = Vec::new();
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut func_stack: Vec<String> = Vec::new();

  let mut char_to_byte: Vec<usize> = Vec::with_capacity(len + 1);
  {
    let mut bp = 0;
    for ch in value.chars() {
      char_to_byte.push(bp);
      bp += ch.len_utf8();
    }
    char_to_byte.push(bp);
  }

  while i < len {
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

    if chars[i] == '(' {
      let fn_end = i;
      let mut fn_start = i;
      if fn_end > 0 {
        let mut j = fn_end as isize - 1;
        while j >= 0
          && (chars[j as usize].is_ascii_alphanumeric()
            || chars[j as usize] == '-'
            || chars[j as usize] == '_')
        {
          fn_start = j as usize;
          j -= 1;
        }
      }
      let func_name = if fn_start < fn_end {
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

    if let Some(func) = func_stack.last()
      && (func == "url" || func == "var")
    {
      i += 1;
      continue;
    }

    if i + 1 < len && chars[i] == '-' && chars[i + 1] == '-' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    if chars[i] == '$' {
      i += 1;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    if chars[i] == '@' && i + 1 < len && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
    {
      i += 1;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    if chars[i] == '#' {
      i += 1;
      while i < len && chars[i].is_ascii_hexdigit() {
        i += 1;
      }
      continue;
    }

    if (chars[i] == 'U' || chars[i] == 'u') && i + 1 < len && chars[i + 1] == '+' {
      i += 2;
      while i < len && (chars[i].is_ascii_hexdigit() || chars[i] == '-' || chars[i] == '?') {
        i += 1;
      }
      continue;
    }

    if chars[i].is_ascii_digit()
      || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit())
    {
      // If the digit is immediately preceded by a letter, underscore, or
      // is part of an identifier (e.g. `a11y`, `edge2edge`, `h2`), skip
      // the entire identifier — this is not a CSS numeric value.
      if i > 0 && (chars[i - 1].is_ascii_alphabetic() || chars[i - 1] == '_') {
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        continue;
      }

      if i > 1
        && chars[i - 1] == '-'
        && (chars[i - 2].is_ascii_alphanumeric() || chars[i - 2] == '-' || chars[i - 2] == '_')
      {
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        continue;
      }

      let number_start = i;
      while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
        i += 1;
      }
      if i < len && (chars[i] == 'e' || chars[i] == 'E') {
        let save = i;
        i += 1;
        if i < len && (chars[i] == '+' || chars[i] == '-') {
          i += 1;
        }
        if i < len && chars[i].is_ascii_digit() {
          while i < len && chars[i].is_ascii_digit() {
            i += 1;
          }
        } else {
          i = save;
        }
      }

      if i < len && (chars[i].is_ascii_alphabetic() || chars[i] == '%') {
        let unit_char_start = i;
        if chars[i] == '%' {
          i += 1;
        } else {
          while i < len && chars[i].is_ascii_alphabetic() {
            i += 1;
          }
        }
        if i < len
          && (chars[i] == '-'
            || chars[i] == '_'
            || chars[i] == '('
            || chars[i].is_ascii_alphanumeric())
        {
          while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
          {
            i += 1;
          }
          continue;
        }
        let unit: String = chars[unit_char_start..i].iter().collect();
        let func = func_stack.last().cloned();
        results.push(UnitOccurrence {
          unit,
          function: func,
          number_byte_offset: char_to_byte[number_start],
          unit_byte_len: char_to_byte[i] - char_to_byte[unit_char_start],
        });
      }
    } else {
      i += 1;
    }
  }

  results
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn function_is_ignored(func_name: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|p| matches_pattern_ci(func_name, p))
}

fn property_matches_any(prop: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|p| matches_pattern_ci(prop, p))
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

struct AllowedListOptions {
  units: Vec<String>,
  ignore_functions: Vec<String>,
  ignore_properties: std::collections::HashMap<String, Vec<String>>,
}

fn parse_options(ctx: &RuleContext) -> AllowedListOptions {
  let primary = ctx.primary_option();
  let secondary = ctx.secondary_options();

  // The primary option is an array of unit strings, e.g. ["px", "em", "%"].
  // But ctx.primary_option() returns arr.first() when options is an array,
  // which is wrong when the array IS the primary (not [primary, secondary]).
  // Handle both formats:
  //   - ["px", "em"]              → options is the array directly
  //   - [["px", "em"], {secondary}] → primary_option() returns the inner array
  let units: Vec<String> = match primary {
    Some(serde_json::Value::Array(arr)) => arr
      .iter()
      .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
      .collect(),
    Some(serde_json::Value::String(s)) => {
      // primary_option() returned a string — the options array was
      // ["px", "em", ...] and it returned just the first element.
      // Use the full options array instead.
      if let Some(serde_json::Value::Array(arr)) = ctx.options {
        arr
          .iter()
          .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
          .collect()
      } else {
        vec![s.to_ascii_lowercase()]
      }
    }
    _ => Vec::new(),
  };

  let mut ignore_functions = Vec::new();
  let mut ignore_properties = std::collections::HashMap::new();

  if let Some(sec) = secondary {
    if let Some(v) = sec.get("ignoreFunctions") {
      ignore_functions = parse_string_list(v);
    }
    if let Some(v) = sec.get("ignoreProperties")
      && let Some(obj) = v.as_object()
    {
      for (unit, patterns) in obj {
        ignore_properties.insert(unit.to_ascii_lowercase(), parse_string_list(patterns));
      }
    }
  }

  AllowedListOptions {
    units,
    ignore_functions,
    ignore_properties,
  }
}

/// Find the byte offset where the value starts in a declaration.
fn find_value_offset(source: &str, decl_offset: usize, decl_length: usize) -> usize {
  let end = (decl_offset + decl_length).min(source.len());
  if decl_offset >= source.len() || end <= decl_offset {
    return decl_offset;
  }
  let slice = &source[decl_offset..end];
  if let Some(colon) = slice.find(':') {
    let after_colon = colon + 1;
    let mut off = after_colon;
    let bytes = slice.as_bytes();
    while off < bytes.len() && bytes[off].is_ascii_whitespace() {
      off += 1;
    }
    decl_offset + off
  } else {
    decl_offset
  }
}

fn find_params_offset(source: &str, at_rule_offset: usize, _at_name: &str) -> usize {
  let start = at_rule_offset;
  if start >= source.len() {
    return start;
  }
  let rest = &source[start..];
  let mut off = 1; // skip `@`
  let bytes = rest.as_bytes();
  while off < bytes.len() && (bytes[off].is_ascii_alphanumeric() || bytes[off] == b'-') {
    off += 1;
  }
  while off < bytes.len() && bytes[off].is_ascii_whitespace() {
    off += 1;
  }
  start + off
}

impl Rule for UnitAllowedList {
  fn name(&self) -> &'static str {
    "unit-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed units"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let opts = parse_options(ctx);
    if opts.units.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();

    match node {
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          if decl.property.eq_ignore_ascii_case("unicode-range") {
            continue;
          }
          let value_offset = find_value_offset(ctx.source, decl.span.offset, decl.span.length);
          check_value(
            &decl.value,
            &opts,
            Some(&decl.property),
            value_offset,
            self.name(),
            self.default_severity(),
            &mut diags,
          );
        }
      }
      CssNode::AtRule(at_rule) => {
        if !at_rule.params.is_empty() {
          let at_name = at_rule.name.to_ascii_lowercase();
          // Stylelint only checks units in @media at-rule params,
          // not in other at-rules like @include, @container, etc.
          if at_name == "media" {
            let params_offset = find_params_offset(ctx.source, at_rule.span.offset, &at_rule.name);
            let occurrences = extract_units_with_context(&at_rule.params);
            for occ in occurrences {
              let unit_lower = occ.unit.to_ascii_lowercase();
              if opts.units.contains(&unit_lower) {
                continue;
              }
              if let Some(ref func) = occ.function
                && function_is_ignored(func, &opts.ignore_functions)
              {
                continue;
              }
              let abs_offset = params_offset + occ.number_byte_offset;
              diags.push(
                Diagnostic::new(self.name(), format!("Unexpected unit \"{}\"", occ.unit))
                  .severity(self.default_severity())
                  .span(Span::new(abs_offset, occ.unit_byte_len)),
              );
            }
          }
        }
      }
      CssNode::Declaration(decl) => {
        if decl.property.eq_ignore_ascii_case("unicode-range") {
          return vec![];
        }
        let value_offset = find_value_offset(ctx.source, decl.span.offset, decl.span.length);
        check_value(
          &decl.value,
          &opts,
          Some(&decl.property),
          value_offset,
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

fn check_value(
  value: &str,
  opts: &AllowedListOptions,
  property: Option<&str>,
  value_base_offset: usize,
  rule_name: &str,
  severity: Severity,
  diags: &mut Vec<Diagnostic>,
) {
  let occurrences = extract_units_with_context(value);
  for occ in occurrences {
    let unit_lower = occ.unit.to_ascii_lowercase();
    if opts.units.contains(&unit_lower) {
      continue;
    }

    if let Some(ref func) = occ.function
      && function_is_ignored(func, &opts.ignore_functions)
    {
      continue;
    }

    if let Some(prop) = property
      && let Some(patterns) = opts.ignore_properties.get(&unit_lower)
      && property_matches_any(prop, patterns)
    {
      continue;
    }

    let abs_offset = value_base_offset + occ.number_byte_offset;
    diags.push(
      Diagnostic::new(rule_name, format!("Unexpected unit \"{}\"", occ.unit))
        .severity(severity)
        .span(Span::new(abs_offset, occ.unit_byte_len)),
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};
  use serde_json::json;

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = UnitAllowedList.check(&style_with_decl("margin", "10px"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_listed_unit() {
    let opts = json!(["px", "rem"]);
    let d = UnitAllowedList.check(&style_with_decl("margin", "10px"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_unlisted_unit() {
    let opts = json!(["rem"]);
    let d = UnitAllowedList.check(&style_with_decl("margin", "10px"), &ctx_with_options(&opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("px"));
  }

  #[test]
  fn case_insensitive_unit_match() {
    let opts = json!(["PX"]);
    let d = UnitAllowedList.check(&style_with_decl("margin", "10px"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_percentage() {
    let opts = json!(["%"]);
    let d = UnitAllowedList.check(&style_with_decl("width", "100%"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(UnitAllowedList.name(), "unit-allowed-list");
  }
}
