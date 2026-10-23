use gale_css_parser::postcss_tree::PostcssTree;
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
  /// Flags a vendor-prefixed at-rule at its `@name` word, with a fix that
  /// drops the prefix.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      let name = at.name();
      if !at.is_standard_syntax() || self.ignore_at_rules.matches(name) || !name.starts_with('-') {
        continue;
      }
      let unprefixed = strip_known_vendor_prefix(name);
      // `@` and the name, exactly as written.
      let word = Span::new(at.start, name.len() + 1);
      diags.push(
        Diagnostic::new(
          NAME,
          format!("Unexpected vendor-prefixed at-rule \"@{name}\""),
        )
        .span(word)
        .fix(Fix::new(
          format!("Remove vendor prefix from @{name}"),
          vec![Edit::new(word, format!("@{unprefixed}"))],
        )),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, invalid_options, lint};

  #[test]
  fn reports_vendor_prefixed_at_rule_at_its_name() {
    for syntax in [Syntax::Css, Syntax::Scss] {
      let d = lint(
        NAME,
        json!(true),
        "a {}\n@-webkit-keyframes fade {}",
        syntax,
      );
      assert_eq!(d.len(), 1);
      assert!(d[0].message.contains("-webkit-keyframes"));
      assert_eq!((d[0].span.offset, d[0].span.length), (5, 18));
    }
  }

  #[test]
  fn allows_standard_at_rule() {
    let css = "@keyframes fade {}\n@media screen {}";
    assert!(lint(NAME, json!(true), css, Syntax::Css).is_empty());
  }

  #[test]
  fn fix_drops_the_prefix() {
    let css = "@-webkit-keyframes fade { }\n@-MOZ-document url(x) { }";
    assert_eq!(
      fix(NAME, json!(true), css, Syntax::Css),
      "@keyframes fade { }\n@document url(x) { }"
    );
  }

  #[test]
  fn ignore_at_rules_skips_listed_names() {
    let options = json!([true, { "ignoreAtRules": ["/^-moz-/"] }]);
    let css = "@-moz-document url(x) {}\n@-webkit-keyframes fade {}";
    let d = lint(NAME, options, css, Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("-webkit-keyframes"));
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
