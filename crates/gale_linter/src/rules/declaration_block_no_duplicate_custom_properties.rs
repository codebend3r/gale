use std::collections::HashSet;

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-block-no-duplicate-custom-properties";

declare_rule! {
  /// Disallow duplicate custom properties within declaration blocks.
  ///
  /// Equivalent to Stylelint's `declaration-block-no-duplicate-custom-properties` rule.
  pub struct DeclarationBlockNoDuplicateCustomProperties => DeclarationBlockNoDuplicateCustomPropertiesCheck {
    name: NAME,
    description: "Disallow duplicate custom properties within declaration blocks",
    default_severity: Warning,
  }
}

/// `declaration-block-no-duplicate-custom-properties` as configured.
pub struct DeclarationBlockNoDuplicateCustomPropertiesCheck {
  /// `ignoreProperties`: custom properties that may repeat.
  ignore_properties: Matchers,
}

impl DeclarationBlockNoDuplicateCustomPropertiesCheck {
  /// Stylelint's options: `true`, then `ignoreProperties`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_properties = options.matchers("ignoreProperties");
    options.finish()?;
    Ok(Self { ignore_properties })
  }
}

impl Check for DeclarationBlockNoDuplicateCustomPropertiesCheck {
  /// Flags a custom property declared more than once in the same style
  /// rule.  Interpolated names are skipped, as they cannot be resolved.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let is_preprocessor = !matches!(context.syntax, Syntax::Css);
    let mut diagnostics = Vec::new();
    for rule in tree.walk_rules() {
      let mut seen = HashSet::new();
      for decl in rule.child_decls() {
        let prop = decl.prop();
        if !prop.starts_with("--") {
          continue;
        }
        // Skip properties with SCSS/Less interpolation: the actual name
        // cannot be resolved, so duplicate detection would produce false
        // positives.
        if is_preprocessor && prop.contains("#{") {
          continue;
        }
        if self.ignore_properties.matches(prop) {
          continue;
        }
        if !seen.insert(prop) {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected duplicate custom property \"{prop}\""),
            )
            .span(decl.span()),
          );
        }
      }
    }
    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The messages and spans reported for `source` in `syntax`.
  fn check(
    source: &str,
    syntax: Syntax,
    options: Option<serde_json::Value>,
  ) -> Vec<(String, &str)> {
    let ctx = context(source, syntax);
    configured(&DeclarationBlockNoDuplicateCustomProperties, options)
      .check_source(&ctx)
      .into_iter()
      .map(|d| {
        (
          d.message,
          &source[d.span.offset..d.span.offset + d.span.length],
        )
      })
      .collect()
  }

  #[test]
  fn reports_duplicate_custom_properties() {
    let source = ":root { --color-primary: red; --color-secondary: blue; --color-primary: green; }";
    assert_eq!(
      check(source, Syntax::Css, None),
      [(
        "Unexpected duplicate custom property \"--color-primary\"".to_string(),
        "--color-primary: green;"
      )]
    );
  }

  #[test]
  fn each_rule_is_its_own_block() {
    let source = "a { --x: 1; b { --x: 2; } --y: 1; } c { --y: 2; }";
    assert!(check(source, Syntax::Scss, None).is_empty());
  }

  #[test]
  fn ignores_duplicate_non_custom_properties() {
    assert!(check("a { color: red; color: blue; }", Syntax::Css, None).is_empty());
  }

  #[test]
  fn ignores_unique_custom_properties() {
    assert!(
      check(
        ":root { --color-a: red; --color-b: blue; }",
        Syntax::Css,
        None
      )
      .is_empty()
    );
  }

  #[test]
  fn skips_interpolated_names_in_preprocessors() {
    let source = "a { --#{$x}: 1; --#{$x}: 2; }";
    assert!(check(source, Syntax::Scss, None).is_empty());
  }

  #[test]
  fn ignore_properties_may_repeat() {
    let d = check(
      "a { --x-a: 1; --x-a: 1; --y: 1; --y: 1 }",
      Syntax::Css,
      Some(serde_json::json!([true, { "ignoreProperties": ["/^--x/"] }])),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].0.contains("--y"));
  }
  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreProperties": "--x" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"declaration-block-no-duplicate-custom-properties\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["--x"] }])),
      [
        "Invalid option name \"ignore\" for rule \"declaration-block-no-duplicate-custom-properties\""
      ]
    );
  }
}
