use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports declarations that appear directly inside at-rules that don't
/// support them (like `@media`, `@supports`).
///
/// A declaration inside `@media` without a selector wrapper is invalid CSS.
///
/// Equivalent to Stylelint's `no-invalid-position-declaration` rule.
pub struct NoInvalidPositionDeclaration;

/// At-rules that act as conditional wrappers and should not contain
/// declarations directly (only nested rules).
const WRAPPER_AT_RULES: &[&str] = &[
  "container",
  "document",
  "layer",
  "media",
  "scope",
  "starting-style",
  "supports",
];

fn check_style_rule(
  rule: &NoInvalidPositionDeclaration,
  sr: &gale_css_parser::StyleRule,
  diags: &mut Vec<Diagnostic>,
) {
  check_nodes(rule, &sr.nested_at_rules, true, diags);
  for child in &sr.children {
    check_style_rule(rule, child, diags);
  }
}

/// Check for invalid declarations inside wrapper at-rules.
///
/// `inside_style_rule` is true when the at-rule is reached from inside a
/// style rule's `nested_at_rules` (SCSS nesting).  In that case wrapper
/// at-rules like `@media` legitimately contain declarations scoped by the
/// parent selector, so we must not flag them.
fn check_nodes(
  rule: &NoInvalidPositionDeclaration,
  nodes: &[CssNode],
  inside_style_rule: bool,
  diags: &mut Vec<Diagnostic>,
) {
  for node in nodes {
    match node {
      CssNode::AtRule(at) => {
        let is_wrapper = WRAPPER_AT_RULES.contains(&at.name.as_str());

        // Only flag declarations in wrapper at-rules that are NOT
        // nested inside a style rule (SCSS nesting makes them valid).
        if is_wrapper && !inside_style_rule {
          for child in &at.children {
            if let CssNode::Declaration(decl) = child {
              if decl.property.starts_with('$') {
                continue;
              }
              // Use the declaration's own span so that inline
              // disable-next-line comments (which disable the
              // declaration's line, not the @-rule's line) work
              // correctly.
              diags.push(
                Diagnostic::new(
                  rule.name(),
                  format!(
                    "Unexpected declaration \"{}\" directly inside @{}",
                    decl.property, at.name
                  ),
                )
                .severity(rule.default_severity())
                .span(Span::new(decl.span.offset, decl.span.length)),
              );
            }
          }
        }

        // Recurse into the at-rule's children (preserving inside_style_rule).
        check_nodes(rule, &at.children, inside_style_rule, diags);
      }
      CssNode::Style(sr) => {
        // Inside a style rule, nested at-rules are valid scoping.
        check_nodes(rule, &sr.nested_at_rules, true, diags);
        // Recurse into child style rules (which are StyleRule, not CssNode).
        for child in &sr.children {
          check_style_rule(rule, child, diags);
        }
      }
      _ => {}
    }
  }
}

impl Rule for NoInvalidPositionDeclaration {
  fn name(&self) -> &'static str {
    "no-invalid-position-declaration"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid position declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    check_nodes(self, nodes, false, &mut diags);
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_declaration_inside_media() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color"));
    assert!(d[0].message.contains("@media"));
  }

  #[test]
  fn allows_style_rules_inside_media() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Style(gale_css_parser::StyleRule {
        selector: "a".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        }],
        span: ParserSpan::new(0, 0),
        ..Default::default()
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_scss_variable_inside_media() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "$sidebar-width".to_string(),
        value: "285px".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(d.is_empty(), "SCSS variables should not be flagged");
  }

  #[test]
  fn allows_declarations_inside_font_face() {
    let node = CssNode::AtRule(AtRule {
      name: "font-face".to_string(),
      params: String::new(),
      span: ParserSpan::new(0, 0),
      children: vec![CssNode::Declaration(Declaration {
        property: "font-family".to_string(),
        value: "MyFont".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      })],
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_scss_nested_media_with_declarations() {
    // In SCSS, @media nested inside a style rule legitimately contains
    // declarations scoped by the parent selector.  This should NOT be flagged.
    let node = CssNode::Style(gale_css_parser::StyleRule {
      selector: ".foo".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, 100),
      nested_at_rules: vec![CssNode::AtRule(AtRule {
        name: "media".to_string(),
        params: "(min-width: 600px)".to_string(),
        span: ParserSpan::new(10, 80),
        children: vec![CssNode::Declaration(Declaration {
          property: "font-size".to_string(),
          value: "14px".to_string(),
          span: ParserSpan::new(40, 16),
          important: false,
        })],
      })],
      ..Default::default()
    });
    let d = NoInvalidPositionDeclaration.check_root(&[node], &ctx());
    assert!(
      d.is_empty(),
      "declarations inside @media nested in a style rule should not be flagged"
    );
  }
}
