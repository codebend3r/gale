use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::js_number::to_js_string;
use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-block-single-line-max-declarations";

declare_rule! {
  /// Limit the number of declarations within a single-line declaration block.
  ///
  /// Equivalent to Stylelint's `declaration-block-single-line-max-declarations` rule.
  /// Default maximum: 1. Detection-only.
  pub struct DeclarationBlockSingleLineMaxDeclarations => DeclarationBlockSingleLineMaxDeclarationsCheck {
    name: NAME,
    description: "Limit the number of declarations within a single-line declaration block",
    default_severity: Warning,
  }
}

/// `declaration-block-single-line-max-declarations` as configured.
pub struct DeclarationBlockSingleLineMaxDeclarationsCheck {
  /// The most declarations a single-line block may hold.
  max: f64,
}

impl DeclarationBlockSingleLineMaxDeclarationsCheck {
  /// Stylelint's option: a number.  Gale reads no option as 1.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_number(&[Accept::Number]);
    options.finish()?;
    Ok(Self {
      max: max.unwrap_or(MAX_DECLARATIONS),
    })
  }
}

/// The most declarations a single-line block may hold when the config
/// gives no number.
const MAX_DECLARATIONS: f64 = 1.0;

impl Check for DeclarationBlockSingleLineMaxDeclarationsCheck {
  /// Flags single-line rules holding more than the configured number of
  /// declarations. Rules written over several lines are skipped.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_rules()
      // The whole rule, selector included, must sit on one line.
      .filter(|rule| !rule.source_text().contains('\n'))
      .filter_map(|rule| {
        let count = rule.child_decls().count();
        (count as f64 > self.max).then(|| {
          Diagnostic::new(
            NAME,
            format!(
              "Expected no more than {} declaration(s) in a single-line block, found {count}",
              to_js_string(self.max)
            ),
          )
          .span(rule.span())
        })
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  /// The messages reported for the CSS `source` with `options`.
  fn messages(source: &str, options: Option<serde_json::Value>) -> Vec<String> {
    configured(&DeclarationBlockSingleLineMaxDeclarations, options)
      .check_source(&ctx_with_source(source))
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn reports_multiple_decls_single_line() {
    let d = configured(&DeclarationBlockSingleLineMaxDeclarations, None)
      .check_source(&ctx_with_source("a { color: red; font-size: 16px; }"));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 2"));
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 34));
  }

  #[test]
  fn allows_single_decl_single_line() {
    assert!(messages("a { color: red; }", None).is_empty());
  }

  #[test]
  fn allows_multiple_decls_multi_line() {
    assert!(messages("a {\n  color: red;\n  font-size: 16px;\n}", None).is_empty());
  }

  #[test]
  fn ignores_blockless_at_rules() {
    let source = "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n";
    assert!(messages(source, None).is_empty());
  }

  #[test]
  fn reads_the_configured_maximum() {
    let source = "a { color: red; top: 0; }";
    assert!(messages(source, Some(serde_json::json!(2))).is_empty());
    // The array form Stylelint configs use for secondary options.
    let one = Some(serde_json::json!([1, { "severity": "warning" }]));
    assert_eq!(messages(source, one).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(3)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("3")),
      ["Invalid option value \"3\" for rule \"declaration-block-single-line-max-declarations\""]
    );
  }
}
