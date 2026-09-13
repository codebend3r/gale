use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow vendor prefixes in selectors.
///
/// Equivalent to Stylelint's `selector-no-vendor-prefix` rule.
/// Only flags vendor-prefixed selectors that have standard (unprefixed)
/// equivalents — i.e., selectors that autoprefixer can handle.
/// Browser-specific pseudo-elements/classes (like `::-webkit-slider-thumb`)
/// that have no standard equivalent are NOT flagged.
pub struct SelectorNoVendorPrefix;

/// Vendor-prefixed selectors that have standard equivalents and should be
/// flagged. This list matches the `SELECTORS` set from Stylelint's
/// `isAutoprefixable` utility, which is derived from Autoprefixer's data.
const AUTOPREFIXABLE_SELECTORS: &[&str] = &[
  ":-moz-any-link",
  ":-moz-full-screen",
  ":-moz-placeholder",
  ":-moz-placeholder-shown",
  ":-moz-read-only",
  ":-moz-read-write",
  ":-ms-fullscreen",
  ":-ms-input-placeholder",
  ":-webkit-any-link",
  ":-webkit-full-screen",
  "::-moz-placeholder",
  "::-moz-selection",
  "::-ms-input-placeholder",
  "::-webkit-backdrop",
  "::-webkit-input-placeholder",
];

impl Rule for SelectorNoVendorPrefix {
  fn name(&self) -> &'static str {
    "selector-no-vendor-prefix"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes for selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let selector_lower = rule.selector.to_ascii_lowercase();
    let mut diags = Vec::new();

    for &autoprefixable in AUTOPREFIXABLE_SELECTORS {
      if selector_lower.contains(autoprefixable) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Unexpected vendor-prefixed selector \"{}\"",
              rule.selector.trim()
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(rule.span.offset, rule.span.length)),
        );
        break; // one diagnostic per selector is enough
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

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
  fn reports_webkit_input_placeholder() {
    let d =
      SelectorNoVendorPrefix.check(&style_with_selector("::-webkit-input-placeholder"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("-webkit-"));
  }

  #[test]
  fn reports_moz_placeholder() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector("::-moz-placeholder"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_ms_input_placeholder() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector(":-ms-input-placeholder"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_webkit_full_screen() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector(":-webkit-full-screen"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_standard_selector() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector("::placeholder"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_class_selector() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector(".my-class"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_webkit_slider_thumb() {
    // ::-webkit-slider-thumb has no standard equivalent — not autoprefixable.
    let d = SelectorNoVendorPrefix.check(&style_with_selector("&::-webkit-slider-thumb"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_moz_range_thumb() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector("&::-moz-range-thumb"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_webkit_autofill() {
    // :-webkit-autofill has no standard equivalent — not autoprefixable.
    let d = SelectorNoVendorPrefix.check(&style_with_selector("&:-webkit-autofill"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_moz_focusring() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector("&:-moz-focusring"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_webkit_calendar_picker_indicator() {
    let d = SelectorNoVendorPrefix.check(
      &style_with_selector("::-webkit-calendar-picker-indicator"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn allows_moz_focus_inner() {
    let d = SelectorNoVendorPrefix.check(&style_with_selector("::-moz-focus-inner"), &ctx());
    assert!(d.is_empty());
  }
}
