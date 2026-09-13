use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow unknown values for known properties.
///
/// This is a conservative first implementation that checks a small set of
/// properties with well-defined keyword values. It intentionally skips
/// validation for:
/// - Values containing `var()`, `env()`, or other function calls
/// - Custom properties (`--*`)
/// - Values with CSS-wide keywords (`inherit`, `initial`, `unset`, `revert`, `revert-layer`)
///
/// Equivalent to Stylelint's `declaration-property-value-no-unknown` rule.
pub struct DeclarationPropertyValueNoUnknown;

/// CSS-wide keywords valid for any property.
const CSS_WIDE_KEYWORDS: &[&str] = &["inherit", "initial", "revert", "revert-layer", "unset"];

/// Valid keywords for the `display` property.
const DISPLAY_KEYWORDS: &[&str] = &[
  "block",
  "contents",
  "flex",
  "flow-root",
  "grid",
  "inline",
  "inline-block",
  "inline-flex",
  "inline-grid",
  "inline-table",
  "list-item",
  "none",
  "ruby",
  "ruby-base",
  "ruby-base-container",
  "ruby-text",
  "ruby-text-container",
  "run-in",
  "table",
  "table-caption",
  "table-cell",
  "table-column",
  "table-column-group",
  "table-footer-group",
  "table-header-group",
  "table-row",
  "table-row-group",
];

/// Valid keywords for the `position` property.
const POSITION_KEYWORDS: &[&str] = &["absolute", "fixed", "relative", "static", "sticky"];

/// Valid keywords for the `float` property.
const FLOAT_KEYWORDS: &[&str] = &["inline-end", "inline-start", "left", "none", "right"];

impl Rule for DeclarationPropertyValueNoUnknown {
  fn name(&self) -> &'static str {
    "declaration-property-value-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown values for known properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };
    let mut all_diags = Vec::new();
    for decl in decls {
      all_diags.extend(self.check_decl(decl));
    }
    all_diags
  }
}

impl DeclarationPropertyValueNoUnknown {
  fn check_decl(&self, decl: &gale_css_parser::Declaration) -> Vec<Diagnostic> {
    let property = decl.property.to_ascii_lowercase();

    // Skip custom properties
    if property.starts_with("--") {
      return vec![];
    }

    let value = decl.value.trim().to_ascii_lowercase();

    // Skip empty values (parser may handle these)
    if value.is_empty() {
      return vec![];
    }

    // Skip values containing function calls — too complex to validate statically
    if contains_function_call(&value) {
      return vec![];
    }

    // Skip CSS-wide keywords (valid for any property)
    if is_css_wide_keyword(&value) {
      return vec![];
    }

    // Skip values with !important stripped (already handled by parser, but be safe)
    let value_clean = value
      .strip_suffix("!important")
      .map(|v| v.trim())
      .unwrap_or(&value);

    if value_clean.is_empty() || is_css_wide_keyword(value_clean) {
      return vec![];
    }

    // Only validate properties we have keyword lists for
    let valid_keywords: Option<&[&str]> = match property.as_str() {
      "display" => Some(DISPLAY_KEYWORDS),
      "position" => Some(POSITION_KEYWORDS),
      "float" => Some(FLOAT_KEYWORDS),
      "text-decoration-skip" => Some(&["none", "auto", "ink", "spaces", "edges", "box-decoration"]),
      _ => None,
    };

    let Some(keywords) = valid_keywords else {
      return vec![];
    };

    // For display, handle multi-keyword syntax (e.g., "inline flex")
    // by checking if each token is valid individually when there are
    // multiple tokens. For single-token values, check directly.
    let tokens: Vec<&str> = value_clean.split_whitespace().collect();

    // For properties that only accept single keywords
    if property == "position" || property == "float" || property == "text-decoration-skip" {
      if tokens.len() != 1 {
        return vec![
          Diagnostic::new(
            self.name(),
            format!(
              "Unexpected unknown value \"{}\" for property \"{}\"",
              decl.value.trim(),
              decl.property
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        ];
      }
      if !keywords.contains(&tokens[0]) {
        let val_off = decl.span.offset + decl.property.len() + 2;
        return vec![
          Diagnostic::new(
            self.name(),
            format!(
              "Unexpected unknown value \"{}\" for property \"{}\"",
              decl.value.trim(),
              decl.property
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(val_off, decl.value.trim().len())),
        ];
      }
      return vec![];
    }

    // For display: check single-token values against the keyword list.
    // Multi-token display values (CSS Display Level 3) like "inline flex" are
    // harder to validate, so we skip them for now.
    if tokens.len() == 1 && !keywords.contains(&tokens[0]) {
      return vec![
        Diagnostic::new(
          self.name(),
          format!(
            "Unexpected unknown value \"{}\" for property \"{}\"",
            decl.value.trim(),
            decl.property
          ),
        )
        .severity(self.default_severity())
        .span({
          // Point to the value, not the whole declaration.
          // Find ": " after the property name within the span.
          let val_offset = decl.span.offset + decl.property.len();
          // Skip colon and whitespace
          let val_offset = val_offset + 2; // ": "
          Span::new(val_offset, decl.value.trim().len())
        }),
      ];
    }

    vec![]
  }
}

/// Returns true if the value contains a CSS function call like `var()`, `env()`,
/// `calc()`, `rgb()`, etc.
fn contains_function_call(value: &str) -> bool {
  // Look for pattern: identifier followed by '('
  let bytes = value.as_bytes();
  for i in 0..bytes.len() {
    if bytes[i] == b'(' && i > 0 {
      // Check that the character before '(' is part of a function name
      let prev = bytes[i - 1];
      if prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_' {
        return true;
      }
    }
  }
  false
}

fn is_css_wide_keyword(value: &str) -> bool {
  CSS_WIDE_KEYWORDS.contains(&value)
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn decl(property: &str, value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: property.to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, 0),
      important: false,
    })
  }

  // --- display ---

  #[test]
  fn allows_valid_display_values() {
    for kw in &[
      "block",
      "inline",
      "flex",
      "grid",
      "none",
      "contents",
      "inline-block",
      "inline-flex",
      "inline-grid",
      "table",
      "flow-root",
      "list-item",
    ] {
      let d = DeclarationPropertyValueNoUnknown.check(&decl("display", kw), &ctx());
      assert!(d.is_empty(), "Expected '{}' to be valid for display", kw);
    }
  }

  #[test]
  fn reports_invalid_display_value() {
    let d = DeclarationPropertyValueNoUnknown.check(&decl("display", "banana"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("banana"));
    assert!(d[0].message.contains("display"));
  }

  #[test]
  fn allows_css_wide_keywords_for_display() {
    for kw in &["inherit", "initial", "unset", "revert", "revert-layer"] {
      let d = DeclarationPropertyValueNoUnknown.check(&decl("display", kw), &ctx());
      assert!(d.is_empty(), "Expected '{}' to be valid for display", kw);
    }
  }

  #[test]
  fn skips_display_with_var() {
    let d = DeclarationPropertyValueNoUnknown.check(&decl("display", "var(--my-display)"), &ctx());
    assert!(d.is_empty());
  }

  // --- position ---

  #[test]
  fn allows_valid_position_values() {
    for kw in &["static", "relative", "absolute", "fixed", "sticky"] {
      let d = DeclarationPropertyValueNoUnknown.check(&decl("position", kw), &ctx());
      assert!(d.is_empty(), "Expected '{}' to be valid for position", kw);
    }
  }

  #[test]
  fn reports_invalid_position_value() {
    let d = DeclarationPropertyValueNoUnknown.check(&decl("position", "floating"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("floating"));
    assert!(d[0].message.contains("position"));
  }

  // --- float ---

  #[test]
  fn allows_valid_float_values() {
    for kw in &["left", "right", "none", "inline-start", "inline-end"] {
      let d = DeclarationPropertyValueNoUnknown.check(&decl("float", kw), &ctx());
      assert!(d.is_empty(), "Expected '{}' to be valid for float", kw);
    }
  }

  #[test]
  fn reports_invalid_float_value() {
    let d = DeclarationPropertyValueNoUnknown.check(&decl("float", "center"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("center"));
    assert!(d[0].message.contains("float"));
  }

  // --- skip cases ---

  #[test]
  fn skips_custom_properties() {
    let d = DeclarationPropertyValueNoUnknown.check(&decl("--my-prop", "anything"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_unknown_properties() {
    // We don't validate properties we don't have keyword lists for
    let d = DeclarationPropertyValueNoUnknown.check(&decl("color", "banana"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_values_with_env() {
    let d =
      DeclarationPropertyValueNoUnknown.check(&decl("display", "env(safe-area-inset-top)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_multi_token_display() {
    // CSS Display Level 3 multi-keyword syntax
    let d = DeclarationPropertyValueNoUnknown.check(&decl("display", "inline flex"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_multiple_tokens_for_position() {
    let d = DeclarationPropertyValueNoUnknown.check(&decl("position", "absolute fixed"), &ctx());
    assert_eq!(d.len(), 1);
  }
}
