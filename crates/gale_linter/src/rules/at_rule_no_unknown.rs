use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::is_known_at_rule_for_syntax;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::stylelint_version::stylelint_major_version;

const NAME: &str = "at-rule-no-unknown";

declare_rule! {
  /// Disallow unknown at-rules.
  ///
  /// Equivalent to Stylelint's `at-rule-no-unknown` rule, including its
  /// `ignoreAtRules` option (names or `/regex/` entries, matched against the
  /// name as written, without the `@`).
  pub struct AtRuleNoUnknown => AtRuleNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown at-rules",
    default_severity: Warning,
  }
}

/// `at-rule-no-unknown` as configured.
pub struct AtRuleNoUnknownCheck {
  /// `ignoreAtRules`.
  ignore_at_rules: Matchers,
}

impl AtRuleNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

impl Check for AtRuleNoUnknownCheck {
  /// Flags at-rules that are not standard for the file's syntax. Vendor-prefixed
  /// names and those in `ignoreAtRules` are left alone.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    // Skip vendor-prefixed
    if at.name.starts_with('-') {
      return vec![];
    }
    if self.ignore_at_rules.matches(&at.name) || is_known_at_rule_for_syntax(&at.name, ctx.syntax) {
      return vec![];
    }
    let at_rule = format!("@{}", at.name);
    let message = if stylelint_major_version() >= 17 {
      format!("Unknown at-rule \"{at_rule}\"")
    } else {
      format!("Unexpected unknown at-rule \"{at_rule}\"")
    };
    vec![
      Diagnostic::new(NAME, message)
        .span(Span::new(at.span.offset, at.span.length))
        .message_args([at_rule]),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Span};

  use crate::testing::{configured, ctx, invalid_options, less_ctx, scss_ctx};

  fn at(name: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_unknown() {
    let rule = configured(&AtRuleNoUnknown, None);
    let d = rule.check(&at("tailwind"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@tailwind"));
  }

  #[test]
  fn ignore_at_rules_skips_listed_names() {
    // material-ui's config, for Tailwind's at-rules.
    let options = serde_json::json!([true, { "ignoreAtRules": ["theme", "/^conf/"] }]);
    let rule = configured(&AtRuleNoUnknown, options.clone());
    let ctx = ctx();
    assert!(rule.check(&at("theme"), &ctx).is_empty());
    assert!(rule.check(&at("config"), &ctx).is_empty());
    assert_eq!(rule.check(&at("tailwind"), &ctx).len(), 1);
  }

  #[test]
  fn allows_known() {
    let rule = configured(&AtRuleNoUnknown, None);
    assert!(rule.check(&at("media"), &ctx()).is_empty());
    assert!(rule.check(&at("keyframes"), &ctx()).is_empty());
  }

  #[test]
  fn skips_vendor_prefixed() {
    let rule = configured(&AtRuleNoUnknown, None);
    assert!(rule.check(&at("-webkit-keyframes"), &ctx()).is_empty());
  }

  #[test]
  fn allows_scss_at_rules_in_scss() {
    let rule = configured(&AtRuleNoUnknown, None);
    let ctx = scss_ctx();
    assert!(rule.check(&at("mixin"), &ctx).is_empty());
    assert!(rule.check(&at("include"), &ctx).is_empty());
    assert!(rule.check(&at("if"), &ctx).is_empty());
    assert!(rule.check(&at("each"), &ctx).is_empty());
    assert!(rule.check(&at("extend"), &ctx).is_empty());
    assert!(rule.check(&at("use"), &ctx).is_empty());
    assert!(rule.check(&at("forward"), &ctx).is_empty());
    assert!(rule.check(&at("at-root"), &ctx).is_empty());
  }

  #[test]
  fn reports_scss_at_rules_in_css() {
    let rule = configured(&AtRuleNoUnknown, None);
    let ctx = self::ctx();
    assert_eq!(rule.check(&at("mixin"), &ctx).len(), 1);
    assert_eq!(rule.check(&at("include"), &ctx).len(), 1);
    assert_eq!(rule.check(&at("if"), &ctx).len(), 1);
  }

  #[test]
  fn allows_less_at_rules_in_less() {
    let rule = configured(&AtRuleNoUnknown, None);
    let ctx = less_ctx();
    assert!(rule.check(&at("plugin"), &ctx).is_empty());
    assert!(rule.check(&at("detached-ruleset"), &ctx).is_empty());
  }

  #[test]
  fn reports_less_at_rules_in_css() {
    let rule = configured(&AtRuleNoUnknown, None);
    let ctx = self::ctx();
    assert_eq!(rule.check(&at("plugin"), &ctx).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": "tailwind" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("never")),
      ["Unexpected option value \"never\" for rule \"at-rule-no-unknown\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreAtRule": ["x"] }])),
      ["Invalid option name \"ignoreAtRule\" for rule \"at-rule-no-unknown\""]
    );
  }
}
