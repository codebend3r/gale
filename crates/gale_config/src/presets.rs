//! The built-in presets: `gale:recommended`, `gale:all`, and approximations
//! of the `stylelint-config-*` packages, built from the linter's rule table.

use std::collections::HashMap;

use gale_linter::rules::Preset;

use crate::{RuleConfig, Severity};

/// The name of every built-in rule, in registration order.
///
/// Read from the linter's rule table, so `gale:all` can never drift from
/// what the registry runs.
fn all_rule_names() -> impl Iterator<Item = &'static str> {
  gale_linter::rules::RULES
    .iter()
    .map(|entry| entry.rule.name())
}

/// The names of the built-in rules the rule table puts in `preset`, in
/// registration order.
fn rules_in(preset: Preset) -> impl Iterator<Item = &'static str> {
  gale_linter::rules::RULES
    .iter()
    .filter(move |entry| entry.presets.contains(&preset))
    .map(|entry| entry.rule.name())
}

/// The rule names that make up `gale:recommended`, error rules first.
///
/// Exposed so other crates (the LSP) can fall back to the same default set
/// instead of inventing their own.
pub fn recommended_rule_names() -> Vec<&'static str> {
  rules_in(Preset::GaleError)
    .chain(rules_in(Preset::GaleWarning))
    .collect()
}

/// Resolve a built-in preset name into a map of rule configurations.
///
/// Returns `None` if the preset name is not recognised.
pub fn resolve_preset(name: &str) -> Option<HashMap<String, RuleConfig>> {
  match name {
    "gale:recommended" => {
      let mut rules = HashMap::new();
      for rule in rules_in(Preset::GaleError) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Error),
            options: None,
          },
        );
      }
      for rule in rules_in(Preset::GaleWarning) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      Some(rules)
    }
    "gale:all" => {
      let mut rules = HashMap::new();
      for rule in all_rule_names() {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      Some(rules)
    }
    "stylelint-config-recommended" => {
      let mut rules = HashMap::new();
      for rule in rules_in(Preset::Recommended) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      // Match the real stylelint-config-recommended options:
      // declaration-block-no-duplicate-properties: [true, { ignore: ["consecutive-duplicates-with-different-syntaxes"] }]
      rules.insert(
        "declaration-block-no-duplicate-properties".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignore": ["consecutive-duplicates-with-different-syntaxes"]
          })),
        },
      );
      // selector-type-no-unknown: [true, { ignore: ["custom-elements"] }]
      rules.insert(
        "selector-type-no-unknown".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignore": ["custom-elements"]
          })),
        },
      );
      Some(rules)
    }
    "stylelint-config-recommended-scss" => {
      let mut rules = HashMap::new();
      for rule in rules_in(Preset::Recommended) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      // The real stylelint-config-recommended-scss disables these core rules
      // because SCSS has its own replacements (scss/at-rule-no-unknown, etc.)
      for &rule in &[
        "annotation-no-unknown",
        "at-rule-no-unknown",
        "comment-no-empty",
        "function-no-unknown",
        "media-query-no-invalid",
      ] {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Off),
            options: None,
          },
        );
      }
      // Enable SCSS-specific rules from stylelint-config-recommended-scss
      for rule in rules_in(Preset::RecommendedScss) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      Some(rules)
    }
    "stylelint-config-standard" => {
      // Standard extends recommended, then adds extra rules.
      let mut rules = HashMap::new();
      for rule in rules_in(Preset::Recommended) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      for rule in rules_in(Preset::Standard) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      // Match the real stylelint-config-recommended options:
      rules.insert(
        "declaration-block-no-duplicate-properties".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignore": ["consecutive-duplicates-with-different-syntaxes"]
          })),
        },
      );
      rules.insert(
        "selector-type-no-unknown".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignore": ["custom-elements"]
          })),
        },
      );
      // Match the real stylelint-config-standard options:
      rules.insert(
        "comment-empty-line-before".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "except": ["first-nested"],
              "ignore": ["stylelint-commands"]
          })),
        },
      );
      rules.insert(
        "value-no-vendor-prefix".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignoreValues": ["box", "inline-box"]
          })),
        },
      );
      rules.insert(
        "length-zero-no-unit".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({"ignore": ["custom-properties"]})),
        },
      );
      Some(rules)
    }
    "stylelint-config-standard-scss" => {
      // Standard-SCSS extends standard, then disables rules that conflict with SCSS.
      let mut rules = HashMap::new();
      for rule in rules_in(Preset::Recommended) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      for rule in rules_in(Preset::Standard) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      // Include options from recommended + standard presets
      rules.insert(
        "declaration-block-no-duplicate-properties".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignore": ["consecutive-duplicates-with-different-syntaxes"]
          })),
        },
      );
      rules.insert(
        "selector-type-no-unknown".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignore": ["custom-elements"]
          })),
        },
      );
      rules.insert(
        "comment-empty-line-before".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "except": ["first-nested"],
              "ignore": ["stylelint-commands"]
          })),
        },
      );
      rules.insert(
        "value-no-vendor-prefix".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({
              "ignoreValues": ["box", "inline-box"]
          })),
        },
      );
      rules.insert(
        "length-zero-no-unit".to_string(),
        RuleConfig {
          severity: Some(Severity::Warning),
          options: Some(serde_json::json!({"ignore": ["custom-properties"]})),
        },
      );
      // Disable rules that conflict with SCSS (inherited from recommended-scss)
      for &rule in &[
        "annotation-no-unknown",
        "at-rule-no-unknown",
        "comment-no-empty",
        "function-no-unknown",
        "media-query-no-invalid",
      ] {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Off),
            options: None,
          },
        );
      }
      // Enable SCSS-specific rules (inherited from recommended-scss)
      for rule in rules_in(Preset::RecommendedScss) {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      Some(rules)
    }
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn resolve_recommended_preset() {
    let preset = resolve_preset("gale:recommended").unwrap();
    // Should contain all error + warning rules.
    assert_eq!(preset.len(), recommended_rule_names().len());
    // Spot-check severities.
    assert_eq!(preset["block-no-empty"].severity, Some(Severity::Error));
    assert_eq!(preset["color-hex-length"].severity, Some(Severity::Warning));
  }

  #[test]
  fn resolve_all_preset() {
    let preset = resolve_preset("gale:all").unwrap();
    assert_eq!(preset.len(), all_rule_names().count());
    // Every rule should be warning.
    for rule_cfg in preset.values() {
      assert_eq!(rule_cfg.severity, Some(Severity::Warning));
    }
  }

  #[test]
  fn unknown_preset_returns_none() {
    assert!(resolve_preset("gale:nonexistent").is_none());
    assert!(resolve_preset("stylelint:recommended").is_none());
  }

  #[test]
  fn standard_preset_includes_length_zero_no_unit_ignore_custom_properties() {
    let preset = resolve_preset("stylelint-config-standard").unwrap();
    let rule = preset
      .get("length-zero-no-unit")
      .expect("length-zero-no-unit should be in standard preset");
    let opts = rule.options.as_ref().expect("should have options");
    let ignore = opts.get("ignore").expect("should have ignore key");
    let arr = ignore.as_array().expect("ignore should be an array");
    assert!(
      arr.iter().any(|v| v.as_str() == Some("custom-properties")),
      "ignore should contain 'custom-properties'; got: {:?}",
      arr
    );
  }

  #[test]
  fn standard_scss_preset_includes_length_zero_no_unit_ignore_custom_properties() {
    let preset = resolve_preset("stylelint-config-standard-scss").unwrap();
    let rule = preset
      .get("length-zero-no-unit")
      .expect("length-zero-no-unit should be in standard-scss preset");
    let opts = rule.options.as_ref().expect("should have options");
    let ignore = opts.get("ignore").expect("should have ignore key");
    let arr = ignore.as_array().expect("ignore should be an array");
    assert!(
      arr.iter().any(|v| v.as_str() == Some("custom-properties")),
      "ignore should contain 'custom-properties'; got: {:?}",
      arr
    );
  }
}
