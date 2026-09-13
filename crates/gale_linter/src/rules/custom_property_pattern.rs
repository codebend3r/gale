use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};
use crate::stylelint_version::stylelint_major_version;

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

/// Returns true if the string has a COMPLETE SCSS interpolation (`#{...}`).
/// Mirrors Stylelint's `hasScssInterpolation` which uses `/#\{.+?\}/s`.
fn has_complete_scss_interpolation(s: &str) -> bool {
  let bytes = s.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i + 1 < len {
    if bytes[i] == b'#' && bytes[i + 1] == b'{' {
      // Found `#{` — check if there's a closing `}`
      if s[i + 2..].contains('}') {
        return true;
      }
    }
    i += 1;
  }
  false
}

/// Build the diagnostic message for a property name.
///
/// Stylelint 16+ uses `Expected "${name}" to match pattern "${pattern}"`.
/// Stylelint <=15 uses `Expected custom property name to match pattern "${pattern}"`.
/// Custom message templates (from JS arrow function conversion) override both.
fn build_message(
  prop_name: &str,
  custom_message: Option<&str>,
  pattern_str: &str,
  stylelint_major: u32,
) -> String {
  if let Some(template) = custom_message {
    // Replace `${name}` placeholder with the actual property name.
    // This handles messages converted from JS arrow functions:
    // `(name) => `...${name}...`` → `"...${name}..."`
    template.replace("${name}", prop_name)
  } else if stylelint_major >= 14 {
    format!("Expected \"{prop_name}\" to match pattern \"{pattern_str}\"")
  } else {
    format!("Expected custom property name to match pattern \"{pattern_str}\"")
  }
}

/// Extract the first token of a `var()` call argument from the given value string.
/// Returns `(token, offset_in_value)` where `offset_in_value` is the byte position
/// of the token within `value`.
///
/// Mimics postcss-value-parser's word tokenization: reads characters until a
/// quote (`'` or `"`), comma (`,`), close-paren (`)`), open-paren (`(`), or
/// whitespace is encountered. This matches how Stylelint identifies the custom
/// property name inside `var()`.
fn extract_var_first_token(value: &str) -> Option<(&str, usize)> {
  // Find `var(` case-insensitively
  let var_start = value
    .as_bytes()
    .windows(4)
    .position(|w| w.eq_ignore_ascii_case(b"var("))?;
  let arg_start = var_start + 4;

  let bytes = value.as_bytes();
  let len = bytes.len();

  // Skip leading whitespace
  let mut pos = arg_start;
  while pos < len && (bytes[pos] == b' ' || bytes[pos] == b'\t' || bytes[pos] == b'\n') {
    pos += 1;
  }

  if pos >= len {
    return None;
  }

  // The argument must start with `--` to be a custom property reference
  if !value[pos..].starts_with("--") {
    return None;
  }

  // Read until: single quote, double quote, comma, close-paren, open-paren, or whitespace
  let token_start = pos;
  while pos < len {
    match bytes[pos] {
      b'\'' | b'"' | b',' | b')' | b'(' | b' ' | b'\t' | b'\n' => break,
      _ => pos += 1,
    }
  }

  if pos == token_start {
    return None;
  }

  Some((&value[token_start..pos], token_start))
}

/// Enforce a naming pattern for custom properties (CSS variables).
///
/// Equivalent to Stylelint's `custom-property-pattern` rule.
/// Accepts a regex string as the primary option.
/// Default pattern: kebab-case (`^([a-z][a-z0-9]*)(-[a-z0-9]+)*$`).
pub struct CustomPropertyPattern;

impl Rule for CustomPropertyPattern {
  fn name(&self) -> &'static str {
    "custom-property-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for custom properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read the user-supplied regex pattern from options, or use the default kebab-case pattern.
    // Options may be a plain string (`"^pattern$"`) or an array where the
    // first element is the pattern and the second is a secondary options
    // object (e.g. `["^pattern$", { "message": "..." }]`).
    let pattern_str = ctx.primary_option_str().unwrap_or(DEFAULT_PATTERN);

    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    // Check for custom message in secondary options
    let custom_message = ctx
      .secondary_options()
      .and_then(|v| v.get("message"))
      .and_then(|v| v.as_str());

    let is_scss = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    );

    let stylelint_major = stylelint_major_version();

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      // --- Check 1: custom property DEFINITION (left side) ---
      if let Some(name) = decl.property.strip_prefix("--") {
        // Skip names with COMPLETE SCSS interpolation `#{...}`.
        // Mirrors Stylelint's isStandardSyntaxProperty check.
        if is_scss && has_complete_scss_interpolation(name) {
          // skip — non-standard syntax
        } else if !re.is_match(name) {
          let full_name = format!("--{name}");
          let message = build_message(&full_name, custom_message, pattern_str, stylelint_major);
          diags.push(
            Diagnostic::new(self.name(), message)
              .severity(self.default_severity())
              .span(Span::new(decl.span.offset, decl.span.length)),
          );
        }
      }

      // --- Check 2: custom property REFERENCES inside var() in values ---
      // Stylelint 14+ checks var() arguments; older versions (<=13) only
      // check property definitions (left side of declarations).
      if stylelint_major >= 14 {
        if decl.value.contains("var(") || decl.value.contains("VAR(") {
          if let Some((prop_name, token_offset_in_value)) = extract_var_first_token(&decl.value) {
            if let Some(name) = prop_name.strip_prefix("--") {
              // Same SCSS interpolation skip as above
              let skip = is_scss && has_complete_scss_interpolation(name);
              if !skip && !re.is_match(name) {
                // Compute absolute byte offset of the token in the source.
                let decl_start = decl.span.offset;
                let decl_end = (decl.span.offset + decl.span.length).min(ctx.source.len());
                let decl_text = &ctx.source[decl_start..decl_end];
                let value_pos_in_decl = decl_text.find(&decl.value).unwrap_or(0);
                let token_abs_offset = decl_start + value_pos_in_decl + token_offset_in_value;

                let message =
                  build_message(prop_name, custom_message, pattern_str, stylelint_major);
                diags.push(
                  Diagnostic::new(self.name(), message)
                    .severity(self.default_severity())
                    .span(Span::new(token_abs_offset, prop_name.len())),
                );
              }
            }
          }
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

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn style_with_property(prop: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: "#fff".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_non_kebab_custom_property() {
    let d = CustomPropertyPattern.check(&style_with_property("--myColor"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myColor"));
  }

  #[test]
  fn allows_kebab_case_custom_property() {
    let d = CustomPropertyPattern.check(&style_with_property("--my-color"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_regular_properties() {
    let d = CustomPropertyPattern.check(&style_with_property("color"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn custom_pattern() {
    let opts = serde_json::json!("^pf-(v[56]|t-)-(color|global|chart|[lcud])-.+$");
    let c = ctx_with_options(&opts);
    // Should pass: matches the custom pattern
    assert!(
      CustomPropertyPattern
        .check(&style_with_property("--pf-v5-color-primary"), &c)
        .is_empty()
    );
    // Should fail: doesn't match the custom pattern
    let d = CustomPropertyPattern.check(&style_with_property("--my-color"), &c);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn custom_pattern_in_message() {
    let opts = serde_json::json!("^pf-");
    let c = ctx_with_options(&opts);
    let d = CustomPropertyPattern.check(&style_with_property("--my-color"), &c);
    assert!(d[0].message.contains("^pf-"));
  }

  #[test]
  fn array_format_options_with_secondary() {
    // Config like: ["^sds|slds-c|kx-...", { "message": "..." }]
    let opts = serde_json::json!(["^sds|slds-c|kx-([a-z][a-z0-9]*)(-[a-z0-9]+)*$", { "message": "Custom msg" }]);
    let c = ctx_with_options(&opts);
    // _slds-c-... should match via the `slds-c` alternative
    assert!(
      CustomPropertyPattern
        .check(&style_with_property("--_slds-c-accordion-spacing"), &c)
        .is_empty(),
      "Should not flag --_slds-c-... with the SLDS pattern"
    );
    // my-color should be flagged (doesn't match the pattern)
    let d = CustomPropertyPattern.check(&style_with_property("--my-color"), &c);
    assert_eq!(d.len(), 1);
  }
}
