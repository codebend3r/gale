use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate `@import` rules within a stylesheet.
///
/// Equivalent to Stylelint's `no-duplicate-at-import-rules` rule.
pub struct NoDuplicateAtImportRules;

impl Rule for NoDuplicateAtImportRules {
  fn name(&self) -> &'static str {
    "no-duplicate-at-import-rules"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate @import rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    let mut seen = HashSet::new();
    let mut diagnostics = Vec::new();

    for node in nodes {
      if let CssNode::AtRule(at_rule) = node
        && at_rule.name == "import"
      {
        let url = at_rule.params.trim().to_string();
        if !seen.insert(url.clone()) {
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Unexpected duplicate @import rule \"{}\"", url),
            )
            .severity(self.default_severity())
            .span(Span::new(at_rule.span.offset, at_rule.span.length)),
          );
        }
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_duplicate_imports() {
    let rule = NoDuplicateAtImportRules;
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(0, 22),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "normalize.css".to_string(),
        span: ParserSpan::new(23, 26),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(50, 22),
        children: vec![],
      }),
    ];
    let diags = rule.check_root(&nodes, &make_context());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate @import rule \"reset.css\""
    );
  }

  #[test]
  fn ignores_unique_imports() {
    let rule = NoDuplicateAtImportRules;
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(0, 22),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "normalize.css".to_string(),
        span: ParserSpan::new(23, 26),
        children: vec![],
      }),
    ];
    let diags = rule.check_root(&nodes, &make_context());
    assert!(diags.is_empty());
  }
}
