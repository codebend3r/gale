use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Prefer string notation for `@import` (i.e. quoted strings, not `url()`).
///
/// Equivalent to Stylelint's `import-notation` rule with "string" option.
/// Detection-only (no autofix).
pub struct ImportNotation;

impl Rule for ImportNotation {
  fn name(&self) -> &'static str {
    "import-notation"
  }

  fn description(&self) -> &'static str {
    "Prefer string notation for @import"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };
    if at_rule.name != "import" {
      return vec![];
    }

    // Check the source text around the @import span for url() usage.
    let span_start = at_rule.span.offset;
    let span_end = span_start + at_rule.span.length;
    let search_area = if span_end <= ctx.source.len() && span_start < span_end {
      &ctx.source[span_start..span_end]
    } else {
      &at_rule.params
    };

    let lower = search_area.to_ascii_lowercase();
    if lower.contains("url(") {
      return vec![
        Diagnostic::new(
          self.name(),
          "Expected string notation for @import instead of url()",
        )
        .severity(self.default_severity())
        .span(Span::new(span_start, at_rule.span.length)),
      ];
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_url_notation() {
    let src = "@import url(\"foo.css\");";
    let node = CssNode::AtRule(AtRule {
      name: "import".to_string(),
      params: "foo.css".to_string(),
      span: ParserSpan::new(0, src.len()),
      children: vec![],
    });
    let d = ImportNotation.check(&node, &ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("string notation"));
  }

  #[test]
  fn allows_string_notation() {
    // When lightningcss parses `@import "foo.css";`, the params is just
    // the URL string without url() wrapper. The source text also doesn't
    // contain url().
    let src = "@import \"foo.css\";";
    let node = CssNode::AtRule(AtRule {
      name: "import".to_string(),
      params: "foo.css".to_string(),
      span: ParserSpan::new(0, src.len()),
      children: vec![],
    });
    let d = ImportNotation.check(&node, &ctx_with_source(src));
    assert!(d.is_empty());
  }
}
