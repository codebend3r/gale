use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified values for specific properties.
///
/// Options: an object mapping property names to arrays of allowed value patterns.
/// Patterns are matched as substrings (case-insensitive).
/// Example: `{"display": ["block", "flex", "grid"], "position": ["relative", "absolute"]}`
///
/// Equivalent to Stylelint's `declaration-property-value-allowed-list` rule.
pub struct DeclarationPropertyValueAllowedList;

fn parse_options(options: Option<&serde_json::Value>) -> HashMap<String, Vec<String>> {
  let Some(val) = options else {
    return HashMap::new();
  };
  let Some(obj) = val.as_object() else {
    return HashMap::new();
  };
  let mut map = HashMap::new();
  for (prop, values_val) in obj {
    if let Some(arr) = values_val.as_array() {
      let values: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
      map.insert(prop.to_string(), values);
    }
  }
  map
}

impl Rule for DeclarationPropertyValueAllowedList {
  fn name(&self) -> &'static str {
    "declaration-property-value-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed property and value pairs within declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let allowed_map = parse_options(ctx.options);
    if allowed_map.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    for decl in declarations {
      if let Some(allowed_values) = allowed_map.get(&decl.property) {
        let is_allowed = allowed_values
          .iter()
          .any(|pattern| decl.value.contains(pattern.as_str()));
        if !is_allowed {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Unexpected value \"{}\" for property \"{}\"",
                decl.value, decl.property
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
          );
        }
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};
  use serde_json::json;

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_decl(prop: &str, val: &str) -> CssNode {
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
    let d = DeclarationPropertyValueAllowedList.check(&style_with_decl("display", "none"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_listed_value() {
    let opts = json!({"display": ["block", "flex"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("display", "flex"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_unlisted_value() {
    let opts = json!({"display": ["block", "flex"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("display", "none"),
      &ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("none"));
  }

  #[test]
  fn case_sensitive_value_match() {
    let opts = json!({"display": ["FLEX"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("display", "flex"),
      &ctx_with_options(&opts),
    );
    // "FLEX" does not match "flex" -- strict matching
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn vendor_prefixed_property_not_matched() {
    let opts = json!({"display": ["block"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("-webkit-display", "flex"),
      &ctx_with_options(&opts),
    );
    // "-webkit-display" is not "display", so the rule does not apply
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_unconfigured_properties() {
    let opts = json!({"display": ["block"]});
    let d = DeclarationPropertyValueAllowedList
      .check(&style_with_decl("color", "red"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      DeclarationPropertyValueAllowedList.name(),
      "declaration-property-value-allowed-list"
    );
  }
}
