use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::is_known_pseudo_element;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
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
  ignored: Vec<String>,
}

impl SelectorPseudoElementNoUnknownCheck {
  /// Stylelint's options: nothing but `true`, then `ignorePseudoElements`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignored = options.strings("ignorePseudoElements");
    options.finish()?;
    Ok(Self { ignored })
  }
}

impl Check for SelectorPseudoElementNoUnknownCheck {
  /// Flags pseudo-elements that are not standard. Vendor-prefixed names,
  /// and names the `ignorePseudoElements` option lists, are skipped.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();
    for name in extract_pseudo_elements(&rule.selector) {
      if name.starts_with('-') || is_ignored(&name, &self.ignored) {
        continue;
      }
      if !is_known_pseudo_element(&name) {
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected unknown pseudo-element selector \"::{name}\""),
          )
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Whether an `ignorePseudoElements` entry, an exact name or a `/regex/`,
/// matches `name`.
fn is_ignored(name: &str, ignored: &[String]) -> bool {
  ignored
    .iter()
    .any(|entry| crate::pattern::match_regex_entry(entry, name).unwrap_or_else(|| entry == name))
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
  use gale_css_parser::{CssNode, Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_unknown_pseudo_element() {
    let rule = configured(&SelectorPseudoElementNoUnknown, None);
    let d = rule.check(&style_with_selector("a::beforre"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("::beforre"));
  }

  #[test]
  fn ignore_pseudo_elements_skips_listed_names() {
    let options = serde_json::json!([true, { "ignorePseudoElements": ["v-deep", "/^my-/"] }]);
    let rule = configured(&SelectorPseudoElementNoUnknown, options.clone());
    let ctx = RuleContext {
      env: Default::default(),
      options: Some(&options),
      ..ctx()
    };
    assert!(
      rule
        .check(&style_with_selector(".a::v-deep"), &ctx)
        .is_empty()
    );
    assert!(
      rule
        .check(&style_with_selector(".a::my-thing"), &ctx)
        .is_empty()
    );
    assert_eq!(
      rule.check(&style_with_selector(".a::v-global"), &ctx).len(),
      1
    );
  }

  #[test]
  fn allows_known_pseudo_element() {
    let rule = configured(&SelectorPseudoElementNoUnknown, None);
    assert!(
      rule
        .check(&style_with_selector("a::before"), &ctx())
        .is_empty()
    );
    assert!(
      rule
        .check(&style_with_selector("a::placeholder"), &ctx())
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
