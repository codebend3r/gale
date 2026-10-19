use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "annotation-no-unknown";

declare_rule! {
  /// Reports unknown annotations in CSS comments.
  ///
  /// Checks comments that look like `/* stylelint-` or `/* gale-` and verifies
  /// the command is one of the recognized directives: "disable", "enable",
  /// "disable-line", "disable-next-line".
  ///
  /// Equivalent to Stylelint's `annotation-no-unknown` rule.
  pub struct AnnotationNoUnknown => AnnotationNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown annotations",
    default_severity: Warning,
  }
}

/// `annotation-no-unknown` as configured.
pub struct AnnotationNoUnknownCheck;

impl AnnotationNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreAnnotations`.  Gale accepts
  /// but does not apply `ignoreAnnotations`: Stylelint's list names `!`
  /// annotations in declaration values, which this rule does not check.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.strings("ignoreAnnotations");
    options.finish()?;
    Ok(Self)
  }
}

const KNOWN_COMMANDS: &[&str] = &["disable", "disable-line", "disable-next-line", "enable"];

impl Check for AnnotationNoUnknownCheck {
  /// Flags a comment whose `stylelint-`/`gale-` prefix is followed by a command
  /// outside the known directive set.
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
              NAME,
              format!("Unexpected unknown annotation \"/* {prefix}{command} */\""),
            )
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
  use gale_css_parser::{Comment, Span as ParserSpan};
  use serde_json::json;

  use crate::testing::{configured, ctx, invalid_options};

  fn comment(text: &str) -> CssNode {
    CssNode::Comment(Comment {
      text: text.to_string(),
      span: ParserSpan::new(0, text.len()),
      is_line: false,
    })
  }

  #[test]
  fn reports_unknown_stylelint_annotation() {
    let rule = configured(&AnnotationNoUnknown, None);
    let d = rule.check(&comment("/* stylelint-disabel color-named */"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("stylelint-disabel"));
  }

  #[test]
  fn reports_unknown_gale_annotation() {
    let rule = configured(&AnnotationNoUnknown, None);
    let d = rule.check(&comment("/* gale-dsiable color-named */"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("gale-dsiable"));
  }

  #[test]
  fn allows_known_annotations() {
    let rule = configured(&AnnotationNoUnknown, None);
    assert!(
      rule
        .check(&comment("/* stylelint-disable */"), &ctx())
        .is_empty()
    );
    assert!(
      rule
        .check(&comment("/* stylelint-enable */"), &ctx())
        .is_empty()
    );
    assert!(
      rule
        .check(&comment("/* stylelint-disable-line */"), &ctx())
        .is_empty()
    );
    assert!(
      rule
        .check(&comment("/* stylelint-disable-next-line */"), &ctx())
        .is_empty()
    );
    assert!(
      rule
        .check(&comment("/* gale-disable */"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_regular_comments() {
    let rule = configured(&AnnotationNoUnknown, None);
    assert!(rule.check(&comment("/* hello world */"), &ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!([true, { "ignoreAnnotations": ["/^x/"] }])).is_empty());
    assert_eq!(
      invalid_options(NAME, json!("always")),
      ["Unexpected option value \"always\" for rule \"annotation-no-unknown\""]
    );
    assert_eq!(
      invalid_options(NAME, json!([true, { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"annotation-no-unknown\""]
    );
  }
}
