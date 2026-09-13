use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Deprecated CSS properties mapped to their replacements (sorted for binary search).
/// `None` means no standard replacement exists.
static DEPRECATED_PROPERTIES: &[(&str, Option<&str>)] = &[
  ("azimuth", None),
  ("clip", Some("clip-path")),
  ("ime-mode", None),
  ("word-wrap", Some("overflow-wrap")),
];

fn find_deprecated_property(name: &str) -> Option<Option<&'static str>> {
  let lower = name.to_ascii_lowercase();
  DEPRECATED_PROPERTIES
    .binary_search_by_key(&lower.as_str(), |(k, _)| k)
    .ok()
    .map(|idx| DEPRECATED_PROPERTIES[idx].1)
}

pub struct PropertyNoDeprecated;

impl Rule for PropertyNoDeprecated {
  fn name(&self) -> &'static str {
    "property-no-deprecated"
  }

  fn description(&self) -> &'static str {
    "Disallow deprecated properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      let prop = &decl.property;
      // Skip custom properties and vendor-prefixed
      if prop.starts_with("--") || prop.starts_with('-') {
        continue;
      }
      if let Some(replacement) = find_deprecated_property(prop) {
        let message = match replacement {
          Some(repl) => format!("Expected \"{}\" to be \"{}\"", prop, repl),
          None => format!("Unexpected deprecated property \"{}\"", prop),
        };
        diags.push(
          Diagnostic::new(self.name(), message)
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_node(sel: &str, props: &[(&str, &str)]) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: props
        .iter()
        .map(|(p, v)| Declaration {
          property: p.to_string(),
          value: v.to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        })
        .collect(),
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_deprecated_clip() {
    let node = style_node("a", &[("clip", "rect(0, 0, 0, 0)")]);
    let d = PropertyNoDeprecated.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Expected \"clip\" to be \"clip-path\"");
  }

  #[test]
  fn reports_deprecated_azimuth() {
    let node = style_node("a", &[("azimuth", "center")]);
    let d = PropertyNoDeprecated.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Unexpected deprecated property \"azimuth\"");
  }

  #[test]
  fn allows_known_property() {
    let node = style_node("a", &[("color", "red")]);
    assert!(PropertyNoDeprecated.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn skips_vendor_prefixed() {
    let node = style_node("a", &[("-webkit-clip", "rect(0, 0, 0, 0)")]);
    assert!(PropertyNoDeprecated.check(&node, &ctx()).is_empty());
  }
}
