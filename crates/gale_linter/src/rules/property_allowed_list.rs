use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "property-allowed-list";

declare_rule! {
  /// Only allow specified CSS properties.
  ///
  /// Options: an array of property names that are allowed. All other properties
  /// are flagged.
  ///
  /// Equivalent to Stylelint's `property-allowed-list` rule.
  pub struct PropertyAllowedList => PropertyAllowedListCheck {
    name: NAME,
    description: "Specify a list of allowed properties",
    default_severity: Warning,
    primary_option_array: true,
  }
}

/// `property-allowed-list` as configured.
pub struct PropertyAllowedListCheck {
  /// The properties allowed, or `None` when the config lists none (a preset
  /// enabling the rule), which allows every property.
  allowed: Option<Vec<String>>,
}

impl PropertyAllowedListCheck {
  /// Stylelint's options: a property name or `/regex/`, or a list of them.
  /// Names are compared exactly; `/regex/` entries are not matched yet.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let allowed = options.primary_strings();
    options.finish()?;
    Ok(Self { allowed })
  }
}

impl Check for PropertyAllowedListCheck {
  /// Flags properties outside the configured allow list.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(allowed) = &self.allowed else {
      return vec![];
    };

    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();

    for decl in declarations {
      if !allowed.iter().any(|a| a == &decl.property) {
        diags.push(
          Diagnostic::new(NAME, format!("Unexpected property \"{}\"", decl.property))
            .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options, lint};

  fn style_with_prop(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = configured(&PropertyAllowedList, None).check(&style_with_prop("color", "red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_listed_property() {
    let d = configured(&PropertyAllowedList, serde_json::json!(["color", "margin"]))
      .check(&style_with_prop("color", "red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn flags_unlisted_property() {
    let d = configured(&PropertyAllowedList, serde_json::json!(["color"]))
      .check(&style_with_prop("margin", "0"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("margin"));
  }

  #[test]
  fn case_sensitive() {
    let d = configured(&PropertyAllowedList, serde_json::json!(["color"]))
      .check(&style_with_prop("Color", "red"), &ctx());
    // "color" does not match "Color" -- strict matching
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn vendor_prefixed_not_matched_by_unprefixed() {
    let d = configured(&PropertyAllowedList, serde_json::json!(["transform"])).check(
      &style_with_prop("-webkit-transform", "rotate(45deg)"),
      &ctx(),
    );
    // "-webkit-transform" is not "transform" -- strict matching
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(PropertyAllowedList::META.name, "property-allowed-list");
    assert!(PropertyAllowedList::META.primary_option_array);
  }

  #[test]
  fn a_list_followed_by_secondary_options_is_still_the_list() {
    let options = serde_json::json!([["color", "margin"], { "severity": "warning" }]);
    let found = lint(
      NAME,
      options,
      "a { color: red; margin: 0; padding: 0; }",
      gale_css_parser::Syntax::Css,
    );
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("padding"));
  }

  #[test]
  fn a_single_name_is_a_list_of_one() {
    let found = lint(
      NAME,
      serde_json::json!("color"),
      "a { color: red; margin: 0; }",
      gale_css_parser::Syntax::Css,
    );
    assert_eq!(found.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"property-allowed-list\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["color", true])),
      ["Invalid option value \"true\" for rule \"property-allowed-list\""]
    );
    assert!(invalid_options(NAME, serde_json::json!(["color", "/^margin/"])).is_empty());
  }
}
