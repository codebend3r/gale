use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::{PostcssTree, RuleRef};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-invalid-position-at-import-rule";

declare_rule! {
  /// Reports `@import` rules that appear after non-import statements.
  ///
  /// `@import` rules must precede all other rules (except `@charset`, `@layer`,
  /// and comments) to be valid CSS.
  ///
  /// In SCSS/Sass files, `@use` and `@forward` are also treated as import-like
  /// rules and must precede other statements (except `@charset`, `@layer`, and
  /// `@forward`/`@use` themselves).
  ///
  /// Equivalent to Stylelint's `no-invalid-position-at-import-rule` rule.
  pub struct NoInvalidPositionAtImportRule => NoInvalidPositionAtImportRuleCheck {
    name: NAME,
    description: "Disallow invalid position @import rules",
    default_severity: Warning,
  }
}

/// `no-invalid-position-at-import-rule` as configured.
pub struct NoInvalidPositionAtImportRuleCheck {
  /// `ignoreAtRules`: at-rules (names or `/regex/` entries) that may come
  /// before an `@import`.
  ignore_at_rules: Matchers,
}

impl NoInvalidPositionAtImportRuleCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

impl Check for NoInvalidPositionAtImportRuleCheck {
  /// Flags an `@import` that follows a rule or an at-rule other than
  /// `@charset`, `@import`, a blockless `@layer` or one `ignoreAtRules`
  /// names, as Stylelint's walk does.  SCSS and Sass are skipped, where
  /// `@import` validly follows `@mixin`, `@function` and `@if`.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    if matches!(context.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diagnostics = Vec::new();
    let mut invalid_position = false;
    for node in tree.walk() {
      if let Some(at) = node.as_at_rule() {
        let name = at.name().to_ascii_lowercase();
        if name == "import" {
          if invalid_position {
            diagnostics.push(
              Diagnostic::new(NAME, "Unexpected @import after other statements").span(at.span()),
            );
          }
          continue;
        }
        let allowed = name == "charset"
          || (name == "layer" && !at.has_block())
          || self.ignore_at_rules.matches(at.name());
        if !allowed && at.is_standard_syntax() {
          invalid_position = true;
        }
      } else if node.as_rule().is_some_and(RuleRef::is_standard_syntax) {
        invalid_position = true;
      }
    }
    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, lint};

  /// The texts reported for `source` in `syntax`.
  fn reported(source: &str, syntax: Syntax) -> Vec<&str> {
    configured(&NoInvalidPositionAtImportRule, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| &source[d.span.offset..d.span.offset + d.span.length])
      .collect()
  }

  #[test]
  fn reports_import_after_style_rule() {
    let d = configured(&NoInvalidPositionAtImportRule, None)
      .check_source(&context("a {}\n@import url(\"a.css\");", Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@import"));
    assert_eq!(
      reported("a {}\n@IMPORT \"a.css\";", Syntax::Less),
      ["@IMPORT \"a.css\";"]
    );
  }

  #[test]
  fn reports_imports_in_blocks_after_other_statements() {
    let source = "@media print { @import \"a.css\"; }";
    assert_eq!(reported(source, Syntax::Css), ["@import \"a.css\";"]);
  }

  #[test]
  fn allows_import_before_rules() {
    assert!(reported("@import \"a.css\";\na {}", Syntax::Css).is_empty());
  }

  #[test]
  fn allows_charset_layer_and_declarations_before_import() {
    let source = "@charset \"utf-8\";\n@layer base;\n/* c */\n@import \"a.css\";";
    assert!(reported(source, Syntax::Css).is_empty());
    // A `@layer` block is a statement like any other.
    assert_eq!(
      reported("@layer base {}\n@import \"a.css\";", Syntax::Css).len(),
      1
    );
  }

  #[test]
  fn less_variables_and_mixin_calls_are_not_statements() {
    let source = "@color: red;\n.mixin();\n@import \"a.less\";";
    assert!(reported(source, Syntax::Less).is_empty());
  }

  #[test]
  fn skips_rule_entirely_in_scss() {
    assert!(reported("a {}\n@import \"a\";", Syntax::Scss).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["/^my-/", "foo"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["foo"] }])),
      ["Invalid option name \"ignore\" for rule \"no-invalid-position-at-import-rule\""]
    );
  }

  #[test]
  fn ignore_at_rules_may_come_before_an_import() {
    let source = "@my-rule x;\n@import url(\"a.css\");";
    assert_eq!(
      lint(NAME, serde_json::json!(true), source, Syntax::Css).len(),
      1
    );
    let ignore = serde_json::json!([true, { "ignoreAtRules": ["/^my-/"] }]);
    assert!(lint(NAME, ignore, source, Syntax::Css).is_empty());
  }
}
