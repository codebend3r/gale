use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_pseudo_element;
use crate::rule::{Rule, RuleContext};

pub struct SelectorPseudoElementNoUnknown;

impl Rule for SelectorPseudoElementNoUnknown {
  fn name(&self) -> &'static str {
    "selector-pseudo-element-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown pseudo-element selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();
    for name in extract_pseudo_elements(&rule.selector) {
      if name.starts_with('-') {
        continue;
      }
      if !is_known_pseudo_element(&name) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown pseudo-element selector \"::{name}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Extract pseudo-element names from a selector string (::name patterns).
fn extract_pseudo_elements(selector: &str) -> Vec<String> {
  let mut elements = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
      i += 2; // skip ::
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        elements.push(name);
      }
    } else {
      i += 1;
    }
  }

  elements
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{CssNode, Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_unknown_pseudo_element() {
    let d = SelectorPseudoElementNoUnknown.check(&style_with_selector("a::beforre"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("::beforre"));
  }

  #[test]
  fn allows_known_pseudo_element() {
    assert!(
      SelectorPseudoElementNoUnknown
        .check(&style_with_selector("a::before"), &ctx())
        .is_empty()
    );
    assert!(
      SelectorPseudoElementNoUnknown
        .check(&style_with_selector("a::placeholder"), &ctx())
        .is_empty()
    );
  }
}
