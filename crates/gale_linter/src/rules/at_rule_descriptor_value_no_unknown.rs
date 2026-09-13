use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

pub struct AtRuleDescriptorValueNoUnknown;

/// Known values for `font-display` in `@font-face`.
static FONT_DISPLAY_VALUES: &[&str] = &["auto", "block", "fallback", "optional", "swap"];

/// Known values for `font-style` in `@font-face` (keyword part only).
static FONT_STYLE_KEYWORDS: &[&str] = &["italic", "normal", "oblique"];

/// Known keyword values for `font-stretch` in `@font-face`.
static FONT_STRETCH_KEYWORDS: &[&str] = &[
  "condensed",
  "expanded",
  "extra-condensed",
  "extra-expanded",
  "normal",
  "semi-condensed",
  "semi-expanded",
  "ultra-condensed",
  "ultra-expanded",
];

/// Known values for `inherits` in `@property`.
static PROPERTY_INHERITS_VALUES: &[&str] = &["false", "true"];

fn lookup(haystack: &[&str], needle: &str) -> bool {
  let lower = needle.to_ascii_lowercase();
  haystack.binary_search(&lower.as_str()).is_ok()
}

/// Returns `true` if the value looks like a percentage (e.g. `50%`, `100%`, `62.5%`).
fn is_percentage(value: &str) -> bool {
  let trimmed = value.trim();
  if let Some(num_part) = trimmed.strip_suffix('%') {
    num_part.parse::<f64>().is_ok()
  } else {
    false
  }
}

/// Returns `true` if the value looks like an angle (e.g. `14deg`, `-20deg`).
fn is_angle(value: &str) -> bool {
  let trimmed = value.trim();
  for suffix in &["deg", "grad", "rad", "turn"] {
    if let Some(num_part) = trimmed.strip_suffix(suffix)
      && num_part.parse::<f64>().is_ok()
    {
      return true;
    }
  }
  false
}

/// Validate a descriptor value within a specific at-rule.
///
/// Returns `Some(message)` if the value is invalid, `None` if valid or not checked.
fn validate_descriptor_value(at_rule_name: &str, descriptor: &str, value: &str) -> Option<String> {
  let at_lower = at_rule_name.to_ascii_lowercase();
  let desc_lower = descriptor.to_ascii_lowercase();
  let val_trimmed = value.trim();

  match at_lower.as_str() {
    "font-face" => match desc_lower.as_str() {
      "font-display" => {
        if !lookup(FONT_DISPLAY_VALUES, val_trimmed) {
          return Some(format!(
            "Unexpected value \"{val_trimmed}\" for descriptor \"font-display\" in @font-face"
          ));
        }
      }
      "font-style" => {
        // Accept keyword alone, or `oblique <angle>` / `oblique <angle> <angle>`
        let parts: Vec<&str> = val_trimmed.split_whitespace().collect();
        if parts.is_empty() {
          return Some(
            "Unexpected empty value for descriptor \"font-style\" in @font-face".to_string(),
          );
        }
        if !lookup(FONT_STYLE_KEYWORDS, parts[0]) {
          return Some(format!(
            "Unexpected value \"{val_trimmed}\" for descriptor \"font-style\" in @font-face"
          ));
        }
        // If "oblique", additional parts must be angle values
        if parts[0].eq_ignore_ascii_case("oblique") {
          for part in &parts[1..] {
            if !is_angle(part) {
              return Some(format!(
                "Unexpected value \"{val_trimmed}\" for descriptor \"font-style\" in @font-face"
              ));
            }
          }
        }
      }
      "font-stretch" => {
        // Accept keyword or percentage(s)
        let parts: Vec<&str> = val_trimmed.split_whitespace().collect();
        if parts.is_empty() {
          return Some(
            "Unexpected empty value for descriptor \"font-stretch\" in @font-face".to_string(),
          );
        }
        // All parts must be either keywords or percentages
        let all_valid = parts
          .iter()
          .all(|p| lookup(FONT_STRETCH_KEYWORDS, p) || is_percentage(p));
        if !all_valid {
          return Some(format!(
            "Unexpected value \"{val_trimmed}\" for descriptor \"font-stretch\" in @font-face"
          ));
        }
      }
      _ => {}
    },
    "property" => {
      if desc_lower == "inherits" && !lookup(PROPERTY_INHERITS_VALUES, val_trimmed) {
        return Some(format!(
          "Unexpected value \"{val_trimmed}\" for descriptor \"inherits\" in @property"
        ));
      }
    }
    _ => {}
  }

  None
}

impl Rule for AtRuleDescriptorValueNoUnknown {
  fn name(&self) -> &'static str {
    "at-rule-descriptor-value-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown values for known descriptors in at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    // Only validate at-rules we have knowledge about
    let at_lower = at.name.to_ascii_lowercase();
    if !matches!(at_lower.as_str(), "font-face" | "property") {
      return vec![];
    }

    let mut diags = Vec::new();
    for child in &at.children {
      if let CssNode::Declaration(decl) = child
        && let Some(msg) = validate_descriptor_value(&at.name, &decl.property, &decl.value)
      {
        diags.push(
          Diagnostic::new(self.name(), msg)
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
  fn reports_invalid_font_display() {
    let node = font_face_node(&[("font-display", "fast")]);
    let d = AtRuleDescriptorValueNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fast"));
    assert!(d[0].message.contains("font-display"));
  }

  #[test]
  fn allows_valid_font_display() {
    for val in &["auto", "block", "swap", "fallback", "optional"] {
      let node = font_face_node(&[("font-display", val)]);
      assert!(
        AtRuleDescriptorValueNoUnknown
          .check(&node, &ctx())
          .is_empty(),
        "Expected \"{}\" to be valid for font-display",
        val
      );
    }
  }

  #[test]
  fn reports_invalid_property_inherits() {
    let node = property_node(&[("inherits", "yes")]);
    let d = AtRuleDescriptorValueNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("yes"));
    assert!(d[0].message.contains("inherits"));
  }

  #[test]
  fn allows_valid_property_inherits() {
    let node = property_node(&[("inherits", "true")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
    let node = property_node(&[("inherits", "false")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_valid_font_style() {
    let node = font_face_node(&[("font-style", "normal")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
    let node = font_face_node(&[("font-style", "italic")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
    let node = font_face_node(&[("font-style", "oblique 14deg")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_invalid_font_style() {
    let node = font_face_node(&[("font-style", "bold")]);
    let d = AtRuleDescriptorValueNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bold"));
  }

  #[test]
  fn allows_valid_font_stretch_keywords_and_percentages() {
    let node = font_face_node(&[("font-stretch", "condensed")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
    let node = font_face_node(&[("font-stretch", "75%")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
    let node = font_face_node(&[("font-stretch", "75% 100%")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_invalid_font_stretch() {
    let node = font_face_node(&[("font-stretch", "wide")]);
    let d = AtRuleDescriptorValueNoUnknown.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("wide"));
  }

  #[test]
  fn ignores_unchecked_descriptors() {
    // `src` is a known descriptor but we don't validate its value
    let node = font_face_node(&[("src", "url(anything-goes.woff2)")]);
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_other_at_rules() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    assert!(
      AtRuleDescriptorValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
  }
}
