use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_unit;
use crate::rule::{Rule, RuleContext};

pub struct UnitNoUnknown;

// ---------------------------------------------------------------------------
// Unit extraction with byte offsets
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

    // Skip inside url() only
    if let Some(func) = func_stack.last()
      && func == "url"
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
        // Skip SCSS map keys: tokens like `2xl:` where the "unit" is
        // actually part of a map key (e.g., `(2xl: value)`).
        let mut peek = i;
        while peek < len && chars[peek].is_ascii_whitespace() {
          peek += 1;
        }
        if peek < len && chars[peek] == ':' {
          // Looks like a map key — skip this token entirely.
          i = peek + 1;
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

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

struct Options {
  ignore_units: Vec<String>,
  ignore_functions: Vec<String>,
  /// Units that Gale normally considers known but should be treated as unknown.
  /// Used for Stylelint version compatibility: older Stylelint versions had a
  /// smaller known-units list (e.g. Stylelint 13 didn't know `dvh`).
  additional_unknown_units: Vec<String>,
}

fn parse_options(ctx: &RuleContext) -> Options {
  let secondary = ctx.secondary_options();
  let mut ignore_units = Vec::new();
  let mut ignore_functions = Vec::new();
  let mut additional_unknown_units = Vec::new();

  if let Some(sec) = secondary {
    if let Some(v) = sec.get("ignoreUnits") {
      ignore_units = parse_string_list(v);
    }
    if let Some(v) = sec.get("ignoreFunctions") {
      ignore_functions = parse_string_list(v);
    }
    // Internal option injected by the CLI for Stylelint version compat.
    if let Some(v) = sec.get("__additionalUnknownUnits") {
      additional_unknown_units = parse_string_list(v);
    }
  }

  Options {
    ignore_units,
    ignore_functions,
    additional_unknown_units,
  }
}

fn unit_is_ignored(unit: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|p| matches_pattern_ci(unit, p))
}

fn function_is_ignored(func_name: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|p| matches_pattern_ci(func_name, p))
}

/// `x` unit is only valid inside `image-set()` and `@media (resolution: ...)`.
fn is_x_unit_valid_in_context(func: Option<&str>, in_media_resolution: bool) -> bool {
  if in_media_resolution {
    return true;
  }
  if let Some(f) = func {
    return f == "image-set";
  }
  false
}

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

fn find_params_offset(source: &str, at_rule_offset: usize) -> usize {
  let start = at_rule_offset;
  if start >= source.len() {
    return start;
  }
  let rest = &source[start..];
  let mut off = 1;
  let bytes = rest.as_bytes();
  while off < bytes.len() && (bytes[off].is_ascii_alphanumeric() || bytes[off] == b'-') {
    off += 1;
  }
  while off < bytes.len() && bytes[off].is_ascii_whitespace() {
    off += 1;
  }
  start + off
}

// ---------------------------------------------------------------------------
// Rule implementation
// ---------------------------------------------------------------------------

impl Rule for UnitNoUnknown {
  fn name(&self) -> &'static str {
    "unit-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown units"
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
          if decl.property.eq_ignore_ascii_case("content") {
            continue;
          }
          if decl.property.eq_ignore_ascii_case("unicode-range") {
            continue;
          }
          // Skip SCSS variable declarations ($var: value)
          if decl.property.starts_with('$') {
            continue;
          }
          let value_offset = find_value_offset(ctx.source, decl.span.offset, decl.span.length);
          check_value(
            &decl.value,
            &opts,
            false,
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
          // Skip at-rules whose params are identifiers, not CSS values
          if matches!(
            at_name.as_str(),
            "font-face"
              | "mixin"
              | "include"
              | "function"
              | "extend"
              | "import"
              | "use"
              | "forward"
              | "at-root"
              | "return"
              | "debug"
              | "warn"
              | "error"
              | "each"
              | "for"
              | "while"
              | "namespace"
              | "layer"
              | "charset"
          ) {
            // skip
          } else {
            let params_offset = find_params_offset(ctx.source, at_rule.span.offset);
            check_at_rule_params(
              &at_rule.params,
              &opts,
              params_offset,
              self.name(),
              self.default_severity(),
              &mut diags,
            );
          }
        }
      }
      CssNode::Declaration(decl) => {
        if decl.property.eq_ignore_ascii_case("content")
          || decl.property.eq_ignore_ascii_case("unicode-range")
          || decl.property.starts_with('$')
        {
          return vec![];
        }
        let value_offset = find_value_offset(ctx.source, decl.span.offset, decl.span.length);
        check_value(
          &decl.value,
          &opts,
          false,
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
  opts: &Options,
  in_media_resolution: bool,
  value_base_offset: usize,
  rule_name: &str,
  severity: Severity,
  diags: &mut Vec<Diagnostic>,
) {
  let occurrences = extract_units_with_context(value);
  for occ in occurrences {
    let unit_lower = occ.unit.to_ascii_lowercase();

    if is_known_unit(&unit_lower) {
      // Check if this unit is forced-unknown for Stylelint version compat.
      let forced_unknown = opts
        .additional_unknown_units
        .iter()
        .any(|u| u.eq_ignore_ascii_case(&unit_lower));

      if unit_lower == "x" {
        if !forced_unknown
          && is_x_unit_valid_in_context(occ.function.as_deref(), in_media_resolution)
        {
          continue;
        }
      } else if !forced_unknown {
        continue;
      }
    }

    if unit_is_ignored(&occ.unit, &opts.ignore_units) {
      continue;
    }

    if let Some(ref func) = occ.function
      && function_is_ignored(func, &opts.ignore_functions)
    {
      continue;
    }

    let abs_offset = value_base_offset + occ.number_byte_offset;
    diags.push(
      Diagnostic::new(
        rule_name,
        format!("Unexpected unknown unit \"{}\"", occ.unit),
      )
      .severity(severity)
      .span(Span::new(abs_offset, occ.unit_byte_len)),
    );
  }
}

fn check_at_rule_params(
  params: &str,
  opts: &Options,
  params_base_offset: usize,
  rule_name: &str,
  severity: Severity,
  diags: &mut Vec<Diagnostic>,
) {
  let features = extract_media_features(params);

  for (feature_name, feature_value, value_byte_offset) in &features {
    let is_resolution = feature_name == "resolution"
      || feature_name == "min-resolution"
      || feature_name == "max-resolution";
    let base = params_base_offset + value_byte_offset;
    check_value(
      feature_value,
      opts,
      is_resolution,
      base,
      rule_name,
      severity,
      diags,
    );
  }

  if features.is_empty() {
    check_value(
      params,
      opts,
      false,
      params_base_offset,
      rule_name,
      severity,
      diags,
    );
  }
}

fn extract_media_features(params: &str) -> Vec<(String, String, usize)> {
  let mut results = Vec::new();
  let lower = params.to_ascii_lowercase();
  let chars: Vec<char> = lower.chars().collect();
  let len = chars.len();
  let mut i = 0;

  let mut char_to_byte: Vec<usize> = Vec::with_capacity(len + 1);
  {
    let mut bp = 0;
    for ch in lower.chars() {
      char_to_byte.push(bp);
      bp += ch.len_utf8();
    }
    char_to_byte.push(bp);
  }

  while i < len {
    if chars[i] == '(' {
      i += 1;
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
      }
      let name_start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      let name: String = chars[name_start..i].iter().collect();
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
      }
      if i < len && (chars[i] == ':' || chars[i] == '<' || chars[i] == '>' || chars[i] == '=') {
        i += 1;
        if i < len && chars[i] == '=' {
          i += 1;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
        let val_start = i;
        let mut depth = 1;
        while i < len && depth > 0 {
          if chars[i] == '(' {
            depth += 1;
          } else if chars[i] == ')' {
            depth -= 1;
            if depth == 0 {
              break;
            }
          }
          i += 1;
        }
        let val: String = chars[val_start..i].iter().collect();
        if !name.is_empty() {
          results.push((name, val.trim().to_string(), char_to_byte[val_start]));
        }
      }
    } else {
      i += 1;
    }
  }
  results
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
        property: "width".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_unknown_unit() {
    let d = UnitNoUnknown.check(&style_with_value("10xyz"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("xyz"));
  }

  #[test]
  fn allows_known_units() {
    assert!(
      UnitNoUnknown
        .check(&style_with_value("10px"), &ctx())
        .is_empty()
    );
    assert!(
      UnitNoUnknown
        .check(&style_with_value("2rem"), &ctx())
        .is_empty()
    );
    assert!(
      UnitNoUnknown
        .check(&style_with_value("50%"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn does_not_extract_unit_from_function_names() {
    let d = UnitNoUnknown.check(&style_with_value("scale3d(1.1, 1.1, 1.1)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_scss_map_keys() {
    // SCSS map keys like `2xl:` should not be treated as "number 2 + unit xl"
    let units = extract_units_with_context("2xl: 100px");
    let unit_names: Vec<&str> = units.iter().map(|u| u.unit.as_str()).collect();
    assert!(
      !unit_names.contains(&"xl"),
      "should not extract 'xl' from SCSS map key '2xl:'; got: {:?}",
      unit_names
    );
    // The value part (100px) should still be parsed
    assert!(
      unit_names.contains(&"px"),
      "should still extract 'px' from value; got: {:?}",
      unit_names
    );
  }

  #[test]
  fn skips_scss_map_key_with_whitespace() {
    // `2xs :` with whitespace before colon
    let units = extract_units_with_context("2xs : 50rem");
    let unit_names: Vec<&str> = units.iter().map(|u| u.unit.as_str()).collect();
    assert!(
      !unit_names.contains(&"xs"),
      "should not extract 'xs' from map key '2xs :'; got: {:?}",
      unit_names
    );
  }

  #[test]
  fn still_reports_unknown_unit_not_followed_by_colon() {
    let units = extract_units_with_context("2xl");
    let unit_names: Vec<&str> = units.iter().map(|u| u.unit.as_str()).collect();
    assert!(
      unit_names.contains(&"xl"),
      "should extract 'xl' when not followed by colon; got: {:?}",
      unit_names
    );
  }
}
