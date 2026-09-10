use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified values for specific media feature names.
///
/// Options: an object mapping media feature names to arrays of allowed
/// values or regex patterns (strings starting and ending with `/`).
///
/// Example config:
/// ```json
/// { "width": ["100px", "200px"], "resolution": ["/^[0-9]+dpi$/"] }
/// ```
///
/// Equivalent to Stylelint's `media-feature-name-value-allowed-list` rule.
pub struct MediaFeatureNameValueAllowedList;

impl Rule for MediaFeatureNameValueAllowedList {
  fn name(&self) -> &'static str {
    "media-feature-name-value-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed values for specific media feature names"
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

    for (feature, value) in extract_media_features(&at_rule.params) {
      if let Some(allowed_values) = allowed_map.get(&feature) {
        let value_trimmed = value.trim();
        let is_allowed = allowed_values.iter().any(|pattern| {
          if let Some(regex_body) = pattern.strip_prefix('/').and_then(|s| s.strip_suffix('/')) {
            // Treat as regex pattern.
            regex::Regex::new(regex_body)
              .map(|re| re.is_match(value_trimmed))
              .unwrap_or(false)
          } else {
            // Exact match (strict).
            pattern == value_trimmed
          }
        });

        if !is_allowed {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!("Unexpected value \"{value_trimmed}\" for media feature \"{feature}\""),
            )
            .severity(self.default_severity())
            .span(Span::new(at_rule.span.offset, at_rule.span.length)),
          );
        }
      }
    }

    diags
  }
}

/// Parse the rule options as `{ "feature-name": ["value1", "/regex/"] }`.
fn parse_options(options: Option<&serde_json::Value>) -> Option<HashMap<String, Vec<String>>> {
  let obj = options?.as_object()?;
  let mut map = HashMap::new();
  for (key, val) in obj {
    if let Some(arr) = val.as_array() {
      let values: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
      map.insert(key.to_string(), values);
    }
  }
  Some(map)
}

/// Extract `(feature_name, value)` pairs from a media query params string.
fn extract_media_features(params: &str) -> Vec<(String, String)> {
  let mut features = Vec::new();
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
  fn reports_disallowed_value() {
    let opts = serde_json::json!({ "width": ["100px", "200px"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = media_at_rule("(width: 50px)");
    let d = MediaFeatureNameValueAllowedList.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("50px"));
  }

  #[test]
  fn allows_permitted_value() {
    let opts = serde_json::json!({ "width": ["100px", "200px"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = media_at_rule("(width: 100px)");
    let d = MediaFeatureNameValueAllowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn min_max_prefix_not_stripped() {
    let opts = serde_json::json!({ "width": ["100px"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    // "min-width" does not match "width" -- strict matching, no constraint = no diagnostic
    let node = media_at_rule("(min-width: 100px)");
    let d = MediaFeatureNameValueAllowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn supports_regex_patterns() {
    let opts = serde_json::json!({ "resolution": ["/^[0-9]+dpi$/"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };

    let node_ok = media_at_rule("(resolution: 300dpi)");
    let d = MediaFeatureNameValueAllowedList.check(&node_ok, &ctx);
    assert!(d.is_empty());

    let node_bad = media_at_rule("(resolution: 2x)");
    let d = MediaFeatureNameValueAllowedList.check(&node_bad, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("2x"));
  }

  #[test]
  fn returns_empty_when_no_options() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let node = media_at_rule("(min-width: 50px)");
    let d = MediaFeatureNameValueAllowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }
}
