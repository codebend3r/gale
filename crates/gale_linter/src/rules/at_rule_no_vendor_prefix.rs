use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "at-rule-no-vendor-prefix";

declare_rule! {
  /// Reports vendor-prefixed at-rules (e.g. `@-webkit-keyframes`).
  ///
  /// Equivalent to Stylelint's `at-rule-no-vendor-prefix` rule.
  pub struct AtRuleNoVendorPrefix => AtRuleNoVendorPrefixCheck {
    name: NAME,
    description: "Disallow vendor prefixes for at-rules",
    default_severity: Warning,
  }
}

/// `at-rule-no-vendor-prefix` as configured.
pub struct AtRuleNoVendorPrefixCheck {
  /// `ignoreAtRules`: names (as written, prefix included) and `/regex/`
  /// entries that are left alone.
  ignore_at_rules: Matchers,
}

impl AtRuleNoVendorPrefixCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

/// The at-rule name without a `-webkit-`, `-moz-`, `-ms-` or `-o-` prefix:
/// `-webkit-keyframes` → `keyframes`.
///
/// Not Stylelint's `unprefix` (`/-\w+-/`): Stylelint reports only the names
/// Autoprefixer handles, where the two agree, while gale reports every name
/// starting with `-` and leaves an unknown prefix (`@--my-rule`) in place.
fn strip_known_vendor_prefix(name: &str) -> &str {
  let lower = name.to_ascii_lowercase();
  for prefix in &["-webkit-", "-moz-", "-ms-", "-o-"] {
    if lower.starts_with(prefix) {
      return &name[prefix.len()..];
    }
  }
  name
}

impl Check for AtRuleNoVendorPrefixCheck {
  /// Flags a vendor-prefixed at-rule and offers a fix that drops the prefix,
  /// locating the name in the source so the edit span is exact.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(rule) = node else {
      return vec![];
    };
    if self.ignore_at_rules.matches(&rule.name) {
      return vec![];
    }
    if rule.name.starts_with('-') {
      let unprefixed = strip_known_vendor_prefix(&rule.name);
      // Search the source within the at-rule span for `@-prefix-name`
      let rule_start = rule.span.offset;
      let rule_end = rule_start + rule.span.length;
      let prefixed_at = format!("@{}", rule.name);
      let unprefixed_at = format!("@{unprefixed}");

      let fix = if rule_end <= ctx.source.len() && rule_start < rule_end {
        let search_area = &ctx.source[rule_start..rule_end];
        // Case-insensitive search for the prefixed at-rule name
        let lower_search = search_area.to_ascii_lowercase();
        let lower_target = prefixed_at.to_ascii_lowercase();
        lower_search.find(&lower_target).map(|rel_offset| {
          let abs_offset = rule_start + rel_offset;
          Fix::new(
            format!("Remove vendor prefix from @{}", rule.name),
            vec![Edit::new(
              Span::new(abs_offset, prefixed_at.len()),
              &unprefixed_at,
            )],
          )
        })
      } else {
        None
      };

      let mut diag = Diagnostic::new(
        NAME,
        format!("Unexpected vendor-prefixed at-rule \"@{}\"", rule.name),
      )
      .span(rule.span);

      if let Some(f) = fix {
        diag = diag.fix(f);
      }

      vec![diag]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::AtRule as CssAtRule;

  use crate::testing::{configured, ctx, ctx_with_source, invalid_options};

  fn at_rule(name: &str) -> CssNode {
    CssNode::AtRule(CssAtRule {
      name: name.to_string(),
      params: "fade".to_string(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_vendor_prefixed_at_rule() {
    let rule = configured(&AtRuleNoVendorPrefix, None);
    let d = rule.check(&at_rule("-webkit-keyframes"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("-webkit-keyframes"));
  }

  #[test]
  fn allows_standard_at_rule() {
    let rule = configured(&AtRuleNoVendorPrefix, None);
    assert!(rule.check(&at_rule("keyframes"), &ctx()).is_empty());
    assert!(rule.check(&at_rule("media"), &ctx()).is_empty());
  }

  #[test]
  fn emits_fix_for_vendor_prefixed_at_rule() {
    let rule = configured(&AtRuleNoVendorPrefix, None);
    let source = "@-webkit-keyframes fade { }";
    let ctx = ctx_with_source(source);
    let node = CssNode::AtRule(CssAtRule {
      name: "-webkit-keyframes".to_string(),
      params: "fade".to_string(),
      span: Span::new(0, source.len()),
      children: vec![],
    });
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_some());
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].new_text, "@keyframes");
  }

  #[test]
  fn ignore_at_rules_skips_listed_names() {
    let rule = configured(
      &AtRuleNoVendorPrefix,
      serde_json::json!([true, { "ignoreAtRules": ["/^-moz-/"] }]),
    );
    assert!(rule.check(&at_rule("-moz-document"), &ctx()).is_empty());
    assert_eq!(rule.check(&at_rule("-webkit-keyframes"), &ctx()).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["-webkit-keyframes"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"at-rule-no-vendor-prefix\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreAtRules": [1] }])),
      ["Invalid value \"1\" for option \"ignoreAtRules\" of rule \"at-rule-no-vendor-prefix\""]
    );
  }
}
