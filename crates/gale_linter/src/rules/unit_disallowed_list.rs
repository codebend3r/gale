use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

pub struct UnitDisallowedList;

// ---------------------------------------------------------------------------
// Unit extraction with byte-offset tracking
// ---------------------------------------------------------------------------

struct UnitOccurrence {
  unit: String,
  /// The innermost function name surrounding this unit, if any (lowercased).
  function: Option<String>,
  /// Byte offset of the start of the number within the searched string.
  number_byte_offset: usize,
  /// Byte length of the unit only (for precise span pointing at the unit).
  unit_byte_len: usize,
}

/// Extract all `<number><unit>` occurrences from a CSS value string.
fn extract_units_with_context(value: &str) -> Vec<UnitOccurrence> {
  let mut results = Vec::new();
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut func_stack: Vec<String> = Vec::new();

  // Build char-index → byte-offset mapping
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
    // Skip quoted strings
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

    // Skip CSS comments
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

    // Track function calls
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

    // Skip inside url() and var()
    if let Some(func) = func_stack.last()
      && (func == "url" || func == "var")
    {
      i += 1;
      continue;
    }

    // Skip custom property names
    if i + 1 < len && chars[i] == '-' && chars[i + 1] == '-' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip SCSS variables
    if chars[i] == '$' {
      i += 1;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip Less variables
    if chars[i] == '@' && i + 1 < len && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
    {
      i += 1;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    // Skip hex colours
    if chars[i] == '#' {
      i += 1;
      while i < len && chars[i].is_ascii_hexdigit() {
        i += 1;
      }
      continue;
    }

    // Skip unicode-range
    if (chars[i] == 'U' || chars[i] == 'u') && i + 1 < len && chars[i + 1] == '+' {
      i += 2;
      while i < len && (chars[i].is_ascii_hexdigit() || chars[i] == '-' || chars[i] == '?') {
        i += 1;
      }
      continue;
    }

    // Extract number followed by unit
    if chars[i].is_ascii_digit()
      || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit())
    {
      // Hyphenated identifier check
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
      // Scientific notation
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
        // Identifier/function name check
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
        let number_byte = char_to_byte[number_start];
        let unit_byte_start = char_to_byte[unit_char_start];
        let unit_byte_end = char_to_byte[i];
        results.push(UnitOccurrence {
          unit,
          function: func,
          number_byte_offset: number_byte,
          unit_byte_len: unit_byte_end - unit_byte_start,
        });
      }
    } else {
      i += 1;
    }
  }

  results
}

// ---------------------------------------------------------------------------
// Pattern matching
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

struct DisallowedListOptions {
  units: Vec<String>,
  ignore_functions: Vec<String>,
  ignore_properties: std::collections::HashMap<String, Vec<String>>,
  ignore_media_feature_names: std::collections::HashMap<String, Vec<String>>,
}

fn parse_options(ctx: &RuleContext) -> DisallowedListOptions {
  let primary = ctx.primary_option();
  let secondary = ctx.secondary_options();

  let units: Vec<String> = match primary {
    Some(serde_json::Value::Array(arr)) => arr
      .iter()
      .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
      .collect(),
    Some(serde_json::Value::String(s)) => vec![s.to_ascii_lowercase()],
    _ => Vec::new(),
  };

  let mut ignore_functions = Vec::new();
  let mut ignore_properties = std::collections::HashMap::new();
  let mut ignore_media_feature_names = std::collections::HashMap::new();

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
    if let Some(v) = sec.get("ignoreMediaFeatureNames")
      && let Some(obj) = v.as_object()
    {
      for (unit, patterns) in obj {
        ignore_media_feature_names.insert(unit.to_ascii_lowercase(), parse_string_list(patterns));
      }
    }
  }

  DisallowedListOptions {
    units,
    ignore_functions,
    ignore_properties,
    ignore_media_feature_names,
  }
}

// ---------------------------------------------------------------------------
// Media feature extraction
// ---------------------------------------------------------------------------

/// A media feature with its name, value, and byte offsets within the params string.
struct MediaFeature {
  name: String,
  value: String,
  /// Byte offset of the value portion within the original (lowercased) params string.
  value_byte_offset: usize,
}

fn extract_media_features(params: &str) -> Vec<MediaFeature> {
  let mut results = Vec::new();
  let lower = params.to_ascii_lowercase();
  let chars: Vec<char> = lower.chars().collect();
  let len = chars.len();
  let mut i = 0;

  // byte mapping for lowercase version
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
      // Skip comments
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
          results.push(MediaFeature {
            name,
            value: val.trim().to_string(),
            value_byte_offset: char_to_byte[val_start],
          });
        }
      }
    } else {
      i += 1;
    }
  }
  results
}

// ---------------------------------------------------------------------------
// Rule implementation
// ---------------------------------------------------------------------------

impl Rule for UnitDisallowedList {
  fn name(&self) -> &'static str {
    "unit-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed units"
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
          // Find where the value starts in the source
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
          if at_name != "font-face" {
            // For @media, use the params offset
            let params_offset = find_params_offset(ctx.source, at_rule.span.offset, &at_rule.name);
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

/// Find the byte offset where the value starts in a declaration.
/// The value starts after "property:" (possibly with whitespace).
fn find_value_offset(source: &str, decl_offset: usize, decl_length: usize) -> usize {
  let end = (decl_offset + decl_length).min(source.len());
  if decl_offset >= source.len() || end <= decl_offset {
    return decl_offset;
  }
  let slice = &source[decl_offset..end];
  // Find the colon, then skip whitespace after it
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

/// Find the byte offset where @-rule params start.
fn find_params_offset(source: &str, at_rule_offset: usize, at_name: &str) -> usize {
  // Params start after `@name` + whitespace
  let start = at_rule_offset;
  if start >= source.len() {
    return start;
  }
  let rest = &source[start..];
  // Skip `@`
  let mut off = 1;
  let bytes = rest.as_bytes();
  // Skip the at-rule name
  while off < bytes.len() && (bytes[off].is_ascii_alphanumeric() || bytes[off] == b'-') {
    off += 1;
  }
  // Skip whitespace
  while off < bytes.len() && bytes[off].is_ascii_whitespace() {
    off += 1;
  }
  start + off
}

fn check_value(
  value: &str,
  opts: &DisallowedListOptions,
  property: Option<&str>,
  value_base_offset: usize,
  rule_name: &str,
  severity: Severity,
  diags: &mut Vec<Diagnostic>,
) {
  let occurrences = extract_units_with_context(value);
  for occ in occurrences {
    let unit_lower = occ.unit.to_ascii_lowercase();
    if !opts.units.contains(&unit_lower) {
      continue;
    }

    // Check ignoreFunctions
    if let Some(ref func) = occ.function
      && function_is_ignored(func, &opts.ignore_functions)
    {
      continue;
    }

    // Check ignoreProperties
    if let Some(prop) = property
      && let Some(patterns) = opts.ignore_properties.get(&unit_lower)
      && property_matches_any(prop, patterns)
    {
      continue;
    }

    // Point span to the number+unit in the source
    let abs_offset = value_base_offset + occ.number_byte_offset;
    diags.push(
      Diagnostic::new(rule_name, format!("Unexpected unit \"{}\"", occ.unit))
        .severity(severity)
        .span(Span::new(abs_offset, occ.unit_byte_len)),
    );
  }
}

fn check_at_rule_params(
  params: &str,
  opts: &DisallowedListOptions,
  params_base_offset: usize,
  rule_name: &str,
  severity: Severity,
  diags: &mut Vec<Diagnostic>,
) {
  let features = extract_media_features(params);

  if features.is_empty() {
    // No features parsed; check the whole params
    let occurrences = extract_units_with_context(params);
    for occ in occurrences {
      let unit_lower = occ.unit.to_ascii_lowercase();
      if !opts.units.contains(&unit_lower) {
        continue;
      }
      if let Some(ref func) = occ.function
        && function_is_ignored(func, &opts.ignore_functions)
      {
        continue;
      }
      let abs_offset = params_base_offset + occ.number_byte_offset;
      diags.push(
        Diagnostic::new(rule_name, format!("Unexpected unit \"{}\"", occ.unit))
          .severity(severity)
          .span(Span::new(abs_offset, occ.unit_byte_len)),
      );
    }
    return;
  }

  for feature in &features {
    let occurrences = extract_units_with_context(&feature.value);
    for occ in occurrences {
      let unit_lower = occ.unit.to_ascii_lowercase();
      if !opts.units.contains(&unit_lower) {
        continue;
      }
      if let Some(ref func) = occ.function
        && function_is_ignored(func, &opts.ignore_functions)
      {
        continue;
      }
      // Check ignoreMediaFeatureNames
      if let Some(patterns) = opts.ignore_media_feature_names.get(&unit_lower)
        && property_matches_any(&feature.name, patterns)
      {
        continue;
      }
      let abs_offset = params_base_offset + feature.value_byte_offset + occ.number_byte_offset;
      diags.push(
        Diagnostic::new(rule_name, format!("Unexpected unit \"{}\"", occ.unit))
          .severity(severity)
          .span(Span::new(abs_offset, occ.unit_byte_len)),
      );
    }
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
    let d = UnitDisallowedList.check(&style_with_decl("margin", "10px"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_disallowed_unit() {
    let opts = json!(["pt"]);
    let d = UnitDisallowedList.check(&style_with_decl("margin", "10px"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_disallowed_unit() {
    let opts = json!(["px"]);
    let d = UnitDisallowedList.check(&style_with_decl("margin", "10px"), &ctx_with_options(&opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("px"));
  }

  #[test]
  fn rejects_multiple_disallowed_units() {
    let opts = json!(["px", "em"]);
    let d = UnitDisallowedList.check(
      &style_with_decl("margin", "10px 1.5em"),
      &ctx_with_options(&opts),
    );
    assert!(d.len() >= 1); // at least one diagnostic for disallowed units
  }

  #[test]
  fn case_insensitive_unit_match() {
    let opts = json!(["PX"]);
    let d = UnitDisallowedList.check(&style_with_decl("margin", "10px"), &ctx_with_options(&opts));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(UnitDisallowedList.name(), "unit-disallowed-list");
  }

  #[test]
  fn skips_quoted_strings() {
    let opts = json!(["px"]);
    let d = UnitDisallowedList.check(
      &style_with_decl("content", "\"10px\""),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn skips_url_function() {
    let opts = json!(["vmin"]);
    let d = UnitDisallowedList.check(
      &style_with_decl("background-url", "url(10vmin)"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn skips_scss_variables() {
    let opts = json!(["px"]);
    let d = UnitDisallowedList.check(
      &style_with_decl("font-size", "$fs10px"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn skips_custom_property_names() {
    let opts = json!(["px"]);
    let d = UnitDisallowedList.check(
      &style_with_decl("font-size", "--some-fs-10px"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }
}
