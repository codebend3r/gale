use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports unknown annotations in CSS comments.
///
/// Checks comments that look like `/* stylelint-` or `/* gale-` and verifies
/// the command is one of the recognized directives: "disable", "enable",
/// "disable-line", "disable-next-line".
///
/// Equivalent to Stylelint's `annotation-no-unknown` rule.
pub struct AnnotationNoUnknown;

const KNOWN_COMMANDS: &[&str] = &["disable", "disable-line", "disable-next-line", "enable"];

impl Rule for AnnotationNoUnknown {
  fn name(&self) -> &'static str {
    "annotation-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown annotations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Comment(comment) = node else {
      return vec![];
    };

    let inner = comment
      .text
      .trim_start_matches("/*")
      .trim_end_matches("*/")
      .trim();

    for prefix in &["stylelint-", "gale-"] {
      if let Some(rest) = inner.strip_prefix(prefix) {
        // Extract the command (everything up to the first space or end)
        let command = rest.split_whitespace().next().unwrap_or("");
        if !KNOWN_COMMANDS.contains(&command) {
          return vec![
            Diagnostic::new(
              self.name(),
              format!("Unexpected unknown annotation \"/* {prefix}{command} */\""),
            )
            .severity(self.default_severity())
            .span(Span::new(comment.span.offset, comment.span.length)),
          ];
        }
      }
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Comment, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn comment(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      text: text.to_string(),
      span: ParserSpan::new(0, text.len()),
      is_line: false,
    })
  }

  #[test]
  fn reports_unknown_stylelint_annotation() {
    let d = AnnotationNoUnknown.check(&comment("/* stylelint-disabel color-named */"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("stylelint-disabel"));
  }

  #[test]
  fn reports_unknown_gale_annotation() {
    let d = AnnotationNoUnknown.check(&comment("/* gale-dsiable color-named */"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("gale-dsiable"));
  }

  #[test]
  fn allows_known_annotations() {
    assert!(
      AnnotationNoUnknown
        .check(&comment("/* stylelint-disable */"), &ctx())
        .is_empty()
    );
    assert!(
      AnnotationNoUnknown
        .check(&comment("/* stylelint-enable */"), &ctx())
        .is_empty()
    );
    assert!(
      AnnotationNoUnknown
        .check(&comment("/* stylelint-disable-line */"), &ctx())
        .is_empty()
    );
    assert!(
      AnnotationNoUnknown
        .check(&comment("/* stylelint-disable-next-line */"), &ctx())
        .is_empty()
    );
    assert!(
      AnnotationNoUnknown
        .check(&comment("/* gale-disable */"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_regular_comments() {
    assert!(
      AnnotationNoUnknown
        .check(&comment("/* hello world */"), &ctx())
        .is_empty()
    );
  }
}
