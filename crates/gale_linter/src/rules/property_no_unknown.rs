use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::data::is_known_property;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

/// Pattern matcher for ignoreProperties — supports plain strings and regex-like "/pattern/".
enum PropertyMatcher {
  Exact(String),
  Regex(std::sync::Arc<crate::pattern::Regex>),
}

impl PropertyMatcher {
  /// Builds a matcher: `/…/` is a regex, anything else an exact lowercase name.
  fn from_pattern(pattern: &str) -> Self {
    match crate::pattern::regex_entry(pattern) {
      Some(re) => PropertyMatcher::Regex(re),
      None => PropertyMatcher::Exact(pattern.to_ascii_lowercase()),
    }
  }

  /// Whether the property matches, comparing case-insensitively for exact names.
  fn matches(&self, lower_prop: &str, original_prop: &str) -> bool {
    match self {
      PropertyMatcher::Exact(s) => lower_prop == s,
      PropertyMatcher::Regex(re) => {
        crate::pattern::is_match(re, original_prop) || crate::pattern::is_match(re, lower_prop)
      }
    }
  }
}

const NAME: &str = "property-no-unknown";

declare_rule! {
  pub struct PropertyNoUnknown => PropertyNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown properties",
    default_severity: Warning,
  }
}

/// `property-no-unknown` as configured.
pub struct PropertyNoUnknownCheck {
  /// `ignoreProperties`: names (any case) and `/regex/` entries.
  ignore_properties: Vec<PropertyMatcher>,
  /// `ignoreSelectors`: the selectors of rules whose properties are not
  /// checked.
  ignore_selectors: Matchers,
}

impl PropertyNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreProperties`, `checkPrefixed`,
  /// `ignoreSelectors` and `ignoreAtRules`.  Gale accepts but does not yet
  /// apply `checkPrefixed` and `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_properties = options.strings("ignoreProperties");
    options.flag("checkPrefixed");
    let ignore_selectors = options.matchers("ignoreSelectors");
    options.strings("ignoreAtRules");
    options.finish()?;
    Ok(Self {
      ignore_properties: ignore_properties
        .iter()
        .map(|pattern| PropertyMatcher::from_pattern(pattern))
        .collect(),
      ignore_selectors,
    })
  }
}

impl Check for PropertyNoUnknownCheck {
  /// Flags properties that are not standard CSS, skipping any matched by
  /// `ignoreProperties`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    if self.ignore_selectors.matches(&rule.selector) {
      return vec![];
    }

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      let prop = &decl.property;
      // Skip custom properties and vendor-prefixed
      if prop.starts_with("--") || prop.starts_with('-') {
        continue;
      }
      // Skip SCSS variable declarations ($var)
      if matches!(
        ctx.syntax,
        gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
      ) && prop.starts_with('$')
      {
        continue;
      }
      // Skip properties containing SCSS interpolation #{...}
      if matches!(
        ctx.syntax,
        gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
      ) && prop.contains("#{")
      {
        continue;
      }
      // Skip Less variable declarations (@var)
      if ctx.syntax == gale_css_parser::Syntax::Less && prop.starts_with('@') {
        continue;
      }
      // Skip explicitly ignored properties (exact match or regex pattern)
      let prop_lower = prop.to_ascii_lowercase();
      if self
        .ignore_properties
        .iter()
        .any(|m| m.matches(&prop_lower, prop))
      {
        continue;
      }
      if !is_known_property(prop) {
        diags.push(
          Diagnostic::new(NAME, format!("Unexpected unknown property \"{prop}\"")).span(decl.span),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use gale_linter_test_helper::*;

  use crate::testing::{configured, ctx};

  #[test]
  fn reports_unknown() {
    let rule = configured(&PropertyNoUnknown, None);
    let node = style_node("a", &[("colr", "red")]);
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("colr"));
  }

  #[test]
  fn allows_known() {
    let rule = configured(&PropertyNoUnknown, None);
    let node = style_node("a", &[("color", "red")]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn skips_custom_and_vendor() {
    let rule = configured(&PropertyNoUnknown, None);
    let node = style_node("a", &[("--my-var", "1"), ("-webkit-appearance", "none")]);
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn ignore_properties_exact() {
    let node = style_node("a", &[("mso-padding-alt", "0")]);
    let opts = serde_json::json!({ "ignoreProperties": ["mso-padding-alt"] });
    let rule = configured(&PropertyNoUnknown, opts.clone());
    let c = RuleContext {
      env: Default::default(),
      ..ctx()
    };
    assert!(rule.check(&node, &c).is_empty());
  }

  #[test]
  fn ignore_properties_regex_pattern() {
    let node = style_node("a", &[("mso-padding-alt", "0"), ("mso-table-lspace", "0")]);
    let opts = serde_json::json!({ "ignoreProperties": ["/^mso-/"] });
    let rule = configured(&PropertyNoUnknown, opts.clone());
    let c = RuleContext {
      env: Default::default(),
      ..ctx()
    };
    assert!(rule.check(&node, &c).is_empty());
  }

  #[test]
  fn ignore_properties_regex_does_not_match_other() {
    let node = style_node("a", &[("colr", "red")]);
    let opts = serde_json::json!({ "ignoreProperties": ["/^mso-/"] });
    let rule = configured(&PropertyNoUnknown, opts.clone());
    let c = RuleContext {
      env: Default::default(),
      ..ctx()
    };
    assert_eq!(rule.check(&node, &c).len(), 1);
  }

  mod gale_linter_test_helper {
    use gale_css_parser::*;
    use gale_diagnostics::Span;
    pub fn style_node(sel: &str, props: &[(&str, &str)]) -> CssNode {
      CssNode::Style(StyleRule {
        selector: sel.to_string(),
        declarations: props
          .iter()
          .map(|(p, v)| Declaration {
            property: p.to_string(),
            value: v.to_string(),
            span: Span::new(0, 0),
            important: false,
          })
          .collect(),
        span: Span::new(0, 0),
        ..Default::default()
      })
    }
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    let valid = serde_json::json!([true, {
      "ignoreProperties": ["/^mso-/"],
      "checkPrefixed": true,
      "ignoreSelectors": [":export"],
      "ignoreAtRules": ["/^my-/"]
    }]);
    assert!(invalid_options(NAME, valid).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "checkPrefixed": "yes" }])),
      ["Invalid value \"yes\" for option \"checkPrefixed\" of rule \"property-no-unknown\""]
    );
  }

  #[test]
  fn ignore_selectors_skips_matching_rules() {
    let rule = configured(
      &PropertyNoUnknown,
      serde_json::json!([true, { "ignoreSelectors": ["/^:export/"] }]),
    );
    assert!(
      rule
        .check(&style_node(":export", &[("colr", "red")]), &ctx())
        .is_empty()
    );
    assert_eq!(
      rule
        .check(&style_node("a", &[("colr", "red")]), &ctx())
        .len(),
      1
    );
  }
}
