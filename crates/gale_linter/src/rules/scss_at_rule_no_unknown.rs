use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::is_known_at_rule_for_syntax;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-rule-no-unknown";

declare_rule! {
  /// SCSS replacement for `at-rule-no-unknown`.
  ///
  /// Like the core rule but recognises SCSS-specific at-rules (`@mixin`,
  /// `@include`, `@if`, `@each`, `@extend`, `@use`, `@forward`, etc.).
  /// Only active when syntax is SCSS or Sass.
  ///
  /// `ignoreAtRules` lists at-rule names (or `/regex/`es) to leave alone.
  pub struct ScssAtRuleNoUnknown => ScssAtRuleNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown at-rules (SCSS-aware)",
    default_severity: Warning,
  }
}

/// `scss/at-rule-no-unknown` as configured.
pub struct ScssAtRuleNoUnknownCheck {
  /// `ignoreAtRules`: at-rule names (or `/regex/`es) left alone.
  ignore_at_rules: Matchers,
}

impl ScssAtRuleNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules` (names or
  /// `/regex/`es, matched against the name as written, without the `@`).
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

impl Check for ScssAtRuleNoUnknownCheck {
  /// Flags at-rules that are neither standard CSS nor SCSS. Runs on every syntax,
  /// as Stylelint does, since configs enable it project-wide.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    // Stylelint runs scss/at-rule-no-unknown on ALL file types (including
    // plain CSS) when the rule is enabled — typically via
    // stylelint-config-standard-scss.  Do not filter by syntax.

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    // Skip vendor-prefixed
    if at.name.starts_with('-') {
      return vec![];
    }

    if !self.ignore_at_rules.matches(&at.name) && !is_known_at_rule_for_syntax(&at.name, ctx.syntax)
    {
      vec![
        Diagnostic::new(NAME, format!("Unexpected unknown at-rule \"@{}\"", at.name))
          .span(Span::new(at.span.offset, at.span.length)),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Span};

  use crate::testing::{configured, ctx, invalid_options, scss_ctx};

  fn at(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_unknown_in_css() {
    let rule = configured(&ScssAtRuleNoUnknown, None);
    // Stylelint runs scss/at-rule-no-unknown on CSS files too when enabled
    // (typically via stylelint-config-standard-scss).
    assert_eq!(rule.check(&at("tailwind"), &ctx()).len(), 1);
  }

  #[test]
  fn allows_scss_at_rules() {
    let rule = configured(&ScssAtRuleNoUnknown, None);
    let ctx = scss_ctx();
    assert!(rule.check(&at("mixin"), &ctx).is_empty());
    assert!(rule.check(&at("include"), &ctx).is_empty());
    assert!(rule.check(&at("if"), &ctx).is_empty());
    assert!(rule.check(&at("each"), &ctx).is_empty());
    assert!(rule.check(&at("extend"), &ctx).is_empty());
    assert!(rule.check(&at("use"), &ctx).is_empty());
    assert!(rule.check(&at("forward"), &ctx).is_empty());
    assert!(rule.check(&at("media"), &ctx).is_empty());
  }

  #[test]
  fn reports_unknown_in_scss() {
    let rule = configured(&ScssAtRuleNoUnknown, None);
    let ctx = scss_ctx();
    let d = rule.check(&at("tailwind"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@tailwind"));
  }

  #[test]
  fn skips_vendor_prefixed() {
    let rule = configured(&ScssAtRuleNoUnknown, None);
    assert!(rule.check(&at("-webkit-keyframes"), &scss_ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["tailwind", "/^my-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-rule-no-unknown\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreAtRules": [1] }])),
      ["Invalid value \"1\" for option \"ignoreAtRules\" of rule \"scss/at-rule-no-unknown\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFunctions": ["x"] }])
      ),
      ["Invalid option name \"ignoreFunctions\" for rule \"scss/at-rule-no-unknown\""]
    );
  }

  #[test]
  fn ignore_at_rules_leaves_listed_at_rules_alone() {
    let source = "@tailwind base;\n@my-thing x;\n@other y;\n";
    let all = crate::testing::lint(
      NAME,
      serde_json::json!(true),
      source,
      gale_css_parser::Syntax::Scss,
    );
    assert_eq!(all.len(), 3);
    let options = serde_json::json!([true, { "ignoreAtRules": ["tailwind", "/^my-/"] }]);
    let left = crate::testing::lint(NAME, options, source, gale_css_parser::Syntax::Scss);
    assert_eq!(left.len(), 1);
    assert!(left[0].message.contains("@other"));
  }
}
