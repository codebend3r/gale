use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

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
  /// Stands in for Stylelint's `annotation-no-unknown` rule, which checks
  /// `!word` annotations in declaration values instead.  It only looks at
  /// SCSS, Less and Sass files: the older CSS parser kept no comments, so it
  /// never reported in plain CSS, and doing so now would add problems
  /// Stylelint never gives.
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
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if ctx.syntax == Syntax::Css {
      return vec![];
    }
    let mut diags = Vec::new();
    for comment in tree.walk_comments() {
      let inner = comment.text();
      for prefix in &["stylelint-", "gale-"] {
        if let Some(rest) = inner.strip_prefix(prefix) {
          // The command runs up to the first whitespace.
          let command = rest.split_whitespace().next().unwrap_or("");
          if !KNOWN_COMMANDS.contains(&command) {
            diags.push(
              Diagnostic::new(
                NAME,
                format!("Unexpected unknown annotation \"/* {prefix}{command} */\""),
              )
              .span(comment.span()),
            );
            break;
          }
        }
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  use crate::testing::{CheckSource, configured, invalid_options, lint, scss_ctx_with_source};

  /// The rule's own reports, without the runner's for the directives.
  fn scss(source: &str) -> Vec<Diagnostic> {
    configured(&AnnotationNoUnknown, None).check_source(&scss_ctx_with_source(source))
  }

  #[test]
  fn reports_unknown_stylelint_annotation() {
    let d = scss("/* stylelint-disabel color-named */");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("stylelint-disabel"));
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 35));
  }

  #[test]
  fn reports_unknown_gale_annotation() {
    let d = lint(
      NAME,
      json!(true),
      "// gale-dsiable color-named",
      Syntax::Less,
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("gale-dsiable"));
  }

  #[test]
  fn allows_known_annotations() {
    for comment in [
      "/* stylelint-disable */",
      "/* stylelint-enable */",
      "/* stylelint-disable-line */",
      "// stylelint-disable-next-line",
      "/* gale-disable */",
    ] {
      let d = scss(comment);
      assert!(d.is_empty(), "{comment} {d:?}");
    }
  }

  #[test]
  fn ignores_regular_comments_and_plain_css() {
    assert!(scss("/* hello world */").is_empty());
    let css = "/* stylelint-disabel color-named */";
    assert!(lint(NAME, json!(true), css, Syntax::Css).is_empty());
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
