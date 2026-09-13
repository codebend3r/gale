use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow unquoted strings inside `unquote()`.
pub struct ScssFunctionUnquoteNoUnquotedStringsInside;

impl Rule for ScssFunctionUnquoteNoUnquotedStringsInside {
  fn name(&self) -> &'static str {
    "scss/function-unquote-no-unquoted-strings-inside"
  }

  fn description(&self) -> &'static str {
    "Disallow unquoted strings inside unquote()"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Declaration(decl) = node else {
      return vec![];
    };

    let value = &decl.value;
    // Simple pattern: look for `unquote(` followed by something that is NOT a quote
    if let Some(idx) = value.find("unquote(") {
      let after = &value[idx + 8..];
      let trimmed = after.trim_start();
      if !trimmed.is_empty()
        && !trimmed.starts_with('"')
        && !trimmed.starts_with('\'')
        && !trimmed.starts_with('$')
        && !trimmed.starts_with(')')
      {
        return vec![
          Diagnostic::new(self.name(), "Unexpected unquoted string inside unquote()")
            .severity(self.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
        ];
      }
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn decl(value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "font-family".to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn reports_unquoted_string_inside_unquote() {
    let d = ScssFunctionUnquoteNoUnquotedStringsInside.check(&decl("unquote(bold)"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_quoted_inside_unquote() {
    let d =
      ScssFunctionUnquoteNoUnquotedStringsInside.check(&decl("unquote(\"bold\")"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_variable_inside_unquote() {
    let d = ScssFunctionUnquoteNoUnquotedStringsInside.check(&decl("unquote($var)"), &scss_ctx());
    assert!(d.is_empty());
  }
}
