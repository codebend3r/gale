use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specific selectors.
///
/// By default, disallows no selectors. Configure via rule options.
///
/// Equivalent to Stylelint's `selector-disallowed-list` rule.
pub struct SelectorDisallowedList;

const DISALLOWED: &[&str] = &[];

impl Rule for SelectorDisallowedList {
  fn name(&self) -> &'static str {
    "selector-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let selector = rule.selector.trim();
    for disallowed in DISALLOWED {
      if selector.contains(disallowed) {
        return vec![
          Diagnostic::new(self.name(), format!("Unexpected selector \"{selector}\""))
            .severity(self.default_severity())
            .span(Span::new(rule.span.offset, rule.span.length)),
        ];
      }
    }
    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Span as ParserSpan, StyleRule, Syntax};

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
      declarations: vec![],
      span: ParserSpan::new(0, sel.len()),
      ..Default::default()
    })
  }

  #[test]
  fn allows_all_when_list_empty() {
    let d = SelectorDisallowedList.check(&style_with_selector(".foo"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_any_selector_by_default() {
    let d = SelectorDisallowedList.check(&style_with_selector("#id > .class"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(SelectorDisallowedList.name(), "selector-disallowed-list");
  }
}
