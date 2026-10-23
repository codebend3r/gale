use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/no-missing-end-of-source-newline";

declare_rule! {
  /// Disallow missing end-of-source newlines.
  ///
  /// Equivalent to `@stylistic/no-missing-end-of-source-newline`.
  pub struct StylisticNoMissingEndOfSourceNewline => StylisticNoMissingEndOfSourceNewlineCheck {
    name: NAME,
    description: "Disallow missing end-of-source newlines",
    default_severity: Warning,
  }
}

/// `@stylistic/no-missing-end-of-source-newline`, which takes no options.
pub struct StylisticNoMissingEndOfSourceNewlineCheck;

impl StylisticNoMissingEndOfSourceNewlineCheck {
  /// The plugin validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for StylisticNoMissingEndOfSourceNewlineCheck {
  /// Flags a file not ending in a newline. Trailing whitespace does not count,
  /// matching Stylelint.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if ctx.source.is_empty() {
      return vec![];
    }

    // Stylelint checks whether the raw source ends with '\n'.
    // It does NOT trim trailing whitespace — a file ending with "\n  "
    // is considered to be missing an end-of-source newline.
    if !ctx.source.ends_with('\n') {
      let offset = ctx.source.len().saturating_sub(1);
      vec![
        Diagnostic::new(NAME, "Unexpected missing end-of-source newline")
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

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  #[test]
  fn reports_missing_newline() {
    let rule = configured(&StylisticNoMissingEndOfSourceNewline, None);
    let ctx = ctx_with_source("a { color: red; }");
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("missing end-of-source"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_trailing_newline() {
    let rule = configured(&StylisticNoMissingEndOfSourceNewline, None);
    let ctx = ctx_with_source("a { color: red; }\n");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_empty_source() {
    let rule = configured(&StylisticNoMissingEndOfSourceNewline, None);
    let ctx = ctx_with_source("");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"@stylistic/no-missing-end-of-source-newline\""
      ]
    );
  }
}
