use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow Unicode BOM.
///
/// Equivalent to `@stylistic/unicode-bom`.
pub struct StylisticUnicodeBom;

const BOM: &str = "\u{FEFF}";

impl Rule for StylisticUnicodeBom {
  fn name(&self) -> &'static str {
    "@stylistic/unicode-bom"
  }

  fn description(&self) -> &'static str {
    "Require or disallow Unicode BOM"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = ctx.primary_option_str().unwrap_or("never");
    let has_bom = ctx.source.starts_with(BOM);

    match option {
      "always" if !has_bom => {
        vec![
          Diagnostic::new(self.name(), "Expected Unicode BOM")
            .severity(self.default_severity())
            .span(Span::new(0, 0))
            .fix(Fix::new(
              "Add Unicode BOM",
              vec![Edit::new(Span::new(0, 0), BOM)],
            )),
        ]
      }
      "never" if has_bom => {
        vec![
          Diagnostic::new(self.name(), "Unexpected Unicode BOM")
            .severity(self.default_severity())
            .span(Span::new(0, BOM.len()))
            .fix(Fix::new(
              "Remove Unicode BOM",
              vec![Edit::new(Span::new(0, BOM.len()), "")],
            )),
        ]
      }
      _ => vec![],
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
  fn reports_bom_when_never() {
    let rule = StylisticUnicodeBom;
    let source = format!("\u{FEFF}a {{ color: red; }}");
    let ctx = ctx_with_source(&source);
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
  }

  #[test]
  fn allows_no_bom_when_never() {
    let rule = StylisticUnicodeBom;
    let ctx = ctx_with_source("a { color: red; }");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }
}
