use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::hex_colors::hex_case_diagnostics;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, LetterCase, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "color-hex-case";

declare_rule! {
  /// Enforce lowercase hex colors (e.g. `#ABC` → `#abc`).
  ///
  /// Equivalent to Stylelint's `color-hex-case` rule: "lower" (the default)
  /// or "upper".
  pub struct ColorHexCase => ColorHexCaseCheck {
    name: NAME,
    description: "Enforce lowercase for hex colors",
    default_severity: Warning,
  }
}

/// `color-hex-case` as configured.
pub struct ColorHexCaseCheck {
  /// The case hex colors are written in.
  case: LetterCase,
}

impl ColorHexCaseCheck {
  /// Stylelint's option: `"lower"` or `"upper"`.  Gale reads no option as
  /// `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let case = options.primary_case();
    options.finish()?;
    Ok(Self { case })
  }
}

impl Check for ColorHexCaseCheck {
  /// Flags hex colors not in the configured case and fixes them.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    hex_case_diagnostics(tree, NAME, self.case, |case, expected| {
      format!("Convert to {} \"{expected}\"", case.word())
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
  fn reports_uppercase_hex() {
    let rule = configured(&ColorHexCase, None);
    let d = check(&*rule, "#FFFFFF");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ffffff"));
    assert!(d[0].fix.is_some());
    assert_eq!(d[0].span, Span::new(11, 7));
  }

  #[test]
  fn allows_lowercase_hex() {
    let rule = configured(&ColorHexCase, None);
    assert!(check(&*rule, "#abc").is_empty());
  }

  #[test]
  fn reports_mixed_case_hex() {
    let rule = configured(&ColorHexCase, None);
    let d = check(&*rule, "#aBcDeF");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#abcdef"));
  }

  #[test]
  fn reads_style_rule_declarations_only() {
    let rule = configured(&ColorHexCase, None);
    let ctx = ctx_with_source("@font-face { color: #FFF; }");
    assert!(rule.check_source(&ctx).is_empty());
  }

  #[test]
  fn upper_reports_and_fixes_lowercase_digits() {
    let rule = configured(&ColorHexCase, serde_json::json!("upper"));
    assert!(check(&*rule, "#ABCDEF").is_empty());
    let d = check(&*rule, "#aBc");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Expected \"#aBc\" to be \"#ABC\"");
    assert_eq!(d[0].fix.as_ref().unwrap().edits[0].new_text, "#ABC");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("upper")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("title")),
      ["Invalid option value \"title\" for rule \"color-hex-case\""]
    );
  }
}
