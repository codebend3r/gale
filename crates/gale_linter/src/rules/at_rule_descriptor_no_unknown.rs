use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "at-rule-descriptor-no-unknown";

declare_rule! {
  pub struct AtRuleDescriptorNoUnknown => AtRuleDescriptorNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown descriptors within at-rules",
    default_severity: Error,
  }
}

/// `at-rule-descriptor-no-unknown`, which takes no options.
pub struct AtRuleDescriptorNoUnknownCheck;

impl AtRuleDescriptorNoUnknownCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

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

/// The descriptor table for an at-rule, or `None` if it takes no descriptors.
fn known_descriptors_for_at_rule(name: &str) -> Option<&'static [&'static str]> {
  match name.to_ascii_lowercase().as_str() {
    "font-face" => Some(FONT_FACE_DESCRIPTORS),
    "counter-style" => Some(COUNTER_STYLE_DESCRIPTORS),
    "property" => Some(PROPERTY_DESCRIPTORS),
    _ => None,
  }
}

/// Case-insensitive binary search over a sorted descriptor table.
fn is_known_descriptor(descriptors: &[&str], name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  descriptors.binary_search(&lower.as_str()).is_ok()
}

impl Check for AtRuleDescriptorNoUnknownCheck {
  /// Flags declarations inside a descriptor-taking at-rule whose property is not
  /// a known descriptor. Vendor-prefixed names are skipped.
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
              NAME,
              format!(
                "Unexpected unknown descriptor \"{}\" in @{}",
                decl.property, at.name
              ),
            )
            .span(decl.span),
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
  use gale_css_parser::{AtRule, Declaration};
  use gale_diagnostics::Span;
  use serde_json::json;

  use crate::testing::{configured, ctx, invalid_options};

  fn font_face_node(descriptors: &[(&str, &str)]) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "font-face".to_string(),
      params: String::new(),
      span: Span::new(0, 0),
      children: descriptors
        .iter()
        .map(|(p, v)| {
          CssNode::Declaration(Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: Span::new(0, 0),
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
      span: Span::new(0, 0),
      children: descriptors
        .iter()
        .map(|(p, v)| {
          CssNode::Declaration(Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: Span::new(0, 0),
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
      span: Span::new(0, 0),
      children: descriptors
        .iter()
        .map(|(p, v)| {
          CssNode::Declaration(Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: Span::new(0, 0),
            important: false,
          })
        })
        .collect(),
    })
  }

  #[test]
  fn reports_unknown_font_face_descriptor() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = font_face_node(&[("font-family", "MyFont"), ("unknown-thing", "value")]);
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("unknown-thing"));
    assert!(d[0].message.contains("@font-face"));
  }

  #[test]
  fn allows_known_font_face_descriptors() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = font_face_node(&[
      ("font-family", "MyFont"),
      ("src", "url(font.woff2)"),
      ("font-weight", "400"),
      ("font-display", "swap"),
    ]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn reports_unknown_counter_style_descriptor() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = counter_style_node(&[("system", "cyclic"), ("bad-desc", "x")]);
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bad-desc"));
  }

  #[test]
  fn allows_known_counter_style_descriptors() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = counter_style_node(&[("system", "cyclic"), ("symbols", "A B C")]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn reports_unknown_property_descriptor() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = property_node(&[("syntax", "\"<color>\""), ("bad", "value")]);
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bad"));
  }

  #[test]
  fn allows_known_property_descriptors() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = property_node(&[
      ("syntax", "\"<color>\""),
      ("inherits", "false"),
      ("initial-value", "red"),
    ]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn ignores_other_at_rules() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: Span::new(0, 0),
      children: vec![],
    });
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn skips_vendor_prefixed_descriptors() {
    let rule = configured(&AtRuleDescriptorNoUnknown, None);
    let node = font_face_node(&[
      ("font-family", "MyFont"),
      ("-webkit-font-smoothing", "auto"),
    ]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, json!("always")),
      ["Unexpected option value \"always\" for rule \"at-rule-descriptor-no-unknown\""]
    );
  }
}
