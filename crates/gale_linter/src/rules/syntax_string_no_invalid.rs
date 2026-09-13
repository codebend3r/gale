use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow invalid syntax strings in `@property` descriptors.
///
/// The `syntax` descriptor in a `@property` rule must be a quoted string
/// containing a valid CSS syntax definition. This rule checks that the value
/// is quoted, non-empty, and uses only known CSS type names.
///
/// Equivalent to Stylelint's `syntax-string-no-invalid` rule.
pub struct SyntaxStringNoInvalid;

/// Known CSS syntax component types that can appear inside `< >`.
const KNOWN_SYNTAX_TYPES: &[&str] = &[
  "angle",
  "color",
  "custom-ident",
  "flex",
  "image",
  "integer",
  "length",
  "length-percentage",
  "number",
  "percentage",
  "resolution",
  "time",
  "transform-function",
  "transform-list",
  "url",
];

/// Validate a CSS syntax string (the unquoted content between quotes).
///
/// Returns `None` if valid, or `Some(reason)` if invalid.
fn validate_syntax_string(value: &str) -> Option<String> {
  if value.is_empty() {
    return Some("Empty syntax string".to_string());
  }

  // Universal syntax
  if value == "*" {
    return None;
  }

  // Split on '|' for alternatives (pipe-separated)
  for component in value.split('|') {
    let component = component.trim();
    if component.is_empty() {
      return Some("Empty component in syntax string".to_string());
    }

    // Strip trailing multiplier (+, #)
    let component = component.trim_end_matches('+').trim_end_matches('#').trim();

    if component.is_empty() {
      return Some("Empty component in syntax string".to_string());
    }

    // Must be a data type like <length> or a keyword (custom-ident literal)
    if component.starts_with('<') {
      if !component.ends_with('>') {
        return Some(format!("Unclosed angle bracket in \"{component}\""));
      }
      let type_name = &component[1..component.len() - 1];
      if !KNOWN_SYNTAX_TYPES.contains(&type_name) {
        return Some(format!("Unknown syntax type \"<{type_name}>\""));
      }
    }
    // Otherwise it's a keyword ident, which is valid
  }

  None
}

impl Rule for SyntaxStringNoInvalid {
  fn name(&self) -> &'static str {
    "syntax-string-no-invalid"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid syntax strings in @property descriptors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "property" {
      return vec![];
    }

    let mut diags = Vec::new();

    // Look for "syntax" declarations in children
    for child in &at.children {
      let CssNode::Declaration(decl) = child else {
        continue;
      };
      if decl.property != "syntax" {
        continue;
      }

      let value = decl.value.trim();

      // Must be quoted
      if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
      {
        let inner = &value[1..value.len() - 1];
        if let Some(reason) = validate_syntax_string(inner) {
          diags.push(
            Diagnostic::new(self.name(), format!("Invalid syntax string: {reason}"))
              .severity(self.default_severity())
              .span(Span::new(decl.span.offset, decl.span.length)),
          );
        }
      } else {
        diags.push(
          Diagnostic::new(self.name(), "Syntax string must be quoted".to_string())
            .severity(self.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Declaration, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn property_rule(syntax_value: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "property".to_string(),
      params: "--my-color".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "syntax".to_string(),
        value: syntax_value.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    })
  }

  #[test]
  fn allows_valid_length() {
    assert!(
      SyntaxStringNoInvalid
        .check(&property_rule("\"<length>\""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_valid_color() {
    assert!(
      SyntaxStringNoInvalid
        .check(&property_rule("\"<color>\""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_universal() {
    assert!(
      SyntaxStringNoInvalid
        .check(&property_rule("\"*\""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_pipe_separated() {
    assert!(
      SyntaxStringNoInvalid
        .check(&property_rule("\"<length> | <color>\""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_plus_suffix() {
    assert!(
      SyntaxStringNoInvalid
        .check(&property_rule("\"<length>+\""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_hash_suffix() {
    assert!(
      SyntaxStringNoInvalid
        .check(&property_rule("\"<length>#\""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_unknown_type() {
    let d = SyntaxStringNoInvalid.check(&property_rule("\"<foo>\""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unknown syntax type"));
    assert!(d[0].message.contains("<foo>"));
  }

  #[test]
  fn reports_unquoted_value() {
    let d = SyntaxStringNoInvalid.check(&property_rule("<length>"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("must be quoted"));
  }

  #[test]
  fn reports_empty_string() {
    let d = SyntaxStringNoInvalid.check(&property_rule("\"\""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Empty syntax string"));
  }

  #[test]
  fn ignores_non_property_at_rules() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "screen".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "syntax".to_string(),
        value: "\"<bogus>\"".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    });
    assert!(SyntaxStringNoInvalid.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn allows_keyword_idents() {
    // Plain keywords like "auto" are valid in syntax descriptors
    assert!(
      SyntaxStringNoInvalid
        .check(&property_rule("\"auto\""), &ctx())
        .is_empty()
    );
  }
}
