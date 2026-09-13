use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specified units for specific properties.
///
/// Options: an object mapping property names to arrays of disallowed units.
/// Example: `{"font-size": ["px"], "margin": ["em"]}`
///
/// Equivalent to Stylelint's `declaration-property-unit-disallowed-list` rule.
pub struct DeclarationPropertyUnitDisallowedList;

/// Extract all units from a CSS value string.
fn extract_units(value: &str) -> Vec<String> {
  let mut units = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if bytes[i].is_ascii_digit()
      || (bytes[i] == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit())
    {
      while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
      }
      let unit_start = i;
      while i < len && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'%') {
        i += 1;
      }
      if i > unit_start {
        units.push(value[unit_start..i].to_string());
      }
    } else {
      i += 1;
    }
  }

  units
}

fn parse_options(options: Option<&serde_json::Value>) -> HashMap<String, Vec<String>> {
  let Some(val) = options else {
    return HashMap::new();
  };
  let Some(obj) = val.as_object() else {
    return HashMap::new();
  };
  let mut map = HashMap::new();
  for (prop, units_val) in obj {
    if let Some(arr) = units_val.as_array() {
      let units: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
      map.insert(prop.to_string(), units);
    }
  }
  map
}

impl Rule for DeclarationPropertyUnitDisallowedList {
  fn name(&self) -> &'static str {
    "declaration-property-unit-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed property and unit pairs within declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let disallowed_map = parse_options(ctx.options);
    if disallowed_map.is_empty() {
      return vec![];
    }

    let mut diags = Vec::new();
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    for decl in declarations {
      if let Some(disallowed_units) = disallowed_map.get(&decl.property) {
        let units = extract_units(&decl.value);
        for unit in units {
          if disallowed_units.contains(&unit) {
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Unexpected unit \"{unit}\" for property \"{}\"",
                  decl.property
                ),
              )
              .severity(self.default_severity())
              .span(Span::new(decl.span.offset, decl.span.length)),
            );
          }
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
    let d =
      DeclarationPropertyUnitDisallowedList.check(&style_with_decl("font-size", "16px"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_disallowed_unit() {
    let opts = json!({"font-size": ["px"]});
    let d = DeclarationPropertyUnitDisallowedList.check(
      &style_with_decl("font-size", "1.5rem"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_disallowed_unit() {
    let opts = json!({"font-size": ["px"]});
    let d = DeclarationPropertyUnitDisallowedList.check(
      &style_with_decl("font-size", "16px"),
      &ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("px"));
  }

  #[test]
  fn case_sensitive_unit_match() {
    let opts = json!({"font-size": ["PX"]});
    let d = DeclarationPropertyUnitDisallowedList.check(
      &style_with_decl("font-size", "16px"),
      &ctx_with_options(&opts),
    );
    // "PX" does not match "px" -- strict matching
    assert!(d.is_empty());
  }

  #[test]
  fn vendor_prefixed_property_not_matched() {
    let opts = json!({"font-size": ["px"]});
    let d = DeclarationPropertyUnitDisallowedList.check(
      &style_with_decl("-webkit-font-size", "16px"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_unconfigured_properties() {
    let opts = json!({"font-size": ["px"]});
    let d = DeclarationPropertyUnitDisallowedList
      .check(&style_with_decl("margin", "10px"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      DeclarationPropertyUnitDisallowedList.name(),
      "declaration-property-unit-disallowed-list"
    );
  }
}
