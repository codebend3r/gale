use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

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
  /// has a keyword table for, in every `@media` (any case, nested or not).
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.name().eq_ignore_ascii_case("media") {
        continue;
      }
      // The params as Stylelint's `getAtRuleParams` reads them.
      for (feature, value) in extract_media_feature_values(&at.raw_params()) {
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
            .span(at.span()),
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
  use gale_css_parser::Syntax;
  use serde_json::{Value, json};

  use crate::testing::lint;

  /// The reports for `@media <params> {}` in CSS.
  fn media(params: &str) -> Vec<Diagnostic> {
    media_with(json!(true), params)
  }

  fn media_with(options: Value, params: &str) -> Vec<Diagnostic> {
    lint(NAME, options, &format!("@media {params} {{}}"), Syntax::Css)
  }

  #[test]
  fn reports_unknown_value_for_prefers_color_scheme() {
    let d = media("(prefers-color-scheme: dimmed)");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("dimmed"));
    assert!(d[0].message.contains("prefers-color-scheme"));
    let d = lint(
      NAME,
      json!(true),
      "a { @MEDIA (prefers-color-scheme: dimmed) { b: c } }",
      Syntax::Scss,
    );
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_valid_prefers_color_scheme() {
    assert!(media("(prefers-color-scheme: dark)").is_empty());
    assert!(media("(prefers-color-scheme: light)").is_empty());
  }

  #[test]
  fn reports_unknown_orientation_value() {
    let d = media("(orientation: diagonal)");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("diagonal"));
  }

  #[test]
  fn allows_valid_orientation() {
    assert!(media("(orientation: portrait)").is_empty());
    assert!(media("(orientation: landscape)").is_empty());
  }

  #[test]
  fn skips_numeric_features() {
    // width, height, resolution etc. should not be checked
    assert!(media("(min-width: 768px)").is_empty());
    assert!(media("(resolution: 2dppx)").is_empty());
  }

  #[test]
  fn allows_valid_hover() {
    assert!(media("(hover: hover)").is_empty());
    assert!(media("(hover: none)").is_empty());
  }

  #[test]
  fn reports_unknown_hover_value() {
    assert_eq!(media("(hover: yes)").len(), 1);
  }

  #[test]
  fn ignores_non_media_at_rules() {
    let css = "@supports (prefers-color-scheme: bogus) {}";
    assert!(lint(NAME, json!(true), css, Syntax::Css).is_empty());
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
    let options = json!([true, {
      "ignoreMediaFeatureNameValues": { "prefers-color-scheme": ["dimmed"], "/^hov/": "/^y/" }
    }]);
    assert!(media_with(options.clone(), "(prefers-color-scheme: dimmed)").is_empty());
    assert!(media_with(options.clone(), "(hover: yes)").is_empty());
    assert_eq!(
      media_with(options.clone(), "(prefers-color-scheme: dim)").len(),
      1
    );
    assert_eq!(media_with(options, "(orientation: diagonal)").len(), 1);
  }
}
