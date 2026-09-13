use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Prefer numeric or named font-weight notation.
///
/// Options:
/// - `"numeric"` (default): require numeric values (`400`, `700`) instead of
///   named keywords (`normal`, `bold`).
/// - `"named-where-possible"`: require named keywords where a numeric value
///   has an equivalent name (`400` → `normal`, `700` → `bold`).
///
/// Secondary options:
/// - `ignore: ["relative"]`: ignore `bolder`/`lighter` (and in `numeric`
///   mode also allow `bold`/`normal` when this option is set — but actually
///   Stylelint's `ignore: ["relative"]` specifically skips `bolder`/`lighter`
///   for numeric mode).
///
/// Equivalent to Stylelint's `font-weight-notation` rule.
pub struct FontWeightNotation;

/// Named font-weight keywords that have numeric equivalents.
const NAMED_WEIGHTS: &[&str] = &["bold", "normal"];

/// Relative font-weight keywords.
const RELATIVE_WEIGHTS: &[&str] = &["bolder", "lighter"];

/// CSS-wide keywords.
const CSS_WIDE_KEYWORDS: &[&str] = &["inherit", "initial", "unset", "revert", "revert-layer"];

/// Map named weight to numeric equivalent.
fn named_to_numeric(name: &str) -> Option<&'static str> {
  match name.to_ascii_lowercase().as_str() {
    "normal" => Some("400"),
    "bold" => Some("700"),
    _ => None,
  }
}

/// Map numeric weight to named equivalent (only for named-where-possible).
fn numeric_to_named(num: &str) -> Option<&'static str> {
  match num.trim() {
    "400" => Some("normal"),
    "700" => Some("bold"),
    _ => None,
  }
}

fn is_css_wide_keyword(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  CSS_WIDE_KEYWORDS.iter().any(|kw| *kw == lower)
}

fn is_relative_weight(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  RELATIVE_WEIGHTS.iter().any(|kw| *kw == lower)
}

fn is_named_weight(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  NAMED_WEIGHTS.iter().any(|kw| *kw == lower)
}

fn is_variable_or_function(s: &str) -> bool {
  s.starts_with('$')
    || s.starts_with('@')
    || s.starts_with("var(")
    || s.contains("#{")
    || s.contains("map-deep-get(")
    || s.contains('(')
}

/// Strip CSS comments from a value, replacing comment characters with spaces
/// to preserve byte offsets (so token positions in the result match the original).
fn strip_comments(value: &str) -> String {
  let mut result = value.as_bytes().to_vec();
  let len = result.len();
  let mut i = 0;
  while i + 1 < len {
    if result[i] == b'/' && result[i + 1] == b'*' {
      result[i] = b' ';
      result[i + 1] = b' ';
      i += 2;
      while i + 1 < len {
        if result[i] == b'*' && result[i + 1] == b'/' {
          result[i] = b' ';
          result[i + 1] = b' ';
          i += 2;
          break;
        }
        result[i] = b' ';
        i += 1;
      }
    } else {
      i += 1;
    }
  }
  String::from_utf8(result).unwrap_or_else(|_| value.to_string())
}

/// Find the byte offset of a specific token within the value string.
fn find_token_offset(value: &str, token: &str) -> Option<usize> {
  let lower_value = value.to_ascii_lowercase();
  let lower_token = token.to_ascii_lowercase();
  lower_value.find(&lower_token)
}

/// Given a declaration, find the byte offset in the source where the value
/// begins (after the `:` and any whitespace).
fn find_value_offset(source: &str, decl_offset: usize, property_len: usize) -> usize {
  let start = decl_offset + property_len;
  if start >= source.len() {
    return decl_offset;
  }
  let rest = &source[start..];
  let mut off = 0;
  let bytes = rest.as_bytes();
  while off < bytes.len() && (bytes[off] == b':' || bytes[off].is_ascii_whitespace()) {
    off += 1;
  }
  start + off
}

/// Check if we are inside an @font-face rule.
fn is_in_font_face(node: &CssNode) -> bool {
  // The node itself is a StyleRule, but in Stylelint's AST, @font-face
  // declarations appear inside an AtRule. Our AST might embed them differently.
  // We check the selector for @font-face pattern.
  if let CssNode::Style(rule) = node {
    // In our parser, @font-face may appear as an at-rule, not a style rule.
    return false;
    let _ = rule;
  }
  false
}

impl Rule for FontWeightNotation {
  fn name(&self) -> &'static str {
    "font-weight-notation"
  }

  fn description(&self) -> &'static str {
    "Require numeric or named font-weight values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    match node {
      CssNode::Style(rule) => {
        self.check_declarations(&rule.declarations, ctx, &mut diags, false);
      }
      CssNode::AtRule(at_rule) => {
        let is_font_face = at_rule.name.eq_ignore_ascii_case("font-face");
        // Check declarations inside at-rules (like @font-face)
        for child in &at_rule.children {
          if let CssNode::Style(rule) = child {
            self.check_declarations(&rule.declarations, ctx, &mut diags, is_font_face);
          }
          if let CssNode::Declaration(decl) = child {
            self.check_single_declaration(decl, ctx, &mut diags, is_font_face);
          }
        }
      }
      _ => {}
    }

    diags
  }
}

impl FontWeightNotation {
  fn check_declarations(
    &self,
    declarations: &[gale_css_parser::Declaration],
    ctx: &RuleContext,
    diags: &mut Vec<Diagnostic>,
    is_font_face: bool,
  ) {
    for decl in declarations {
      self.check_single_declaration(decl, ctx, diags, is_font_face);
    }
  }

  fn check_single_declaration(
    &self,
    decl: &gale_css_parser::Declaration,
    ctx: &RuleContext,
    diags: &mut Vec<Diagnostic>,
    is_font_face: bool,
  ) {
    let mode = ctx.primary_option_str().unwrap_or("numeric");

    let ignore_relative = ctx
      .secondary_options()
      .and_then(|v| v.get("ignore"))
      .and_then(|v| v.as_array())
      .map(|arr| arr.iter().any(|v| v.as_str() == Some("relative")))
      .unwrap_or(false);

    let prop_lower = decl.property.to_ascii_lowercase();

    // Use source text when available for accurate positions and values.
    let (raw_value, value_offset) = if !ctx.source.is_empty() {
      let vo = find_value_offset(ctx.source, decl.span.offset, decl.property.len());
      // Find end of value (semicolon or closing brace)
      let rest = &ctx.source[vo..];
      let end = rest
        .find([';', '}'])
        .map(|i| vo + i)
        .unwrap_or(vo + rest.len());
      let raw = ctx.source[vo..end].trim().to_string();
      (raw, vo)
    } else {
      let vo = decl.span.offset + decl.property.len() + 2;
      (decl.value.clone(), vo)
    };

    let clean_value = strip_comments(&raw_value);
    let value_trimmed = clean_value.trim();

    // Skip variables and functions
    if is_variable_or_function(value_trimmed) {
      return;
    }

    if prop_lower == "font-weight" {
      if is_font_face {
        // In @font-face, font-weight can be a range like "400 700" or "normal bold"
        let tokens: Vec<&str> = value_trimmed.split_whitespace().collect();
        for token in &tokens {
          if is_css_wide_keyword(token) || is_variable_or_function(token) {
            continue;
          }
          if is_relative_weight(token) {
            continue;
          }
          self.check_weight_token(
            token,
            mode,
            ignore_relative,
            decl,
            ctx,
            diags,
            &clean_value,
            value_offset,
          );
        }
      } else {
        if is_css_wide_keyword(value_trimmed) {
          return;
        }
        if is_relative_weight(value_trimmed) {
          return;
        }
        self.check_weight_token(
          value_trimmed,
          mode,
          ignore_relative,
          decl,
          ctx,
          diags,
          &clean_value,
          value_offset,
        );
      }
    } else if prop_lower == "font" {
      self.check_font_shorthand(
        value_trimmed,
        mode,
        ignore_relative,
        decl,
        ctx,
        diags,
        &clean_value,
        value_offset,
      );
    }
  }

  fn check_weight_token(
    &self,
    token: &str,
    mode: &str,
    ignore_relative: bool,
    decl: &gale_css_parser::Declaration,
    ctx: &RuleContext,
    diags: &mut Vec<Diagnostic>,
    clean_value: &str,
    value_offset: usize,
  ) {
    let lower = token.to_ascii_lowercase();
    // Skip fractional values (valid in @font-face)
    if token.contains('.') {
      return;
    }

    match mode {
      "numeric" => {
        if is_named_weight(&lower) {
          let token_offset = find_token_in_value(clean_value, token)
            .map(|off| value_offset + off)
            .unwrap_or(value_offset);
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Expected numeric font-weight notation instead of \"{}\"",
                lower
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(token_offset, token.len())),
          );
        }
      }
      "named-where-possible" => {
        // Flag numeric values that have named equivalents
        if let Some(named) = numeric_to_named(token) {
          let token_offset = find_token_in_value(clean_value, token)
            .map(|off| value_offset + off)
            .unwrap_or(value_offset);
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Expected named font-weight notation \"{}\" instead of \"{}\"",
                named, token
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(token_offset, token.len())),
          );
        }
      }
      _ => {}
    }
  }

  fn check_font_shorthand(
    &self,
    value: &str,
    mode: &str,
    ignore_relative: bool,
    decl: &gale_css_parser::Declaration,
    ctx: &RuleContext,
    diags: &mut Vec<Diagnostic>,
    clean_value: &str,
    value_offset: usize,
  ) {
    // In the font shorthand, the weight is one of the first tokens
    // (before the font-size). We need to identify which token is the weight.
    // Font shorthand: [style] [variant] [weight] [stretch] size[/line-height] family
    //
    // The weight position is before the font-size. We tokenize and check
    // each token that could be a weight.
    let tokens: Vec<&str> = value.split_whitespace().collect();

    // Find the font-size token (first token containing a digit followed by a unit,
    // or has a / for line-height).
    let mut size_idx = None;
    for (i, t) in tokens.iter().enumerate() {
      let lower = t.to_ascii_lowercase();
      if lower
        .chars()
        .next()
        .map(|c| c.is_ascii_digit() || c == '.')
        .unwrap_or(false)
        && (lower.ends_with("px")
          || lower.ends_with("em")
          || lower.ends_with("rem")
          || lower.ends_with("pt")
          || lower.ends_with('%')
          || lower.ends_with("vw")
          || lower.ends_with("vh")
          || lower.ends_with("ex")
          || lower.ends_with("ch")
          || lower.contains('/'))
      {
        size_idx = Some(i);
        break;
      }
    }

    // Check tokens before the size for weight values
    let check_up_to = size_idx.unwrap_or(tokens.len());

    // In numeric mode, check if there's an explicit numeric weight before the size.
    // If so, "normal" tokens are treated as font-style/variant, not weight.
    let has_numeric_weight = tokens[..check_up_to]
      .iter()
      .any(|t| t.parse::<u32>().is_ok());

    for i in 0..check_up_to {
      let token = tokens[i];
      let lower = token.to_ascii_lowercase();

      // Skip CSS-wide keywords
      if is_css_wide_keyword(&lower) {
        continue;
      }

      // Skip quoted font names that appear early (shouldn't happen before size, but be safe)
      if token.starts_with('"') || token.starts_with('\'') {
        continue;
      }

      match mode {
        "numeric" => {
          if lower == "bold" {
            // "bold" in font shorthand is unambiguously a weight
            let token_offset = find_token_in_value(clean_value, token)
              .map(|off| value_offset + off)
              .unwrap_or(value_offset);
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Expected numeric font-weight notation instead of \"{}\" in font shorthand",
                  lower
                ),
              )
              .severity(self.default_severity())
              .span(Span::new(token_offset, token.len())),
            );
          } else if lower == "normal" {
            // "normal" in font shorthand is ambiguous — could be
            // font-style, font-variant, or font-weight.
            // Only flag if there's no explicit numeric weight (meaning
            // "normal" is the weight).
            if !has_numeric_weight {
              let token_offset = find_token_in_value(clean_value, token)
                .map(|off| value_offset + off)
                .unwrap_or(value_offset);
              diags.push(
                Diagnostic::new(
                  self.name(),
                  format!(
                    "Expected numeric font-weight notation instead of \"{}\" in font shorthand",
                    lower
                  ),
                )
                .severity(self.default_severity())
                .span(Span::new(token_offset, token.len())),
              );
            }
          } else if is_relative_weight(&lower) {
            if ignore_relative {
              continue;
            }
            let token_offset = find_token_in_value(clean_value, token)
              .map(|off| value_offset + off)
              .unwrap_or(value_offset);
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Expected numeric font-weight notation instead of \"{}\" in font shorthand",
                  lower
                ),
              )
              .severity(self.default_severity())
              .span(Span::new(token_offset, token.len())),
            );
          }
        }
        "named-where-possible" => {
          // Check if a numeric token in weight position has a named equivalent
          if let Some(named) = numeric_to_named(token) {
            let token_offset = find_token_in_value(clean_value, token)
              .map(|off| value_offset + off)
              .unwrap_or(value_offset);
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Expected named font-weight notation \"{}\" instead of \"{}\" in font shorthand",
                  named, token
                ),
              )
              .severity(self.default_severity())
              .span(Span::new(token_offset, token.len())),
            );
          }
        }
        _ => {}
      }
    }

    // Also check if a font name after size accidentally matches weight keywords.
    // We need to NOT flag "bold" if it appears inside a quoted font name like "bold font name"
    // or as part of an unquoted font name like "boldfontname".
  }
}

/// Find the byte offset of a token within the value, matching case-insensitively
/// and ensuring it's a whole word.
fn find_token_in_value(value: &str, token: &str) -> Option<usize> {
  let lower_value = value.to_ascii_lowercase();
  let lower_token = token.to_ascii_lowercase();
  let mut start = 0;
  while let Some(pos) = lower_value[start..].find(&lower_token) {
    let abs_pos = start + pos;
    let before_ok = abs_pos == 0 || !value.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
    let after_pos = abs_pos + lower_token.len();
    let after_ok = after_pos >= value.len() || !value.as_bytes()[after_pos].is_ascii_alphanumeric();
    if before_ok && after_ok {
      return Some(abs_pos);
    }
    start = abs_pos + 1;
  }
  None
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

  fn style_with_decl(prop: &str, value: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: value.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_bold_keyword() {
    let d = FontWeightNotation.check(&style_with_decl("font-weight", "bold"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bold"));
  }

  #[test]
  fn reports_normal_keyword() {
    let d = FontWeightNotation.check(&style_with_decl("font-weight", "normal"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("normal"));
  }

  #[test]
  fn allows_numeric_weight() {
    let d = FontWeightNotation.check(&style_with_decl("font-weight", "700"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_bold_in_font_shorthand() {
    let d = FontWeightNotation.check(&style_with_decl("font", "bold 16px Arial"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bold"));
  }

  #[test]
  fn allows_numeric_in_font_shorthand() {
    let d = FontWeightNotation.check(&style_with_decl("font", "700 16px Arial"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_inherit() {
    let d = FontWeightNotation.check(&style_with_decl("font-weight", "inherit"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_bolder() {
    let d = FontWeightNotation.check(&style_with_decl("font-weight", "bolder"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_lighter() {
    let d = FontWeightNotation.check(&style_with_decl("font-weight", "lighter"), &ctx());
    assert!(d.is_empty());
  }
}
