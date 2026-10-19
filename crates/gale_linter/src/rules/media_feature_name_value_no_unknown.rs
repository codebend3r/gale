use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::list_rules::{Entries, KeyedList, keyed_option};
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "media-feature-name-value-no-unknown";

declare_rule! {
  /// Reports unknown values for known media feature names in `@media` rules.
  ///
  /// Equivalent to Stylelint's `media-feature-name-value-no-unknown` rule.
  pub struct MediaFeatureNameValueNoUnknown => MediaFeatureNameValueNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown values for known media feature names",
    default_severity: Error,
  }
}

/// `media-feature-name-value-no-unknown` as configured.
pub struct MediaFeatureNameValueNoUnknownCheck {
  /// `ignoreMediaFeatureNameValues`: media feature names (or `/regex/`
  /// entries), each with the values (or `/regex/` entries) it may take.
  ignore_media_feature_name_values: KeyedList,
}

impl MediaFeatureNameValueNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreMediaFeatureNameValues`, an
  /// object from names to a value or a list of them.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_media_feature_name_values = keyed_option(
      &mut options,
      "ignoreMediaFeatureNameValues",
      Entries::Patterns,
    );
    options.finish()?;
    Ok(Self {
      ignore_media_feature_name_values,
    })
  }

  /// Stylelint's `optionsMatchesEntry`: whether the first entry of
  /// `ignoreMediaFeatureNameValues` whose name matches `feature` lists
  /// `value`.
  fn is_ignored(&self, feature: &str, value: &str) -> bool {
    self
      .ignore_media_feature_name_values
      .first_matches(feature, value)
  }
}

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

impl Check for MediaFeatureNameValueNoUnknownCheck {
  /// Flags a value the media feature does not accept, for the features this rule
  /// has a keyword table for.
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
        && !self.is_ignored(&feature_lower, &value_lower)
      {
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected unknown value \"{value}\" for media feature \"{feature}\""),
          )
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
  use gale_css_parser::{AtRule, Span as ParserSpan};

  use crate::testing::{configured, ctx};

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
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    let d = rule.check(&media("(prefers-color-scheme: dimmed)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("dimmed"));
    assert!(d[0].message.contains("prefers-color-scheme"));
  }

  #[test]
  fn allows_valid_prefers_color_scheme() {
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    assert!(
      rule
        .check(&media("(prefers-color-scheme: dark)"), &ctx())
        .is_empty()
    );
    assert!(
      rule
        .check(&media("(prefers-color-scheme: light)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_unknown_orientation_value() {
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    let d = rule.check(&media("(orientation: diagonal)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("diagonal"));
  }

  #[test]
  fn allows_valid_orientation() {
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    assert!(
      rule
        .check(&media("(orientation: portrait)"), &ctx())
        .is_empty()
    );
    assert!(
      rule
        .check(&media("(orientation: landscape)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_numeric_features() {
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    // width, height, resolution etc. should not be checked
    assert!(rule.check(&media("(min-width: 768px)"), &ctx()).is_empty());
    assert!(rule.check(&media("(resolution: 2dppx)"), &ctx()).is_empty());
  }

  #[test]
  fn allows_valid_hover() {
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    assert!(rule.check(&media("(hover: hover)"), &ctx()).is_empty());
    assert!(rule.check(&media("(hover: none)"), &ctx()).is_empty());
  }

  #[test]
  fn reports_unknown_hover_value() {
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    let d = rule.check(&media("(hover: yes)"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_non_media_at_rules() {
    let rule = configured(&MediaFeatureNameValueNoUnknown, None);
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "(prefers-color-scheme: bogus)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    let valid = serde_json::json!([true, {
      "ignoreMediaFeatureNameValues": { "prefers-color-scheme": ["dimmed", "/^x-/"], "/^hov/": "yes" }
    }]);
    assert!(invalid_options(NAME, valid).is_empty());
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreMediaFeatureNameValues": ["dimmed"] }])
      ),
      [
        "Invalid value \"dimmed\" for option \"ignoreMediaFeatureNameValues\" of rule \"media-feature-name-value-no-unknown\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"media-feature-name-value-no-unknown\""]
    );
  }

  #[test]
  fn ignore_media_feature_name_values_skips_listed_values() {
    let options = serde_json::json!([true, {
      "ignoreMediaFeatureNameValues": { "prefers-color-scheme": ["dimmed"], "/^hov/": "/^y/" }
    }]);
    let rule = configured(&MediaFeatureNameValueNoUnknown, options);
    assert!(
      rule
        .check(&media("(prefers-color-scheme: dimmed)"), &ctx())
        .is_empty()
    );
    assert!(rule.check(&media("(hover: yes)"), &ctx()).is_empty());
    assert_eq!(
      rule
        .check(&media("(prefers-color-scheme: dim)"), &ctx())
        .len(),
      1
    );
    assert_eq!(
      rule.check(&media("(orientation: diagonal)"), &ctx()).len(),
      1
    );
  }
}
