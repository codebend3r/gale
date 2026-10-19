use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting, is_truthy};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "order/properties-alphabetical-order";

declare_rule! {
  /// Enforce alphabetical ordering of properties within declaration blocks.
  ///
  /// Equivalent to stylelint-order's `order/properties-alphabetical-order` rule.
  pub struct OrderPropertiesAlphabeticalOrder => OrderPropertiesAlphabeticalOrderCheck {
    name: NAME,
    description: "Require properties within declaration blocks to be in alphabetical order",
    default_severity: Warning,
  }
}

/// `order/properties-alphabetical-order`, which takes no options.
pub struct OrderPropertiesAlphabeticalOrderCheck;

impl OrderPropertiesAlphabeticalOrderCheck {
  /// stylelint-order validates the primary option with `possible: Boolean`,
  /// a function: any truthy value is accepted.  It validates no secondary
  /// options.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_test(is_truthy);
    options.finish()?;
    Ok(Self)
  }
}

/// Strip vendor prefix from a property name for sort comparison.
///
/// E.g. `-webkit-transform` -> `transform`, `-moz-appearance` -> `appearance`.
fn strip_vendor_prefix(prop: &str) -> &str {
  if prop.starts_with('-') {
    // Find the second '-' after the vendor prefix
    if let Some(idx) = prop[1..].find('-') {
      return &prop[idx + 2..];
    }
  }
  prop
}

impl Check for OrderPropertiesAlphabeticalOrderCheck {
  /// Flags a property that sorts before the one declared above it. SCSS variables
  /// are skipped.
  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diagnostics = Vec::new();
    let mut prev_sort_key: Option<String> = None;
    let mut prev_original: Option<String> = None;

    for decl in &rule.declarations {
      let prop = &decl.property;

      // Skip SCSS variables ($var)
      if prop.starts_with('$') {
        continue;
      }

      // Skip custom properties (--var)
      if prop.starts_with("--") {
        continue;
      }

      // Strip vendor prefix and lowercase for comparison
      let unprefixed = strip_vendor_prefix(prop);
      let sort_key = unprefixed.to_ascii_lowercase();

      if let Some(ref prev) = prev_sort_key
        && sort_key < *prev
      {
        diagnostics.push(
          Diagnostic::new(
            NAME,
            format!(
              "Expected \"{prop}\" to come before \"{}\"",
              prev_original.as_deref().unwrap_or(prev)
            ),
          )
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }

      prev_sort_key = Some(sort_key);
      prev_original = Some(prop.clone());
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx};

  fn make_decl(property: &str, value: &str, offset: usize, length: usize) -> Declaration {
    Declaration {
      property: property.to_string(),
      value: value.to_string(),
      span: ParserSpan::new(offset, length),
      important: false,
    }
  }

  #[test]
  fn accepts_alphabetical_order() {
    let rule = configured(&OrderPropertiesAlphabeticalOrder, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("color", "red", 4, 10),
        make_decl("display", "block", 15, 14),
        make_decl("font-size", "12px", 30, 15),
      ],
      span: ParserSpan::new(0, 50),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn reports_non_alphabetical_order() {
    let rule = configured(&OrderPropertiesAlphabeticalOrder, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("color", "red", 19, 10),
      ],
      span: ParserSpan::new(0, 35),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("color"));
    assert!(diags[0].message.contains("display"));
  }

  #[test]
  fn skips_scss_variables() {
    let rule = configured(&OrderPropertiesAlphabeticalOrder, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("$my-var", "10px", 19, 14),
        make_decl("color", "red", 34, 10),
      ],
      span: ParserSpan::new(0, 50),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    // "color" comes after "display" which is out of order, but $my-var is skipped
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn case_insensitive_comparison() {
    let rule = configured(&OrderPropertiesAlphabeticalOrder, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("Color", "red", 4, 10),
        make_decl("display", "block", 15, 14),
      ],
      span: ParserSpan::new(0, 35),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn skips_custom_properties() {
    let rule = configured(&OrderPropertiesAlphabeticalOrder, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("--my-var", "10px", 19, 16),
        make_decl("font-size", "12px", 36, 15),
      ],
      span: ParserSpan::new(0, 55),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn vendor_prefixed_sorts_as_unprefixed() {
    let rule = configured(&OrderPropertiesAlphabeticalOrder, None);
    // -webkit-transform should sort as "transform", which comes after "display"
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("-webkit-transform", "scale(1)", 19, 28),
        make_decl("transform", "scale(1)", 48, 21),
      ],
      span: ParserSpan::new(0, 70),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn vendor_prefixed_out_of_order() {
    let rule = configured(&OrderPropertiesAlphabeticalOrder, None);
    // -webkit-transform sorts as "transform", which should come after "appearance"
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("-webkit-transform", "scale(1)", 4, 28),
        make_decl("appearance", "none", 33, 17),
      ],
      span: ParserSpan::new(0, 55),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("appearance"));
  }

  #[test]
  fn strip_vendor_prefix_works() {
    assert_eq!(strip_vendor_prefix("-webkit-transform"), "transform");
    assert_eq!(strip_vendor_prefix("-moz-appearance"), "appearance");
    assert_eq!(strip_vendor_prefix("-ms-flex"), "flex");
    assert_eq!(strip_vendor_prefix("transform"), "transform");
  }

  #[test]
  fn options_are_validated_like_stylelint_order() {
    use crate::testing::invalid_options;
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Any truthy value passes `Boolean`, and secondary options are ignored.
    assert!(invalid_options(NAME, serde_json::json!(["yes", { "foo": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(0)),
      ["Invalid option \"0\" for rule \"order/properties-alphabetical-order\""]
    );
  }
}
