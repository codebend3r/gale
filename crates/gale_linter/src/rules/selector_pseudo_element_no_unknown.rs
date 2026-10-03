use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::data::is_known_pseudo_element;
use crate::rule::{Rule, RuleContext};

pub struct SelectorPseudoElementNoUnknown;

impl Rule for SelectorPseudoElementNoUnknown {
  fn name(&self) -> &'static str {
    "selector-pseudo-element-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown pseudo-element selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags pseudo-elements that are not standard. Vendor-prefixed names,
  /// and names the `ignorePseudoElements` option lists, are skipped.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let ignored: Vec<&str> = ctx
      .secondary_options()
      .and_then(|s| s.get("ignorePseudoElements"))
      .and_then(|v| v.as_array())
      .map(|names| names.iter().filter_map(|v| v.as_str()).collect())
      .unwrap_or_default();
    let mut diags = Vec::new();
    for name in extract_pseudo_elements(&rule.selector) {
      if name.starts_with('-') || is_ignored(&name, &ignored) {
        continue;
      }
      if !is_known_pseudo_element(&name) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown pseudo-element selector \"::{name}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Whether an `ignorePseudoElements` entry, an exact name or a `/regex/`,
/// matches `name`.
fn is_ignored(name: &str, ignored: &[&str]) -> bool {
  ignored
    .iter()
    .any(|entry| crate::pattern::match_regex_entry(entry, name).unwrap_or_else(|| *entry == name))
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

  use crate::testing::ctx;

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
    let d = SelectorPseudoElementNoUnknown.check(&style_with_selector("a::beforre"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("::beforre"));
  }

  #[test]
  fn ignore_pseudo_elements_skips_listed_names() {
    let options = serde_json::json!([true, { "ignorePseudoElements": ["v-deep", "/^my-/"] }]);
    let ctx = RuleContext {
      options: Some(&options),
      ..ctx()
    };
    let rule = SelectorPseudoElementNoUnknown;
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
    assert!(
      SelectorPseudoElementNoUnknown
        .check(&style_with_selector("a::before"), &ctx())
        .is_empty()
    );
    assert!(
      SelectorPseudoElementNoUnknown
        .check(&style_with_selector("a::placeholder"), &ctx())
        .is_empty()
    );
  }
}
