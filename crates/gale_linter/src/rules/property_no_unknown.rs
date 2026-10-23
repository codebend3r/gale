use gale_css_parser::postcss_tree::{NodeRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};

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
  ///
  /// Only the declarations of style rules are read (keyframe blocks
  /// included); Stylelint also reads those of at-rules.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      let Some(rule) = decl.parent().and_then(NodeRef::as_rule) else {
        continue;
      };
      // SCSS nested properties (`font: { family: x }`) and other
      // non-standard declarations are not properties.
      if !decl.is_standard_syntax() || self.ignore_selectors.matches(rule.selector()) {
        continue;
      }
      let prop = decl.prop();
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
        // Stylelint reports the property, not the whole declaration.
        let start = decl.prop_start();
        diags.push(
          Diagnostic::new(NAME, format!("Unexpected unknown property \"{prop}\""))
            .span(Span::from_range(start, start + prop.len())),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, ctx_with_source};

  /// A rule `sel` holding the declarations `props`.
  fn style_node(sel: &str, props: &[(&str, &str)]) -> String {
    let decls: String = props.iter().map(|(p, v)| format!(" {p}: {v};")).collect();
    format!("{sel} {{{decls} }}")
  }

  #[test]
  fn reports_unknown() {
    let rule = configured(&PropertyNoUnknown, None);
    let node = style_node("a", &[("colr", "red")]);
    let d = rule.check_source(&ctx_with_source(&node));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("colr"));
    // At the property, as Stylelint reports it.
    assert_eq!(d[0].span, Span::new(4, 4));
  }

  #[test]
  fn allows_known() {
    let rule = configured(&PropertyNoUnknown, None);
    let node = style_node("a", &[("color", "red"), ("COLOR", "red")]);
    assert!(rule.check_source(&ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn skips_custom_and_vendor() {
    let rule = configured(&PropertyNoUnknown, None);
    let node = style_node("a", &[("--my-var", "1"), ("-webkit-appearance", "none")]);
    assert!(rule.check_source(&ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn skips_preprocessor_declarations() {
    let rule = configured(&PropertyNoUnknown, None);
    let scss = "a { $v: 1; #{$p}-x: 1; font: { family: x; } }";
    assert!(rule.check_source(&context(scss, Syntax::Scss)).is_empty());
    let less = "a { @v: 1; .m(); }";
    assert!(rule.check_source(&context(less, Syntax::Less)).is_empty());
  }

  #[test]
  fn reads_only_the_declarations_of_style_rules() {
    let rule = configured(&PropertyNoUnknown, None);
    let source = "@keyframes k { to { colr: red; } }\n@font-face { colr: x; }";
    let d = rule.check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span.offset, 20);
  }

  #[test]
  fn ignore_properties_exact() {
    let node = style_node("a", &[("mso-padding-alt", "0")]);
    let opts = serde_json::json!({ "ignoreProperties": ["mso-padding-alt"] });
    let rule = configured(&PropertyNoUnknown, opts.clone());
    assert!(rule.check_source(&ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn ignore_properties_regex_pattern() {
    let node = style_node("a", &[("mso-padding-alt", "0"), ("mso-table-lspace", "0")]);
    let opts = serde_json::json!({ "ignoreProperties": ["/^mso-/"] });
    let rule = configured(&PropertyNoUnknown, opts.clone());
    assert!(rule.check_source(&ctx_with_source(&node)).is_empty());
  }

  #[test]
  fn ignore_properties_regex_does_not_match_other() {
    let node = style_node("a", &[("colr", "red")]);
    let opts = serde_json::json!({ "ignoreProperties": ["/^mso-/"] });
    let rule = configured(&PropertyNoUnknown, opts.clone());
    assert_eq!(rule.check_source(&ctx_with_source(&node)).len(), 1);
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
    let ignored = style_node(":export", &[("colr", "red")]);
    assert!(rule.check_source(&ctx_with_source(&ignored)).is_empty());
    let checked = style_node("a", &[("colr", "red")]);
    assert_eq!(rule.check_source(&ctx_with_source(&checked)).len(), 1);
  }
}
