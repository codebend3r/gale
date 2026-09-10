use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow `@if` conditions that compare to `null`.
///
/// Users should use `not $variable` instead of `$variable != null`,
/// and `$variable` instead of `$variable == null`.
pub struct ScssAtIfNoNull;

impl Rule for ScssAtIfNoNull {
  fn name(&self) -> &'static str {
    "scss/at-if-no-null"
  }

  fn description(&self) -> &'static str {
    "Disallow null comparisons in @if conditions"
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

    if at.name != "if" {
      return vec![];
    }

    let params = at.params.to_ascii_lowercase();

    // Stylelint's scss/at-if-no-null skips `!= null and ...` patterns —
    // these are compound conditions where the null check is part of a
    // larger boolean expression and removing it would change semantics.
    if params.contains("!= null and ") {
      return vec![];
    }

    if params.contains("== null") || params.contains("!= null") {
      vec![
        Diagnostic::new(self.name(), "Unexpected null comparison in @if condition")
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
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn if_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "if".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_equals_null() {
    let d = ScssAtIfNoNull.check(&if_rule("$var == null"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_not_equals_null() {
    let d = ScssAtIfNoNull.check(&if_rule("$var != null"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_non_null_comparison() {
    let d = ScssAtIfNoNull.check(&if_rule("$var"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_not_variable() {
    let d = ScssAtIfNoNull.check(&if_rule("not $var"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_not_equals_null_and_compound() {
    // `!= null and <expr>` is a compound condition — Stylelint skips these.
    let d = ScssAtIfNoNull.check(&if_rule("$var != null and $var != ''"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssAtIfNoNull
        .check(&if_rule("$var == null"), &ctx)
        .is_empty()
    );
  }
}
