use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/unicode-bom";

declare_rule! {
  /// Require or disallow Unicode BOM.
  ///
  /// Equivalent to `@stylistic/unicode-bom`.
  pub struct StylisticUnicodeBom => StylisticUnicodeBomCheck {
    name: NAME,
    description: "Require or disallow Unicode BOM",
    default_severity: Warning,
  }
}

/// `@stylistic/unicode-bom` as configured.
pub struct StylisticUnicodeBomCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
}

impl StylisticUnicodeBomCheck {
  /// The plugin's options: `"always"` or `"never"`.  Gale reads no option as
  /// `"never"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

const BOM: &str = "\u{FEFF}";

impl Check for StylisticUnicodeBomCheck {
  /// Flags a file whose byte-order mark does not match the option.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let has_bom = ctx.source.starts_with(BOM);

    match option {
      "always" if !has_bom => {
        vec![
          Diagnostic::new(NAME, "Expected Unicode BOM")
            .span(Span::new(0, 0))
            .fix(Fix::new(
              "Add Unicode BOM",
              vec![Edit::new(Span::new(0, 0), BOM)],
            )),
        ]
      }
      "never" if has_bom => {
        vec![
          Diagnostic::new(NAME, "Unexpected Unicode BOM")
            .span(Span::new(0, BOM.len()))
            .fix(Fix::new(
              "Remove Unicode BOM",
              vec![Edit::new(Span::new(0, BOM.len()), "")],
            )),
        ]
      }
      _ => vec![],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  #[test]
  fn reports_bom_when_never() {
    let rule = configured(&StylisticUnicodeBom, None);
    let source = format!("\u{FEFF}a {{ color: red; }}");
    let ctx = ctx_with_source(&source);
    let d = rule.check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
  }

  #[test]
  fn allows_no_bom_when_never() {
    let rule = configured(&StylisticUnicodeBom, None);
    let ctx = ctx_with_source("a { color: red; }");
    let d = rule.check_source(&ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/unicode-bom\""]
    );
  }
}
