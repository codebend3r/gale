use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow missing end-of-source newlines.
///
/// Equivalent to `@stylistic/no-missing-end-of-source-newline`.
pub struct StylisticNoMissingEndOfSourceNewline;

impl Rule for StylisticNoMissingEndOfSourceNewline {
  fn name(&self) -> &'static str {
    "@stylistic/no-missing-end-of-source-newline"
  }

  fn description(&self) -> &'static str {
    "Disallow missing end-of-source newlines"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if ctx.source.is_empty() {
      return vec![];
    }

    // Stylelint checks whether the raw source ends with '\n'.
    // It does NOT trim trailing whitespace — a file ending with "\n  "
    // is considered to be missing an end-of-source newline.
    if !ctx.source.ends_with('\n') {
      let offset = ctx.source.len().saturating_sub(1);
      vec![
        Diagnostic::new(self.name(), "Unexpected missing end-of-source newline")
          .severity(self.default_severity())
          .span(Span::new(offset, 0))
          .fix(Fix::new(
            "Add newline at end of source",
            vec![Edit::new(Span::new(ctx.source.len(), 0), "\n")],
          )),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_missing_newline() {
    let rule = StylisticNoMissingEndOfSourceNewline;
    let ctx = ctx_with_source("a { color: red; }");
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("missing end-of-source"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_trailing_newline() {
    let rule = StylisticNoMissingEndOfSourceNewline;
    let ctx = ctx_with_source("a { color: red; }\n");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_empty_source() {
    let rule = StylisticNoMissingEndOfSourceNewline;
    let ctx = ctx_with_source("");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }
}
