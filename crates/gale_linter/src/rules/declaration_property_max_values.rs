use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of values in a declaration for specific properties.
///
/// Options: an object mapping property names (or patterns) to maximum
/// allowed value counts.
/// Example: `{"border": 2, "/^margin/": 1}`
///
/// v17: Properties are checked as-is (no vendor prefix stripping).
/// E.g. `{ "border": 2 }` does NOT match `-webkit-border`.
///
/// Equivalent to Stylelint's `declaration-property-max-values` rule.
pub struct DeclarationPropertyMaxValues;

impl Rule for DeclarationPropertyMaxValues {
  fn name(&self) -> &'static str {
    "declaration-property-max-values"
  }

  fn description(&self) -> &'static str {
    "Limit the number of values for properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let limits = parse_options(ctx.primary_option());

    if limits.is_empty() {
      return vec![];
    }

    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();
    for decl in decls {
      let prop_lower = decl.property.to_ascii_lowercase();

      // v17: check property as-is, no vendor prefix stripping
      let max = find_matching_limit(&prop_lower, &limits);
      let Some(max_values) = max else {
        continue;
      };

      let value_count = count_values(&decl.value);
      if value_count > max_values {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Expected no more than {max_values} value(s) for property \
                             \"{}\", found {value_count}",
              decl.property,
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }
    diags
  }
}

/// Parse the primary option into a map of property patterns to max values.
fn parse_options(options: Option<&serde_json::Value>) -> HashMap<String, usize> {
  let Some(val) = options else {
    return HashMap::new();
  };
  let Some(obj) = val.as_object() else {
    return HashMap::new();
  };
  let mut map = HashMap::new();
  for (prop, max_val) in obj {
    if let Some(n) = max_val.as_u64() {
      map.insert(prop.to_ascii_lowercase(), n as usize);
    } else if let Some(n) = max_val.as_i64() {
      map.insert(prop.to_ascii_lowercase(), n.max(0) as usize);
    }
  }
  map
}

/// Find the matching limit for a property, checking exact matches and regex patterns.
fn find_matching_limit(prop: &str, limits: &HashMap<String, usize>) -> Option<usize> {
  // Exact match first
  if let Some(&max) = limits.get(prop) {
    return Some(max);
  }
  // Check regex patterns (keys starting and ending with `/`)
  for (pattern, &max) in limits {
    if pattern.starts_with('/') && pattern.ends_with('/') {
      let re_str = &pattern[1..pattern.len() - 1];
      if let Ok(re) = regex::Regex::new(re_str) {
        if re.is_match(prop) {
          return Some(max);
        }
      }
    }
  }
  None
}

/// Count the number of space-separated values in a CSS value string.
/// Respects parentheses (function arguments are not counted as separate values).
fn count_values(value: &str) -> usize {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return 0;
  }

  let mut count = 0;
  let mut depth: usize = 0;
  let mut in_value = false;

  for ch in trimmed.chars() {
    match ch {
      '(' => {
        depth += 1;
        in_value = true;
      }
      ')' => {
        depth = depth.saturating_sub(1);
      }
      c if c.is_ascii_whitespace() && depth == 0 => {
        if in_value {
          count += 1;
          in_value = false;
        }
      }
      _ => {
        in_value = true;
      }
    }
  }
  if in_value {
    count += 1;
  }
  count
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_options(options: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(options),
    }
  }

  fn style_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, prop.len() + val.len() + 2),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn flags_excess_values() {
    let opts = serde_json::json!({"border": 2});
    let d = DeclarationPropertyMaxValues.check(
      &style_decl("border", "1px solid red"),
      &ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("no more than 2"));
  }

  #[test]
  fn allows_within_limit() {
    let opts = serde_json::json!({"border": 3});
    let d = DeclarationPropertyMaxValues.check(
      &style_decl("border", "1px solid red"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_match_vendor_prefixed_property() {
    // v17: "border" limit should NOT match "-webkit-border"
    let opts = serde_json::json!({"border": 1});
    let d = DeclarationPropertyMaxValues.check(
      &style_decl("-webkit-border", "1px solid red"),
      &ctx_with_options(&opts),
    );
    assert!(
      d.is_empty(),
      "v17: property limit should not match vendor-prefixed equivalent"
    );
  }

  #[test]
  fn matches_vendor_prefixed_when_explicitly_specified() {
    let opts = serde_json::json!({"-webkit-border": 1});
    let d = DeclarationPropertyMaxValues.check(
      &style_decl("-webkit-border", "1px solid red"),
      &ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn function_args_count_as_one_value() {
    let opts = serde_json::json!({"background": 1});
    let d = DeclarationPropertyMaxValues.check(
      &style_decl("background", "rgb(0, 0, 0)"),
      &ctx_with_options(&opts),
    );
    assert!(
      d.is_empty(),
      "function args should not be counted as separate values"
    );
  }

  #[test]
  fn count_values_basic() {
    assert_eq!(count_values("1px solid red"), 3);
    assert_eq!(count_values("1px"), 1);
    assert_eq!(count_values("rgb(0, 0, 0) 1px"), 2);
    assert_eq!(count_values(""), 0);
  }
}
