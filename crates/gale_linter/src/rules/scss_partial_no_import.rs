use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow `@import` in partial files (files starting with `_`).
///
/// In SCSS, partial files (prefixed with `_`) should use `@use` or `@forward`
/// instead of `@import`.
///
/// Equivalent to `scss/partial-no-import`.
pub struct ScssPartialNoImport;

impl Rule for ScssPartialNoImport {
  fn name(&self) -> &'static str {
    "scss/partial-no-import"
  }

  fn description(&self) -> &'static str {
    "Disallow @import in partial SCSS files"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    // Only applies to partial files (filename starts with `_`)
    let filename = ctx
      .file_path
      .rsplit('/')
      .next()
      .or_else(|| ctx.file_path.rsplit('\\').next())
      .unwrap_or(ctx.file_path);

    if !filename.starts_with('_') {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "import" {
      return vec![];
    }

    vec![
      Diagnostic::new(
        self.name(),
        "Unexpected @import in partial SCSS file. Use @use or @forward instead.".to_string(),
      )
      .severity(self.default_severity())
      .span(Span::new(at.span.offset, at.span.length)),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn partial_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "_partial.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn non_partial_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "main.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn import(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "import".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    })
  }

  #[test]
  fn reports_import_in_partial() {
    let d = ScssPartialNoImport.check(&import("\"foo\""), &partial_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@import"));
  }

  #[test]
  fn allows_import_in_non_partial() {
    assert!(
      ScssPartialNoImport
        .check(&import("\"foo\""), &non_partial_ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_non_import_at_rules() {
    let node = CssNode::AtRule(AtRule {
      name: "use".to_string(),
      params: "\"foo\"".to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    });
    assert!(ScssPartialNoImport.check(&node, &partial_ctx()).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let css_ctx = RuleContext {
      file_path: "_partial.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssPartialNoImport
        .check(&import("\"foo\""), &css_ctx)
        .is_empty()
    );
  }
}
