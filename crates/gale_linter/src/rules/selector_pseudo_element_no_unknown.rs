use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::data::is_known_pseudo_element;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-pseudo-element-no-unknown";

declare_rule! {
  pub struct SelectorPseudoElementNoUnknown => SelectorPseudoElementNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown pseudo-element selectors",
    default_severity: Warning,
  }
}

/// `selector-pseudo-element-no-unknown` as configured.
pub struct SelectorPseudoElementNoUnknownCheck {
  /// The `ignorePseudoElements` option: names and `/regex/` entries.
  ignored: Matchers,
}

impl SelectorPseudoElementNoUnknownCheck {
  /// Stylelint's options: nothing but `true`, then `ignorePseudoElements`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignored = options.strings("ignorePseudoElements");
    options.finish()?;
    Ok(Self {
      ignored: Matchers::new(&ignored),
    })
  }
}

impl Check for SelectorPseudoElementNoUnknownCheck {
  /// Flags pseudo-elements that are not standard. Vendor-prefixed names,
  /// and names the `ignorePseudoElements` option lists, are skipped.
  /// Rules that are not standard syntax are skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      for name in extract_pseudo_elements(rule.selector()) {
        if name.starts_with('-') || self.ignored.matches(&name) {
          continue;
        }
        if !is_known_pseudo_element(&name) {
          diags.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected unknown pseudo-element selector \"::{name}\""),
            )
            .span(rule.span()),
          );
        }
      }
    }
    diags
  }
}

/// Extract pseudo-element names from a selector string (::name patterns).
fn extract_pseudo_elements(selector: &str) -> Vec<String> {
  let mut elements = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
      i += 2; // skip ::
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        elements.push(name);
      }
    } else {
      i += 1;
    }
  }

  elements
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_unknown_pseudo_element() {
    let rule = configured(&SelectorPseudoElementNoUnknown, None);
    let d = rule.check_source(&context(&style_with_selector("a::beforre"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("::beforre"));
  }

  #[test]
  fn ignore_pseudo_elements_skips_listed_names() {
    let options = serde_json::json!([true, { "ignorePseudoElements": ["v-deep", "/^my-/"] }]);
    let rule = configured(&SelectorPseudoElementNoUnknown, options.clone());
    assert!(
      rule
        .check_source(&context(&style_with_selector(".a::v-deep"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(&style_with_selector(".a::my-thing"), Syntax::Css))
        .is_empty()
    );
    assert_eq!(
      rule
        .check_source(&context(&style_with_selector(".a::v-global"), Syntax::Css))
        .len(),
      1
    );
  }

  #[test]
  fn allows_known_pseudo_element() {
    let rule = configured(&SelectorPseudoElementNoUnknown, None);
    assert!(
      rule
        .check_source(&context(&style_with_selector("a::before"), Syntax::Css))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&context(
          &style_with_selector("a::placeholder"),
          Syntax::Css
        ))
        .is_empty()
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignorePseudoElements": ["v-deep", "/^my-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"selector-pseudo-element-no-unknown\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignorePseudoClasses": ["x"] }])
      ),
      [
        "Invalid option name \"ignorePseudoClasses\" for rule \"selector-pseudo-element-no-unknown\""
      ]
    );
  }
}
