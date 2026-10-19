use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

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
  /// Flags an `@import` that follows a non-import statement. SCSS and Sass are
  /// skipped, where the ordering semantics differ.
  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    // In SCSS/Sass, `@import` can validly appear after `@mixin`, `@function`,
    // and inside `@if` blocks. The import ordering semantics differ from CSS.
    // Skip this rule entirely for SCSS/Sass to avoid false positives.
    if matches!(context.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut diagnostics = Vec::new();
    let mut seen_non_import = false;
    let is_scss = false;

    for node in nodes {
      match node {
        // Comments are always allowed anywhere.
        CssNode::Comment(_) => continue,

        CssNode::AtRule(at_rule) => {
          let name = at_rule.name.as_str();

          // These are always allowed before @import.
          if name == "charset" || name == "layer" {
            continue;
          }

          // In SCSS, @use and @forward are import-like and allowed
          // before @import (and subject to the same ordering rule).
          if is_scss && (name == "use" || name == "forward") {
            if seen_non_import {
              diagnostics.push(
                Diagnostic::new(NAME, format!("Unexpected @{name} after other statements"))
                  .span(Span::new(at_rule.span.offset, at_rule.span.length)),
              );
            }
            continue;
          }

          if name == "import" {
            if seen_non_import {
              diagnostics.push(
                Diagnostic::new(NAME, "Unexpected @import after other statements")
                  .span(Span::new(at_rule.span.offset, at_rule.span.length)),
              );
            }
          } else if !self.ignore_at_rules.matches(name) {
            seen_non_import = true;
          }
        }

        // Any other node (style rules, declarations) counts as non-import.
        _ => {
          seen_non_import = true;
        }
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, scss_ctx};

  #[test]
  fn reports_import_after_style_rule() {
    let rule = configured(&NoInvalidPositionAtImportRule, None);
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 5),
        ..Default::default()
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "url.css".to_string(),
        span: ParserSpan::new(10, 20),
        children: vec![],
      }),
    ];
    let context = ctx();
    let diags = rule.check_root(&nodes, &context);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("@import"));
  }

  #[test]
  fn allows_import_before_rules() {
    let rule = configured(&NoInvalidPositionAtImportRule, None);
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(0, 20),
        children: vec![],
      }),
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(25, 5),
        ..Default::default()
      }),
    ];
    let context = ctx();
    let diags = rule.check_root(&nodes, &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_use_and_forward_before_import_in_scss() {
    let rule = configured(&NoInvalidPositionAtImportRule, None);
    // SCSS files now skip this rule entirely, so no diagnostics expected.
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "use".to_string(),
        params: "'sass:math'".to_string(),
        span: ParserSpan::new(0, 20),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "forward".to_string(),
        params: "'variables'".to_string(),
        span: ParserSpan::new(21, 25),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(50, 20),
        children: vec![],
      }),
    ];
    let context = scss_ctx();
    let diags = rule.check_root(&nodes, &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn skips_use_after_style_rule_in_scss() {
    let rule = configured(&NoInvalidPositionAtImportRule, None);
    // SCSS files now skip this rule entirely, so even misplaced @use is not flagged.
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 5),
        ..Default::default()
      }),
      CssNode::AtRule(AtRule {
        name: "use".to_string(),
        params: "'sass:math'".to_string(),
        span: ParserSpan::new(10, 20),
        children: vec![],
      }),
    ];
    let context = scss_ctx();
    let diags = rule.check_root(&nodes, &context);
    assert!(diags.is_empty());
  }

  #[test]
  fn skips_rule_entirely_in_scss() {
    let rule = configured(&NoInvalidPositionAtImportRule, None);
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 5),
        ..Default::default()
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "url.css".to_string(),
        span: ParserSpan::new(10, 20),
        children: vec![],
      }),
    ];
    let context = scss_ctx();
    let diags = rule.check_root(&nodes, &context);
    assert!(
      diags.is_empty(),
      "SCSS files should skip this rule entirely"
    );
  }

  #[test]
  fn allows_charset_and_layer_before_import() {
    let rule = configured(&NoInvalidPositionAtImportRule, None);
    let nodes = vec![
      CssNode::AtRule(AtRule {
        name: "charset".to_string(),
        params: "UTF-8".to_string(),
        span: ParserSpan::new(0, 15),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "layer".to_string(),
        params: "base".to_string(),
        span: ParserSpan::new(16, 12),
        children: vec![],
      }),
      CssNode::AtRule(AtRule {
        name: "import".to_string(),
        params: "reset.css".to_string(),
        span: ParserSpan::new(30, 20),
        children: vec![],
      }),
    ];
    let context = ctx();
    let diags = rule.check_root(&nodes, &context);
    assert!(diags.is_empty());
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
    use crate::testing::lint;
    let source = "@my-rule;\n@import url(\"a.css\");";
    assert_eq!(
      lint(NAME, serde_json::json!(true), source, Syntax::Css).len(),
      1
    );
    let ignore = serde_json::json!([true, { "ignoreAtRules": ["/^my-/"] }]);
    assert!(lint(NAME, ignore, source, Syntax::Css).is_empty());
  }
}
