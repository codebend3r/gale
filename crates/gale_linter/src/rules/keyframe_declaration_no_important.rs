use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports `!important` within `@keyframes` declarations.
///
/// Using `!important` inside keyframe declarations is ignored by browsers
/// and is almost certainly a mistake.
///
/// Equivalent to Stylelint's `keyframe-declaration-no-important` rule.
pub struct KeyframeDeclarationNoImportant;

impl Rule for KeyframeDeclarationNoImportant {
  fn name(&self) -> &'static str {
    "keyframe-declaration-no-important"
  }

  fn description(&self) -> &'static str {
    "Disallow !important within keyframe declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let CssNode::AtRule(at_rule) = node
      && at_rule.name == "keyframes"
    {
      collect_important_in_children(&at_rule.children, self, &mut diagnostics);
    }

    diagnostics
  }
}

fn collect_important_in_children(
  children: &[CssNode],
  rule: &KeyframeDeclarationNoImportant,
  diagnostics: &mut Vec<Diagnostic>,
) {
  for child in children {
    match child {
      CssNode::Declaration(decl) if decl.important => {
        diagnostics.push(
          Diagnostic::new(
            rule.name(),
            format!(
              "Unexpected !important in keyframe declaration \"{}\"",
              decl.property
            ),
          )
          .severity(rule.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
      CssNode::Style(style_rule) => {
        for decl in &style_rule.declarations {
          if decl.important {
            diagnostics.push(
              Diagnostic::new(
                rule.name(),
                format!(
                  "Unexpected !important in keyframe declaration \"{}\"",
                  decl.property
                ),
              )
              .severity(rule.default_severity())
              .span(Span::new(decl.span.offset, decl.span.length)),
            );
          }
        }
      }
      _ => {}
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_important_in_keyframe() {
    let rule = KeyframeDeclarationNoImportant;
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, 50),
      children: vec![CssNode::Declaration(Declaration {
        property: "opacity".to_string(),
        value: "0".to_string(),
        span: ParserSpan::new(20, 15),
        important: true,
      })],
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("!important"));
  }

  #[test]
  fn ignores_keyframe_without_important() {
    let rule = KeyframeDeclarationNoImportant;
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, 50),
      children: vec![CssNode::Declaration(Declaration {
        property: "opacity".to_string(),
        value: "0".to_string(),
        span: ParserSpan::new(20, 15),
        important: false,
      })],
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_non_keyframe_at_rule() {
    let rule = KeyframeDeclarationNoImportant;
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 30),
      children: vec![],
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn detects_important_in_parsed_css_keyframes() {
    let css = "@keyframes fade { from { opacity: 0 !important; } }";
    let result = gale_css_parser::parse(css, Syntax::Css).expect("should parse CSS");
    let rule = KeyframeDeclarationNoImportant;
    let ctx = make_context();
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect !important in CSS keyframe"
    );
  }

  #[test]
  fn detects_important_in_parsed_scss_keyframes() {
    let scss = "@keyframes fade { from { opacity: 0 !important; } }";
    let result = gale_css_parser::parse(scss, Syntax::Scss).expect("should parse SCSS");
    let rule = KeyframeDeclarationNoImportant;
    let ctx = RuleContext {
      file_path: "test.scss",
      source: scss,
      syntax: Syntax::Scss,
      options: None,
    };
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect !important in SCSS keyframe"
    );
  }
}
