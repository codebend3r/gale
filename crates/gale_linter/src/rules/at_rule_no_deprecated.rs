use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Deprecated CSS at-rules (sorted for binary search).
static DEPRECATED_AT_RULES: &[&str] = &["charset", "document"];

fn is_deprecated_at_rule(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  DEPRECATED_AT_RULES.binary_search(&lower.as_str()).is_ok()
}

pub struct AtRuleNoDeprecated;

impl Rule for AtRuleNoDeprecated {
  fn name(&self) -> &'static str {
    "at-rule-no-deprecated"
  }

  fn description(&self) -> &'static str {
    "Disallow deprecated at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if is_deprecated_at_rule(&at.name) {
      vec![
        Diagnostic::new(
          self.name(),
          format!("Unexpected deprecated at-rule \"@{}\"", at.name),
        )
        .severity(self.default_severity())
        .span(Span::new(at.span.offset, at.span.length)),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Span, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn at(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_charset() {
    let d = AtRuleNoDeprecated.check(&at("charset"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@charset"));
  }

  #[test]
  fn reports_document() {
    let d = AtRuleNoDeprecated.check(&at("document"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@document"));
  }

  #[test]
  fn allows_media() {
    assert!(AtRuleNoDeprecated.check(&at("media"), &ctx()).is_empty());
  }

  #[test]
  fn allows_keyframes() {
    assert!(
      AtRuleNoDeprecated
        .check(&at("keyframes"), &ctx())
        .is_empty()
    );
  }
}
