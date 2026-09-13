use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

pub struct AtRuleDescriptorNoUnknown;

/// Known descriptors for `@font-face` (sorted).
static FONT_FACE_DESCRIPTORS: &[&str] = &[
  "ascent-override",
  "descent-override",
  "font-display",
  "font-family",
  "font-feature-settings",
  "font-stretch",
  "font-style",
  "font-variation-settings",
  "font-weight",
  "line-gap-override",
  "size-adjust",
  "src",
  "unicode-range",
];

/// Known descriptors for `@counter-style` (sorted).
static COUNTER_STYLE_DESCRIPTORS: &[&str] = &[
  "additive-symbols",
  "fallback",
  "negative",
  "pad",
  "prefix",
  "range",
  "speak-as",
  "suffix",
  "symbols",
  "system",
];

/// Known descriptors for `@property` (sorted).
static PROPERTY_DESCRIPTORS: &[&str] = &["inherits", "initial-value", "syntax"];

fn known_descriptors_for_at_rule(name: &str) -> Option<&'static [&'static str]> {
  match name.to_ascii_lowercase().as_str() {
    "font-face" => Some(FONT_FACE_DESCRIPTORS),
    "counter-style" => Some(COUNTER_STYLE_DESCRIPTORS),
    "property" => Some(PROPERTY_DESCRIPTORS),
    _ => None,
  }
}

fn is_known_descriptor(descriptors: &[&str], name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  descriptors.binary_search(&lower.as_str()).is_ok()
}

impl Rule for AtRuleDescriptorNoUnknown {
  fn name(&self) -> &'static str {
    "at-rule-descriptor-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown descriptors within at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    let Some(known) = known_descriptors_for_at_rule(&at.name) else {
      return vec![];
    };

    let mut diags = Vec::new();
    for child in &at.children {
      if let CssNode::Declaration(decl) = child {
        // Skip vendor-prefixed descriptors
        if decl.property.starts_with('-') {
          continue;
        }
        if !is_known_descriptor(known, &decl.property) {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Unexpected unknown descriptor \"{}\" in @{}",
                decl.property, at.name
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
          );
        }
      }
    }
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

  fn font_face_node(descriptors: &[(&str, &str)]) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "font-face".to_string(),
      params: String::new(),
      span: ParserSpan::new(0, 0),
      children: descriptors
        .iter()
        .map(|(p, v)| {
          CssNode::Declaration(Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: ParserSpan::new(0, 0),
            important: false,
          })
        })
        .collect(),
    })
  }

  fn counter_style_node(descriptors: &[(&str, &str)]) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "counter-style".to_string(),
      params: "my-counter".to_string(),
      span: ParserSpan::new(0, 0),
      children: descriptors
        .iter()
        .map(|(p, v)| {
          CssNode::Declaration(Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: ParserSpan::new(0, 0),
            important: false,
          })
        })
        .collect(),
    })
  }

  fn property_node(descriptors: &[(&str, &str)]) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "property".to_string(),
      params: "--my-prop".to_string(),
      span: ParserSpan::new(0, 0),
      children: descriptors
        .iter()
        .map(|(p, v)| {
          CssNode::Declaration(Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: ParserSpan::new(0, 0),
            important: false,
          })
        })
        .collect(),
    })
  }

  #[test]
  fn reports_unknown_font_face_descriptor() {
    let node = font_face_node(&[("font-family", "MyFont"), ("unknown-thing", "value")]);
    let d = AtRuleDescriptorNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("unknown-thing"));
    assert!(d[0].message.contains("@font-face"));
  }

  #[test]
  fn allows_known_font_face_descriptors() {
    let node = font_face_node(&[
      ("font-family", "MyFont"),
      ("src", "url(font.woff2)"),
      ("font-weight", "400"),
      ("font-display", "swap"),
    ]);
    assert!(AtRuleDescriptorNoUnknown.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn reports_unknown_counter_style_descriptor() {
    let node = counter_style_node(&[("system", "cyclic"), ("bad-desc", "x")]);
    let d = AtRuleDescriptorNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bad-desc"));
  }

  #[test]
  fn allows_known_counter_style_descriptors() {
    let node = counter_style_node(&[("system", "cyclic"), ("symbols", "A B C")]);
    assert!(AtRuleDescriptorNoUnknown.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn reports_unknown_property_descriptor() {
    let node = property_node(&[("syntax", "\"<color>\""), ("bad", "value")]);
    let d = AtRuleDescriptorNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bad"));
  }

  #[test]
  fn allows_known_property_descriptors() {
    let node = property_node(&[
      ("syntax", "\"<color>\""),
      ("inherits", "false"),
      ("initial-value", "red"),
    ]);
    assert!(AtRuleDescriptorNoUnknown.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn ignores_other_at_rules() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    assert!(AtRuleDescriptorNoUnknown.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn skips_vendor_prefixed_descriptors() {
    let node = font_face_node(&[
      ("font-family", "MyFont"),
      ("-webkit-font-smoothing", "auto"),
    ]);
    assert!(AtRuleDescriptorNoUnknown.check(&node, &ctx()).is_empty());
  }
}
