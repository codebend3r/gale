use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified units for specific media feature names.
///
/// Options: an object mapping media feature names to arrays of allowed units.
///
/// Example config:
/// ```json
/// { "width": ["em", "rem"], "height": ["px"] }
/// ```
///
/// Equivalent to Stylelint's `media-feature-name-unit-allowed-list` rule.
pub struct MediaFeatureNameUnitAllowedList;

impl Rule for MediaFeatureNameUnitAllowedList {
  fn name(&self) -> &'static str {
    "media-feature-name-unit-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed units for specific media feature names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };

    if !at_rule.name.eq_ignore_ascii_case("media") {
      return vec![];
    }

    let allowed_map = match parse_options(ctx.options) {
      Some(m) => m,
      None => return vec![],
    };

    let mut diags = Vec::new();

    // Parse media features from params, e.g. "(min-width: 100px) and (height: 50em)"
    for (feature, value) in extract_media_features(&at_rule.params) {
      let feature_lower = feature.to_ascii_lowercase();
      // Strip min-/max- prefix for matching.
      let base_feature = feature_lower
        .strip_prefix("min-")
        .or_else(|| feature_lower.strip_prefix("max-"))
        .unwrap_or(&feature_lower);

      if let Some(allowed_units) = allowed_map.get(base_feature) {
        for unit in extract_units(&value) {
          let unit_lower = unit.to_ascii_lowercase();
          if !allowed_units.contains(&unit_lower) {
            diags.push(
              Diagnostic::new(
                self.name(),
                format!("Unexpected unit \"{unit}\" for media feature \"{feature}\""),
              )
              .severity(self.default_severity())
              .span(Span::new(at_rule.span.offset, at_rule.span.length)),
            );
          }
        }
      }
    }

    diags
  }
}

/// Parse the rule options as `{ "feature-name": ["unit1", "unit2"] }`.
fn parse_options(options: Option<&serde_json::Value>) -> Option<HashMap<String, Vec<String>>> {
  let obj = options?.as_object()?;
  let mut map = HashMap::new();
  for (key, val) in obj {
    if let Some(arr) = val.as_array() {
      let units: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
        .collect();
      map.insert(key.to_ascii_lowercase(), units);
    }
  }
  Some(map)
}

/// Extract `(feature_name, value)` pairs from a media query params string.
/// Handles expressions like `(min-width: 100px)`, `(width: 50em)`.
fn extract_media_features(params: &str) -> Vec<(String, String)> {
  let mut features = Vec::new();
  // Simple parser: find parenthesized expressions containing a colon.
  let mut i = 0;
  let chars: Vec<char> = params.chars().collect();
  let len = chars.len();

  while i < len {
    if chars[i] == '(' {
      let start = i + 1;
      let mut depth = 1;
      i += 1;
      while i < len && depth > 0 {
        if chars[i] == '(' {
          depth += 1;
        } else if chars[i] == ')' {
          depth -= 1;
        }
        i += 1;
      }
      let end = if depth == 0 { i - 1 } else { i };
      let content: String = chars[start..end].iter().collect();
      if let Some(colon_pos) = content.find(':') {
        let feature = content[..colon_pos].trim().to_string();
        let value = content[colon_pos + 1..].trim().to_string();
        features.push((feature, value));
      }
    } else {
      i += 1;
    }
  }

  features
}

/// Extract units from a CSS value string. For example, `"100px"` -> `["px"]`,
/// `"calc(10em + 5px)"` -> `["em", "px"]`.
fn extract_units(value: &str) -> Vec<String> {
  let mut units = Vec::new();
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    // Skip non-digit characters until we find a number.
    if chars[i].is_ascii_digit() || chars[i] == '.' {
      // Skip past the number.
      while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
        i += 1;
      }
      // Collect the unit (alphabetic characters following the number).
      let unit_start = i;
      while i < len && chars[i].is_ascii_alphabetic() || (i < len && chars[i] == '%') {
        i += 1;
      }
      if i > unit_start {
        let unit: String = chars[unit_start..i].iter().collect();
        units.push(unit);
      }
    } else {
      i += 1;
    }
  }

  units
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn media_at_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    })
  }

  #[test]
  fn reports_disallowed_unit() {
    let opts = serde_json::json!({ "width": ["em", "rem"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = media_at_rule("(min-width: 100px)");
    let d = MediaFeatureNameUnitAllowedList.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("px"));
    assert!(d[0].message.contains("min-width"));
  }

  #[test]
  fn allows_permitted_unit() {
    let opts = serde_json::json!({ "width": ["em", "rem"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = media_at_rule("(min-width: 100em)");
    let d = MediaFeatureNameUnitAllowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_unmatched_feature() {
    let opts = serde_json::json!({ "width": ["em"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = media_at_rule("(color: 8)");
    let d = MediaFeatureNameUnitAllowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn returns_empty_when_no_options() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let node = media_at_rule("(min-width: 100px)");
    let d = MediaFeatureNameUnitAllowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_media_at_rules() {
    let opts = serde_json::json!({ "width": ["em"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = CssNode::AtRule(AtRule {
      name: "supports".to_string(),
      params: "(min-width: 100px)".to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    });
    let d = MediaFeatureNameUnitAllowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }
}
