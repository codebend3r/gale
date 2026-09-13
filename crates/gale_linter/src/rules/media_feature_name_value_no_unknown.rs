use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports unknown values for known media feature names in `@media` rules.
///
/// Equivalent to Stylelint's `media-feature-name-value-no-unknown` rule.
pub struct MediaFeatureNameValueNoUnknown;

/// Returns the set of valid keyword values for a known discrete media feature,
/// or `None` if the feature is numeric/range-based (width, height, etc.) and
/// should be skipped.
fn known_values_for_feature(feature: &str) -> Option<&'static [&'static str]> {
  match feature {
    "prefers-color-scheme" => Some(&["light", "dark"]),
    "prefers-reduced-motion" => Some(&["no-preference", "reduce"]),
    "prefers-contrast" => Some(&["no-preference", "more", "less", "custom"]),
    "orientation" => Some(&["portrait", "landscape"]),
    "scan" => Some(&["interlace", "progressive"]),
    "update" => Some(&["none", "slow", "fast"]),
    "overflow-block" | "overflow-inline" => Some(&["none", "scroll", "optional-paged", "paged"]),
    "pointer" | "any-pointer" => Some(&["none", "coarse", "fine"]),
    "hover" | "any-hover" => Some(&["none", "hover"]),
    "color-gamut" => Some(&["srgb", "p3", "rec2020"]),
    "display-mode" => Some(&[
      "fullscreen",
      "standalone",
      "minimal-ui",
      "browser",
      "window-controls-overlay",
    ]),
    "forced-colors" => Some(&["none", "active"]),
    "inverted-colors" => Some(&["none", "inverted"]),
    "scripting" => Some(&["none", "initial-only", "enabled"]),
    _ => None,
  }
}

/// Extract `(feature-name, value)` pairs from a `@media` params string.
///
/// Looks for patterns like `(feature-name: value)` in the media query params.
fn extract_media_feature_values(params: &str) -> Vec<(String, String)> {
  let mut pairs = Vec::new();
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
      let name_start = i;
      // Collect feature name
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      if i > name_start {
        let name: String = chars[name_start..i].iter().collect();
        // Skip whitespace
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
        // Must be followed by ':'
        if i < len && chars[i] == ':' {
          i += 1;
          // Skip whitespace
          while i < len && chars[i].is_ascii_whitespace() {
            i += 1;
          }
          let value_start = i;
          // Collect value until ')' or whitespace followed by ')'
          while i < len && chars[i] != ')' && chars[i] != '(' {
            i += 1;
          }
          if i > value_start {
            let value: String = chars[value_start..i]
              .iter()
              .collect::<String>()
              .trim()
              .to_string();
            if !value.is_empty() {
              pairs.push((name, value));
            }
          }
        }
      }
    } else {
      i += 1;
    }
  }

  pairs
}

impl Rule for MediaFeatureNameValueNoUnknown {
  fn name(&self) -> &'static str {
    "media-feature-name-value-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown values for known media feature names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "media" {
      return vec![];
    }

    let mut diags = Vec::new();
    for (feature, value) in extract_media_feature_values(&at.params) {
      let feature_lower = feature.to_ascii_lowercase();
      let value_lower = value.to_ascii_lowercase();

      if let Some(valid_values) = known_values_for_feature(&feature_lower)
        && !valid_values.contains(&value_lower.as_str())
      {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown value \"{value}\" for media feature \"{feature}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(at.span.offset, at.span.length)),
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

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_unknown_value_for_prefers_color_scheme() {
    let d = MediaFeatureNameValueNoUnknown.check(&media("(prefers-color-scheme: dimmed)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("dimmed"));
    assert!(d[0].message.contains("prefers-color-scheme"));
  }

  #[test]
  fn allows_valid_prefers_color_scheme() {
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(prefers-color-scheme: dark)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(prefers-color-scheme: light)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_unknown_orientation_value() {
    let d = MediaFeatureNameValueNoUnknown.check(&media("(orientation: diagonal)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("diagonal"));
  }

  #[test]
  fn allows_valid_orientation() {
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(orientation: portrait)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(orientation: landscape)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_numeric_features() {
    // width, height, resolution etc. should not be checked
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(min-width: 768px)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(resolution: 2dppx)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_valid_hover() {
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(hover: hover)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&media("(hover: none)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_unknown_hover_value() {
    let d = MediaFeatureNameValueNoUnknown.check(&media("(hover: yes)"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_non_media_at_rules() {
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "(prefers-color-scheme: bogus)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    assert!(
      MediaFeatureNameValueNoUnknown
        .check(&node, &ctx())
        .is_empty()
    );
  }
}
