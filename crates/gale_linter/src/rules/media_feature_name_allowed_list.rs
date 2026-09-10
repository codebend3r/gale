use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Only allow specified media feature names.
///
/// Options: an array of allowed media feature names.
/// Example: `["width", "height", "color"]`
///
/// Equivalent to Stylelint's `media-feature-name-allowed-list` rule.
pub struct MediaFeatureNameAllowedList;

/// Extract media feature names from a media query params string.
/// Looks for patterns like `(feature-name:` or `(feature-name)`.
fn extract_media_features(params: &str) -> Vec<String> {
  let mut features = Vec::new();
  let chars: Vec<char> = params.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '(' {
      i += 1;
      // Skip whitespace
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      // Collect the feature name
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name = params[start..i].to_string();
        // Skip whitespace after name
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
        // Confirm it's followed by ':' or ')' (it's a feature, not a keyword like "not")
        if i < len && (chars[i] == ':' || chars[i] == ')') {
          // Exclude logical keywords
          let lower = name.to_ascii_lowercase();
          if lower != "not" && lower != "and" && lower != "or" && lower != "only" {
            features.push(name);
          }
        }
      }
    } else {
      i += 1;
    }
  }

  features
}

fn parse_allowed_list(options: Option<&serde_json::Value>) -> Vec<String> {
  let Some(val) = options else {
    return Vec::new();
  };
  let Some(arr) = val.as_array() else {
    return Vec::new();
  };
  arr
    .iter()
    .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
    .collect()
}

impl Rule for MediaFeatureNameAllowedList {
  fn name(&self) -> &'static str {
    "media-feature-name-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed media feature names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let allowed = parse_allowed_list(ctx.options);
    if allowed.is_empty() {
      return vec![];
    }

    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };
    if !at_rule.name.eq_ignore_ascii_case("media") {
      return vec![];
    }

    let mut diags = Vec::new();
    let features = extract_media_features(&at_rule.params);

    for feature in features {
      let feature_lower = feature.to_ascii_lowercase();
      if !allowed.contains(&feature_lower) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected media feature name \"{feature}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(at_rule.span.offset, at_rule.span.length)),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};
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

  fn media_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    })
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = MediaFeatureNameAllowedList.check(&media_rule("(min-width: 768px)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_listed_feature() {
    let opts = json!(["min-width", "max-width"]);
    let d = MediaFeatureNameAllowedList
      .check(&media_rule("(min-width: 768px)"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_unlisted_feature() {
    let opts = json!(["min-width"]);
    let d = MediaFeatureNameAllowedList.check(&media_rule("(color)"), &ctx_with_options(&opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color"));
  }

  #[test]
  fn case_insensitive_feature_match() {
    let opts = json!(["MIN-WIDTH"]);
    let d = MediaFeatureNameAllowedList
      .check(&media_rule("(min-width: 768px)"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_media_at_rules() {
    let opts = json!(["min-width"]);
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "slide".to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    });
    let d = MediaFeatureNameAllowedList.check(&node, &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      MediaFeatureNameAllowedList.name(),
      "media-feature-name-allowed-list"
    );
  }

  #[test]
  fn extract_features_works() {
    let features = extract_media_features("(min-width: 768px) and (color)");
    assert_eq!(features.len(), 2);
    assert_eq!(features[0], "min-width");
    assert_eq!(features[1], "color");
  }
}
