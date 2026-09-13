use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow empty first lines.
///
/// Equivalent to `@stylistic/no-empty-first-line`.
pub struct StylisticNoEmptyFirstLine;

impl Rule for StylisticNoEmptyFirstLine {
  fn name(&self) -> &'static str {
    "@stylistic/no-empty-first-line"
  }

  fn description(&self) -> &'static str {
    "Disallow empty first lines"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if ctx.source.is_empty() {
      return vec![];
    }

    // Check if the source starts with an empty first line.
    // An empty first line means the source starts with \r\n or \n.
    let starts_empty = ctx.source.starts_with('\n') || ctx.source.starts_with("\r\n");

    if starts_empty {
      // Find the length of the leading empty whitespace to remove
      let mut end = 0;
      let bytes = ctx.source.as_bytes();
      while end < bytes.len()
        && (bytes[end] == b'\n' || bytes[end] == b'\r' || bytes[end] == b' ' || bytes[end] == b'\t')
      {
        if bytes[end] == b'\n' {
          end += 1;
          break;
        }
        if bytes[end] == b'\r' && end + 1 < bytes.len() && bytes[end + 1] == b'\n' {
          end += 2;
          break;
        }
        end += 1;
      }

      vec![
        Diagnostic::new(self.name(), "Unexpected empty first line")
          .severity(self.default_severity())
          .span(Span::new(0, 1))
          .fix(Fix::new(
            "Remove empty first line",
            vec![Edit::new(Span::new(0, end), "")],
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
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_empty_first_line_lf() {
    let rule = StylisticNoEmptyFirstLine;
    let ctx = ctx_with_source("\na { color: red; }\n");
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty first line"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn reports_empty_first_line_crlf() {
    let rule = StylisticNoEmptyFirstLine;
    let ctx = ctx_with_source("\r\na { color: red; }\n");
    let d = rule.check_root(&[], &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty first line"));
  }

  #[test]
  fn allows_non_empty_first_line() {
    let rule = StylisticNoEmptyFirstLine;
    let ctx = ctx_with_source("a { color: red; }\n");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_empty_source() {
    let rule = StylisticNoEmptyFirstLine;
    let ctx = ctx_with_source("");
    let d = rule.check_root(&[], &ctx);
    assert!(d.is_empty());
  }
}
