use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow `@extend` without a `%placeholder` selector.
pub struct ScssAtExtendNoMissingPlaceholder;

impl Rule for ScssAtExtendNoMissingPlaceholder {
  fn name(&self) -> &'static str {
    "scss/at-extend-no-missing-placeholder"
  }

  fn description(&self) -> &'static str {
    "Disallow @extend without a %placeholder selector"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "extend" {
      return vec![];
    }

    let params = at.params.trim();
    if !params.starts_with('%') {
      // Point to the selector, not the `@extend` keyword.
      let at_src_end = (at.span.offset + at.span.length).min(ctx.source.len());
      let at_src = &ctx.source[at.span.offset..at_src_end];
      let sel_off = at_src.find(params).unwrap_or(0);
      vec![
        Diagnostic::new(
          self.name(),
          "Expected a placeholder selector (e.g. %placeholder) to be used in @extend".to_string(),
        )
        .severity(self.default_severity())
        .span(Span::new(at.span.offset + sel_off, params.len())),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn extend(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "extend".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_non_placeholder() {
    let d = ScssAtExtendNoMissingPlaceholder.check(&extend(".foo"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("placeholder"));
  }

  #[test]
  fn allows_placeholder() {
    let d = ScssAtExtendNoMissingPlaceholder.check(&extend("%placeholder"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_extend() {
    let node = CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "foo".to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    });
    assert!(
      ScssAtExtendNoMissingPlaceholder
        .check(&node, &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_non_scss() {
    let css_ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssAtExtendNoMissingPlaceholder
        .check(&extend(".foo"), &css_ctx)
        .is_empty()
    );
  }
}
