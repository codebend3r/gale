use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports shorthand property values that contain redundant parts
/// (e.g. `margin: 1px 1px 1px 1px` → `margin: 1px`).
///
/// Equivalent to Stylelint's `shorthand-property-no-redundant-values` rule.
pub struct ShorthandPropertyNoRedundantValues;

/// Returns `true` if the value is "standard CSS syntax" — i.e., it does not
/// contain SCSS/Less constructs that would make comparison unreliable.
fn is_standard_syntax_value(value: &str) -> bool {
  if value.contains('$') || value.contains("#{") || value.contains("@{") {
    return false;
  }
  // Less variables: @variable
  if value.contains('@') {
    // Check if any @ is followed by an alpha char (Less variable).
    // But allow @media etc. won't appear in values normally.
    let bytes = value.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
      if bytes[i] == b'@' && bytes[i + 1].is_ascii_alphabetic() {
        return false;
      }
    }
  }
  // SCSS module function call: `namespace.function(`
  let bytes = value.as_bytes();
  for i in 1..bytes.len().saturating_sub(1) {
    if bytes[i] == b'.'
      && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'-' || bytes[i + 1] == b'_')
    {
      return false;
    }
  }
  true
}

/// Properties that support 2/3/4-value shorthand reduction.
const SHORTHAND_PROPERTIES: &[&str] = &[
  "margin",
  "padding",
  "border-color",
  "border-style",
  "border-width",
  "border-radius",
  "gap",
  "grid-gap",
  "overflow",
  "inset",
];

/// Returns `true` if the given property name (case-insensitive) is a
/// recognised shorthand, optionally with a vendor prefix.
fn is_shorthand_property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  // Strip vendor prefix if present.
  let base = if lower.starts_with("-webkit-") {
    &lower[8..]
  } else if lower.starts_with("-moz-") {
    &lower[5..]
  } else if lower.starts_with("-ms-") {
    &lower[4..]
  } else if lower.starts_with("-o-") {
    &lower[3..]
  } else {
    &lower
  };
  SHORTHAND_PROPERTIES.contains(&base)
}

/// Check if the property is `border-radius` (with optional vendor prefix).
fn is_border_radius(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  let base = if lower.starts_with("-webkit-") {
    &lower[8..]
  } else if lower.starts_with("-moz-") {
    &lower[5..]
  } else if lower.starts_with("-ms-") {
    &lower[4..]
  } else if lower.starts_with("-o-") {
    &lower[3..]
  } else {
    &lower
  };
  base == "border-radius"
}

/// Parse ignore options.
struct Options {
  ignore_four_into_three: bool,
}

impl Options {
  fn from_ctx(ctx: &RuleContext) -> Self {
    let mut opts = Options {
      ignore_four_into_three: false,
    };
    let Some(value) = ctx.options else {
      return opts;
    };
    // The config system strips the primary (true) and stores the
    // secondary object directly as options.  So ctx.options is typically
    // `{"ignore": ["four-into-three-edge-values"]}` (an Object).
    // Handle both Object and Array forms for robustness.
    let secondary = match value {
      serde_json::Value::Object(_) => Some(value),
      serde_json::Value::Array(arr) => arr.get(1),
      _ => None,
    };
    if let Some(sec) = secondary
      && let Some(ignore) = sec.get("ignore").and_then(|v| v.as_array())
    {
      for item in ignore {
        if item.as_str() == Some("four-into-three-edge-values") {
          opts.ignore_four_into_three = true;
        }
      }
    }
    opts
  }
}

impl Rule for ShorthandPropertyNoRedundantValues {
  fn name(&self) -> &'static str {
    "shorthand-property-no-redundant-values"
  }

  fn description(&self) -> &'static str {
    "Disallow redundant values in shorthand properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let opts = Options::from_ctx(ctx);
    let mut diags = Vec::new();
    for decl in &rule.declarations {
      // Skip custom properties, SCSS variables, Less variables.
      if decl.property.starts_with("--")
        || decl.property.starts_with('$')
        || decl.property.starts_with('@')
      {
        continue;
      }

      // Check if property is a shorthand (case-insensitive, vendor-prefix aware).
      if !is_shorthand_property(&decl.property) {
        continue;
      }

      // Extract the raw value from the source text (not the parsed value,
      // since lightningcss normalizes shorthand values).
      let (raw_value, value_offset) = match extract_raw_value(ctx.source, decl) {
        Some(v) => v,
        None => continue,
      };

      // Skip non-standard-syntax values (SCSS variables, interpolation,
      // module function calls).
      if !is_standard_syntax_value(&raw_value) {
        continue;
      }

      // For border-radius, handle the `/` separator.
      // (border-radius allows var() in each half — checked per-half)
      if is_border_radius(&decl.property) {
        if let Some(diag) = check_border_radius(&raw_value, value_offset, decl, ctx, self.name()) {
          diags.push(diag);
        }
        continue;
      }

      // Skip values with var() — they could be anything at runtime.
      if raw_value.to_ascii_lowercase().contains("var(") {
        continue;
      }

      // Skip values with commas (background, transition, etc.) — those
      // are not simple shorthand values.
      if raw_value.contains(',') {
        continue;
      }

      // Skip values with `/` (font shorthand: `12pt/10pt`).
      if raw_value.contains('/') {
        continue;
      }

      // Split into tokens, respecting function arguments.
      let tokens = split_value_tokens(&raw_value);

      // Strip `!important` from the token list before checking length.
      let tokens = strip_important(&tokens);

      // Only handle 2, 3, or 4 values (more means it's a different shorthand like border).
      if tokens.len() < 2 || tokens.len() > 4 {
        continue;
      }

      if let Some(shortened) = shorten(&tokens, opts.ignore_four_into_three) {
        let fix = build_fix_from_raw(ctx.source, decl, &raw_value, &shortened);

        let mut diag = Diagnostic::new(
          self.name(),
          format!("Expected \"{}\" to be \"{}\"", raw_value, shortened),
        )
        .severity(self.default_severity())
        .span(Span::new(value_offset, raw_value.len()));

        if let Some(f) = fix {
          diag = diag.fix(f);
        }

        diags.push(diag);
      }
    }
    diags
  }
}

/// Extract the raw value string and its byte offset from source text for a
/// declaration.  This avoids lightningcss's normalisation of shorthand values.
/// Returns `(raw_value, value_offset_in_source)`.
fn extract_raw_value(source: &str, decl: &gale_css_parser::Declaration) -> Option<(String, usize)> {
  let decl_start = decl.span.offset;
  let decl_end = decl_start + decl.span.length;
  if decl_end > source.len() || decl_start >= source.len() {
    return None;
  }
  let decl_text = &source[decl_start..decl_end];

  // Find the colon
  let colon_pos = decl_text.find(':')?;
  let after_colon = &decl_text[colon_pos + 1..];

  // Trim leading whitespace
  let leading_ws = after_colon.len() - after_colon.trim_start().len();
  let value_offset = decl_start + colon_pos + 1 + leading_ws;
  let trimmed = after_colon.trim_start();

  // Strip trailing semicolon and whitespace
  let trimmed = trimmed.trim_end_matches(';').trim_end();

  Some((trimmed.to_string(), value_offset))
}

/// Split a CSS value into tokens, respecting function call parentheses.
/// e.g., `calc(1px + 1px) calc(1px + 1px)` → `["calc(1px + 1px)", "calc(1px + 1px)"]`
fn split_value_tokens(value: &str) -> Vec<String> {
  let mut tokens = Vec::new();
  let mut current = String::new();
  let mut paren_depth = 0;

  for ch in value.chars() {
    match ch {
      '(' => {
        paren_depth += 1;
        current.push(ch);
      }
      ')' => {
        paren_depth -= 1;
        current.push(ch);
      }
      ' ' | '\t' if paren_depth == 0 => {
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
          tokens.push(trimmed);
        }
        current.clear();
      }
      _ => {
        current.push(ch);
      }
    }
  }
  let trimmed = current.trim().to_string();
  if !trimmed.is_empty() {
    tokens.push(trimmed);
  }
  tokens
}

/// Strip `!important` from the token list. If the last token is `!important`,
/// remove it. If the last token ends with `!important`, strip that suffix.
fn strip_important(tokens: &[String]) -> Vec<String> {
  let mut result: Vec<String> = tokens.to_vec();
  if result.is_empty() {
    return result;
  }

  // Check if last token is literally "!important"
  if result
    .last()
    .map(|s| s.eq_ignore_ascii_case("!important"))
    .unwrap_or(false)
  {
    result.pop();
    return result;
  }

  // Check if second-to-last token exists and last is "important" with second-to-last ending in "!"
  // Actually, `!important` is typically `value !important` with a space.
  // But it could also be `value!important` without space.

  // Handle "1px !important" where "!" is part of a separate token
  if result.len() >= 2 {
    let last = result.last().unwrap();
    if last.eq_ignore_ascii_case("important") {
      let prev = &result[result.len() - 2];
      if prev.ends_with('!') {
        let trimmed = prev[..prev.len() - 1].trim_end().to_string();
        result.pop(); // remove "important"
        result.pop(); // remove "value!"
        if !trimmed.is_empty() {
          result.push(trimmed);
        }
        return result;
      }
    }
  }

  // Handle embedded !important at the end of the last token
  let last = result.last().unwrap().clone();
  let lower = last.to_ascii_lowercase();
  if lower.ends_with("!important") {
    let trimmed = last[..last.len() - 10].trim_end().to_string();
    result.pop();
    if !trimmed.is_empty() {
      result.push(trimmed);
    }
  }

  result
}

/// Check border-radius specifically, handling the `/` separator.
fn check_border_radius(
  raw_value: &str,
  value_offset: usize,
  decl: &gale_css_parser::Declaration,
  ctx: &RuleContext,
  rule_name: &str,
) -> Option<Diagnostic> {
  // Skip values with commas (not a real border-radius shorthand).
  if raw_value.contains(',') {
    return None;
  }

  // Skip non-standard syntax.
  if !is_standard_syntax_value(raw_value) {
    return None;
  }

  // Split on `/` for horizontal/vertical radii.
  let slash_parts: Vec<&str> = raw_value.splitn(2, '/').collect();
  if slash_parts.len() == 2 {
    let h_part = slash_parts[0].trim();
    let v_part = slash_parts[1].trim();

    // If both halves contain var(), skip the check entirely
    // (Stylelint ignores variables with slash in shorthand).
    let h_has_var = h_part.to_ascii_lowercase().contains("var(");
    let v_has_var = v_part.to_ascii_lowercase().contains("var(");
    if h_has_var && v_has_var {
      return None;
    }

    let h_tokens = split_value_tokens(h_part);
    let v_tokens = split_value_tokens(v_part);

    // Both halves must have 1-4 tokens.
    if h_tokens.is_empty() || h_tokens.len() > 4 || v_tokens.is_empty() || v_tokens.len() > 4 {
      return None;
    }

    let h_shortened = shorten_ci(&h_tokens);
    let v_shortened = shorten_ci(&v_tokens);

    let h_changed = h_shortened.is_some();
    let v_changed = v_shortened.is_some();

    if h_changed || v_changed {
      let h_result = h_shortened.unwrap_or_else(|| h_tokens.join(" "));
      let v_result = v_shortened.unwrap_or_else(|| v_tokens.join(" "));
      let shortened = format!("{} / {}", h_result, v_result);

      let fix = build_fix_from_raw(ctx.source, decl, raw_value, &shortened);
      let mut diag = Diagnostic::new(
        rule_name,
        format!("Expected \"{}\" to be \"{}\"", raw_value, shortened),
      )
      .severity(Severity::Warning)
      .span(Span::new(value_offset, raw_value.len()));

      if let Some(f) = fix {
        diag = diag.fix(f);
      }

      return Some(diag);
    }
  } else {
    // No slash — treat like a normal shorthand.
    let tokens = split_value_tokens(raw_value);
    if tokens.len() < 2 || tokens.len() > 4 {
      return None;
    }
    if let Some(shortened) = shorten_ci(&tokens) {
      let fix = build_fix_from_raw(ctx.source, decl, raw_value, &shortened);
      let mut diag = Diagnostic::new(
        rule_name,
        format!("Expected \"{}\" to be \"{}\"", raw_value, shortened),
      )
      .severity(Severity::Warning)
      .span(Span::new(value_offset, raw_value.len()));

      if let Some(f) = fix {
        diag = diag.fix(f);
      }

      return Some(diag);
    }
  }
  None
}

/// Build a Fix that replaces the raw value in source.
fn build_fix_from_raw(
  source: &str,
  decl: &gale_css_parser::Declaration,
  raw_value: &str,
  shortened: &str,
) -> Option<Fix> {
  let decl_start = decl.span.offset;
  let decl_end = decl_start + decl.span.length;
  if decl_end > source.len() {
    return None;
  }
  let decl_text = &source[decl_start..decl_end];

  let colon_pos = decl_text.find(':')?;
  let after_colon = &decl_text[colon_pos + 1..];
  let leading_ws = after_colon.len() - after_colon.trim_start().len();
  let value_start_in_source = decl_start + colon_pos + 1 + leading_ws;
  let value_end_in_source = value_start_in_source + raw_value.len();

  Some(Fix::new(
    format!("Shorten to \"{shortened}\""),
    vec![Edit::new(
      Span::from_range(value_start_in_source, value_end_in_source),
      shortened,
    )],
  ))
}

/// Try to shorten redundant values. Comparison is case-insensitive.
/// Returns Some(shortened) if redundant.
fn shorten(parts: &[String], ignore_four_into_three: bool) -> Option<String> {
  shorten_inner(parts, ignore_four_into_three)
}

/// Case-insensitive shorten (for border-radius parts).
fn shorten_ci(parts: &[String]) -> Option<String> {
  shorten_inner(parts, false)
}

fn shorten_inner(parts: &[String], ignore_four_into_three: bool) -> Option<String> {
  let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);

  match parts.len() {
    4 => {
      let (top, right, bottom, left) = (&parts[0], &parts[1], &parts[2], &parts[3]);
      if eq(top, right) && eq(right, bottom) && eq(bottom, left) {
        // 1px 1px 1px 1px → 1px
        Some(top.to_string())
      } else if eq(top, bottom) && eq(right, left) {
        // 1px 2px 1px 2px → 1px 2px
        Some(format!("{top} {right}"))
      } else if eq(right, left) {
        if ignore_four_into_three {
          return None;
        }
        // 1px 2px 3px 2px → 1px 2px 3px
        Some(format!("{top} {right} {bottom}"))
      } else {
        None
      }
    }
    3 => {
      let (top, right, bottom) = (&parts[0], &parts[1], &parts[2]);
      if eq(top, right) && eq(right, bottom) {
        Some(top.to_string())
      } else if eq(top, bottom) {
        Some(format!("{top} {right}"))
      } else {
        None
      }
    }
    2 => {
      if eq(&parts[0], &parts[1]) {
        Some(parts[0].to_string())
      } else {
        None
      }
    }
    _ => None,
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

  /// Helper to build a CssNode with a correct source-backed declaration span.
  fn build_node_from_source(source: &str) -> (CssNode, RuleContext<'static>) {
    // Parse with the real parser to get accurate spans.
    // However, we need a raw source check, so let's build manually.
    // Source format: `a { property: value; }`
    // Find the declaration within the source.
    let decl_start = source
      .find(|c: char| c.is_alphabetic() && c != 'a')
      .unwrap_or(4);
    // Actually, let's find past "a { "
    let brace_pos = source.find('{').unwrap();
    let after_brace = &source[brace_pos + 1..];
    let trimmed = after_brace.trim_start();
    let prop_start = source.len() - after_brace.len() + (after_brace.len() - trimmed.len());

    // Find the end of the declaration (before closing brace).
    let end_brace = source.rfind('}').unwrap();
    let decl_text = source[prop_start..end_brace]
      .trim_end()
      .trim_end_matches(';');
    let decl_end = prop_start + decl_text.len();

    let colon = decl_text.find(':').unwrap();
    let property = decl_text[..colon].trim().to_string();
    let value_raw = decl_text[colon + 1..].trim().to_string();

    // For the value field, just use the raw value since our rule reads from source.
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property,
        value: value_raw.clone(),
        span: ParserSpan::new(prop_start, decl_end - prop_start + 1),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });

    // Leak the source string so we get a 'static lifetime for tests.
    let source_leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    let context = RuleContext {
      file_path: "t.css",
      source: source_leaked,
      syntax: Syntax::Css,
      options: None,
    };

    (node, context)
  }

  #[test]
  fn reports_four_identical_values() {
    let source = "a { margin: 1px 1px 1px 1px; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("1px"));
  }

  #[test]
  fn reports_two_identical_values() {
    let source = "a { padding: 10px 10px; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_non_redundant_values() {
    let source = "a { margin: 1px 2px 3px 4px; }";
    let (node, ctx) = build_node_from_source(source);
    assert!(
      ShorthandPropertyNoRedundantValues
        .check(&node, &ctx)
        .is_empty()
    );

    let source2 = "a { margin: 1px; }";
    let (node2, ctx2) = build_node_from_source(source2);
    assert!(
      ShorthandPropertyNoRedundantValues
        .check(&node2, &ctx2)
        .is_empty()
    );
  }

  #[test]
  fn case_insensitive_comparison() {
    let source = "a { margin: 1Px 1pX; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn handles_important() {
    let source = "a { margin: 1px 1px !important; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn handles_border_radius_with_slash() {
    let source = "a { border-radius: 1px 1px 1px 1px / 2px 2px 2px 2px; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_var_functions() {
    let source = "a { margin: var(--margin) var(--margin); }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert!(d.is_empty(), "var() values should be skipped");
  }

  #[test]
  fn handles_calc_functions() {
    let source = "a { margin: calc(1px + 1px) calc(1px + 1px); }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn vendor_prefixed_property() {
    let source = "a { -webkit-border-radius: 1px 1px 1px 1px; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn skips_non_shorthand() {
    let source = "a { border: 5px solid red; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn overflow_shorthand() {
    let source = "a { overflow: scroll scroll; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn gap_shorthand() {
    let source = "a { gap: 1rem 1rem; }";
    let (node, ctx) = build_node_from_source(source);
    let d = ShorthandPropertyNoRedundantValues.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }
}
