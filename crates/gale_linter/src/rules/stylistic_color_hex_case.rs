use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::hex_colors::hex_case_diagnostics;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, LetterCase, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/color-hex-case";

declare_rule! {
  /// Specify lowercase or uppercase for hex colors (@stylistic/ version).
  ///
  /// Equivalent to `@stylistic/color-hex-case`. Supports both "lower" and "upper"
  /// primary options, unlike the core `color-hex-case` which only enforces lowercase.
  pub struct StylisticColorHexCase => StylisticColorHexCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for hex colors",
    default_severity: Warning,
  }
}

/// `@stylistic/color-hex-case` as configured.
pub struct StylisticColorHexCaseCheck {
  /// The case hex colors are written in.
  case: LetterCase,
}

impl StylisticColorHexCaseCheck {
  /// The plugin's options: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let case = options.primary_case();
    options.finish()?;
    Ok(Self { case })
  }
}

impl Check for StylisticColorHexCaseCheck {
  /// Flags hex colors not in the configured case and offers the converted value
  /// as a fix.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    hex_case_diagnostics(tree, NAME, self.case, |case, _| {
      format!("Convert to {}", case.word())
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_diagnostics::Span;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(rule: &dyn Check, value: &str) -> Vec<Diagnostic> {
    let source = format!("a {{ color: {value}; }}");
    rule.check_source(&ctx_with_source(&source))
  }

  #[test]
  fn reports_uppercase_hex_default_lower() {
    let d = check(&*configured(&StylisticColorHexCase, None), "#FFFFFF");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffff"));
    assert_eq!(d[0].span, Span::new(11, 7));
  }

  #[test]
  fn allows_lowercase_hex() {
    let d = check(&*configured(&StylisticColorHexCase, None), "#abc");
    assert!(d.is_empty());
  }

  #[test]
  fn reports_mixed_case() {
    let d = check(&*configured(&StylisticColorHexCase, None), "#aBcDeF");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#abcdef"));
  }

  #[test]
  fn upper_reports_lowercase_hex() {
    let rule = configured(&StylisticColorHexCase, serde_json::json!("upper"));
    let d = check(&*rule, "#aBc");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ABC"));
  }

  #[test]
  fn only_lower_and_upper_are_options() {
    assert_eq!(
      invalid_options(NAME, serde_json::json!("Lower")),
      ["Invalid option value \"Lower\" for rule \"@stylistic/color-hex-case\""]
    );
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
  }
}
