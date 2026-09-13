use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow file extensions in `@use`, `@forward`, and `@import` paths.
pub struct ScssLoadPartialExtension;

impl Rule for ScssLoadPartialExtension {
  fn name(&self) -> &'static str {
    "scss/load-partial-extension"
  }

  fn description(&self) -> &'static str {
    "Disallow unnecessary file extensions in @use/@import"
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

    if !matches!(at.name.as_str(), "use" | "forward" | "import") {
      return vec![];
    }

    let path = extract_path(&at.params);
    if path.is_empty() {
      return vec![];
    }

    for ext in &[".scss", ".sass"] {
      if path.ends_with(ext) {
        return vec![
          Diagnostic::new(
            self.name(),
            format!("Unexpected file extension \"{}\" in partial import", ext),
          )
          .severity(self.default_severity())
          .span(Span::new(at.span.offset, at.span.length)),
        ];
      }
    }

    vec![]
  }
}

/// Extract a path string from at-rule params, stripping quotes.
fn extract_path(params: &str) -> &str {
  let trimmed = params.trim();
  let trimmed = trimmed
    .strip_prefix('"')
    .and_then(|s| s.strip_suffix('"'))
    .or_else(|| {
      trimmed
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
    })
    .unwrap_or(trimmed);
  trimmed.split_whitespace().next().unwrap_or(trimmed)
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

  fn use_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "use".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_scss_extension() {
    let d = ScssLoadPartialExtension.check(&use_rule("\"variables.scss\""), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".scss"));
  }

  #[test]
  fn reports_sass_extension() {
    let d = ScssLoadPartialExtension.check(&use_rule("\"variables.sass\""), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_css_extension() {
    let d = ScssLoadPartialExtension.check(&use_rule("\"reset.css\""), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_extension() {
    let d = ScssLoadPartialExtension.check(&use_rule("\"variables\""), &scss_ctx());
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
      ScssLoadPartialExtension
        .check(&use_rule("\"variables.scss\""), &ctx)
        .is_empty()
    );
  }
}
