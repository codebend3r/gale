use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specified media feature names.
///
/// Options: an array of disallowed media feature names.
/// Example: `["max-width", "min-resolution"]`
///
/// Equivalent to Stylelint's `media-feature-name-disallowed-list` rule.
pub struct MediaFeatureNameDisallowedList;

/// Extract media feature names from a media query params string.
fn extract_media_features(params: &str) -> Vec<String> {
  let mut features = Vec::new();
  let chars: Vec<char> = params.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '(' {
      i += 1;
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > start {
        let name = params[start..i].to_string();
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
        if i < len && (chars[i] == ':' || chars[i] == ')') {
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

fn parse_disallowed_list(options: Option<&serde_json::Value>) -> Vec<String> {
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

impl Rule for MediaFeatureNameDisallowedList {
  fn name(&self) -> &'static str {
    "media-feature-name-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed media feature names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let disallowed = parse_disallowed_list(ctx.options);
    if disallowed.is_empty() {
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
      if disallowed.contains(&feature_lower) {
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
    let d = MediaFeatureNameDisallowedList.check(&media_rule("(min-width: 768px)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_disallowed_feature() {
    let opts = json!(["max-width"]);
    let d = MediaFeatureNameDisallowedList
      .check(&media_rule("(min-width: 768px)"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_disallowed_feature() {
    let opts = json!(["min-width"]);
    let d = MediaFeatureNameDisallowedList
      .check(&media_rule("(min-width: 768px)"), &ctx_with_options(&opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("min-width"));
  }

  #[test]
  fn case_insensitive_match() {
    let opts = json!(["MIN-WIDTH"]);
    let d = MediaFeatureNameDisallowedList
      .check(&media_rule("(min-width: 768px)"), &ctx_with_options(&opts));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_non_media_at_rules() {
    let opts = json!(["min-width"]);
    let node = CssNode::AtRule(AtRule {
      name: "supports".to_string(),
      params: "(display: grid)".to_string(),
      span: ParserSpan::new(0, 20),
      children: vec![],
    });
    let d = MediaFeatureNameDisallowedList.check(&node, &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      MediaFeatureNameDisallowedList.name(),
      "media-feature-name-disallowed-list"
    );
  }
}
