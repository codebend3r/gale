//! A config file as written, before `extends` and overrides are resolved:
//! the serde model that every config format deserializes into.

use std::collections::HashMap;

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

use crate::{RuleConfig, Severity};

/// Deserialize a value that can be either a single string or an array of strings.
fn string_or_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
  D: Deserializer<'de>,
{
  #[derive(Deserialize)]
  #[serde(untagged)]
  enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
  }

  Option::<StringOrVec>::deserialize(deserializer).map(|opt| {
    opt.map(|v| match v {
      StringOrVec::Single(s) => vec![s],
      StringOrVec::Multiple(v) => v,
    })
  })
}

/// Stands in for an `extends` entry that is not a string: a JavaScript
/// expression the static config reader could not evaluate (which it turns
/// into `null`), or an inline config object.  Resolving it fails, so it is
/// warned about and skipped like any other entry gale cannot find.
pub(crate) const UNEVALUATED_EXTENDS: &str = "<a JavaScript expression gale cannot evaluate>";

/// Deserialize `extends`: a single entry or an array of them.
///
/// Unlike [`string_or_vec`], entries that are not strings do not make the
/// whole config fail to load; each becomes [`UNEVALUATED_EXTENDS`], so the
/// user is told an entry was skipped instead of the config being rejected or
/// silently losing its `extends`.
fn extends_entries<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
  D: Deserializer<'de>,
{
  let entry = |value: serde_json::Value| match value {
    serde_json::Value::String(name) => name,
    _ => UNEVALUATED_EXTENDS.to_string(),
  };
  Ok(
    match Option::<serde_json::Value>::deserialize(deserializer)? {
      None => None,
      Some(serde_json::Value::Array(items)) => Some(items.into_iter().map(entry).collect()),
      Some(other) => Some(vec![entry(other)]),
    },
  )
}

/// What is actually stored in a config file on disk.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
  pub rules: Option<HashMap<String, RuleConfigValue>>,
  pub ignore_patterns: Option<Vec<String>>,
  /// Stylelint uses `ignoreFiles` for the same purpose as `ignorePatterns`.
  /// Accept both field names — when both are present the lists are merged.
  #[serde(default)]
  pub ignore_files: Option<Vec<String>>,
  pub formatter: Option<String>,
  /// List of shared configs / presets to extend (e.g. `"gale:recommended"`).
  /// Accepts a single string or an array of strings.
  #[serde(default, deserialize_with = "extends_entries")]
  pub extends: Option<Vec<String>>,
  /// File-pattern-based overrides (like Stylelint's `overrides` field).
  pub overrides: Option<Vec<ConfigOverride>>,
  /// Stylelint's `plugins` field — Gale does not support plugins, but we
  /// accept the field so configs with plugins don't fail to parse.
  #[serde(default)]
  pub plugins: Option<serde_json::Value>,
  /// Stylelint's `reportNeedlessDisables` option.  When `true`, report
  /// `stylelint-disable` comments that don't suppress any warnings.
  #[serde(default)]
  pub report_needless_disables: Option<serde_json::Value>,
  /// Stylelint's `defaultSeverity` option.  When set, rules that don't
  /// specify their own severity will use this value instead of their
  /// built-in default.
  #[serde(default)]
  pub default_severity: Option<String>,
  /// Stylelint's top-level `customSyntax` field.  When set to an
  /// unsupported value, all files are skipped gracefully.
  #[serde(default)]
  pub custom_syntax: Option<serde_json::Value>,
  /// Stylelint's `ignoreDisables` option.
  #[serde(default)]
  pub ignore_disables: Option<bool>,
  /// Stylelint's `reportInvalidScopeDisables` option (`true`, `false`, or
  /// `[bool, { except, severity }]`).
  #[serde(default)]
  pub report_invalid_scope_disables: Option<serde_json::Value>,
  /// Stylelint's `reportDescriptionlessDisables` option (same shapes).
  #[serde(default)]
  pub report_descriptionless_disables: Option<serde_json::Value>,
  /// Stylelint's `reportUnscopedDisables` option (same shapes).
  #[serde(default)]
  pub report_unscoped_disables: Option<serde_json::Value>,
  /// Stylelint's `allowEmptyInput` option.
  #[serde(default)]
  pub allow_empty_input: Option<bool>,
  /// Stylelint's `quiet` option.
  #[serde(default)]
  pub quiet: Option<bool>,
  /// Stylelint's `fix` option: `true`, `false`, `"strict"` or `"lax"`.
  #[serde(default)]
  pub fix: Option<serde_json::Value>,
  /// Stylelint's `cache` option.
  #[serde(default)]
  pub cache: Option<bool>,
  /// Stylelint's `cacheLocation` option.
  #[serde(default)]
  pub cache_location: Option<String>,
  /// Stylelint's `cacheStrategy` option.
  #[serde(default)]
  pub cache_strategy: Option<String>,
}

/// A single override entry as it appears in the config file.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOverride {
  /// Glob patterns for files this override applies to.
  /// Accepts a single string or an array of strings.
  #[serde(default, deserialize_with = "string_or_vec")]
  pub files: Option<Vec<String>>,
  /// Glob patterns for files to EXCLUDE from this override.
  /// Stylelint's `ignoreFiles` field within an override entry.
  #[serde(default, alias = "ignoreFiles", deserialize_with = "string_or_vec")]
  pub ignore_files: Option<Vec<String>>,
  /// Rules to apply for matching files.
  pub rules: Option<HashMap<String, RuleConfigValue>>,
  /// Shared configs to extend for matching files.
  #[serde(default, deserialize_with = "extends_entries")]
  pub extends: Option<Vec<String>>,
  /// Stylelint's `customSyntax` field — specifies a PostCSS syntax plugin
  /// for non-standard file types (e.g. `postcss-markdown`, `postcss-html`).
  /// Gale only supports `postcss-scss`, `postcss-less`, and `postcss` (CSS).
  /// Files matching an override with an unsupported `customSyntax` are
  /// skipped gracefully.
  #[serde(default)]
  pub custom_syntax: Option<serde_json::Value>,
}

/// A serde-friendly enum matching Stylelint's flexible rule value format.
///
/// Accepts any of:
/// - `null`             (null — treated as off)
/// - `true` / `false`  (boolean — true means error, false means off)
/// - `0`               (numeric zero — treated as off)
/// - `"error"` / `"warning"` / `"off"` (string severity)
/// - `["error", { ...options }]` (tuple of severity + options)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleConfigValue {
  /// Null: treated as Off (used in JS configs to disable inherited rules).
  Null(Option<()>),
  /// Boolean shorthand: `true` → Error, `false` → Off.
  Bool(bool),
  /// Numeric value (e.g. `0` for max-rules).
  Number(serde_json::Number),
  /// String severity: `"error"`, `"warning"`, `"off"`.
  Severity(String),
  /// Array form: `["error", { ...options }]`.
  Array(Vec<serde_json::Value>),
  /// Object form: a bare object used as the primary option (e.g.
  /// `{ "border": "none" }` for `declaration-property-value-disallowed-list`).
  /// Treated as error severity with the object as options.
  Object(serde_json::Map<String, serde_json::Value>),
}

impl RuleConfigValue {
  /// Convert the raw config value into a resolved [`RuleConfig`].
  pub fn resolve(&self) -> RuleConfig {
    match self {
      RuleConfigValue::Null(_) => RuleConfig {
        severity: Some(Severity::Off),
        options: None,
      },
      RuleConfigValue::Bool(b) => RuleConfig {
        severity: Some(if *b { Severity::Error } else { Severity::Off }),
        options: None,
      },
      RuleConfigValue::Number(n) => {
        // Numeric value — store as the primary option.
        // In Stylelint, a numeric primary option (e.g. `selector-max-id: 0`)
        // means the rule is enabled with that numeric value as the max.
        // 0 is a valid max (meaning "disallow entirely").
        RuleConfig {
          severity: Some(Severity::Error),
          options: Some(serde_json::Value::Number(n.clone())),
        }
      }
      RuleConfigValue::Severity(s) => {
        if is_severity_string(s) {
          RuleConfig {
            severity: Some(parse_severity(s)),
            options: None,
          }
        } else {
          // Not a known severity — treat as a primary option value.
          // e.g. `color-named: "never"` or `color-hex-length: "short"`
          // means the rule is enabled at error severity with that
          // string as its primary option.
          RuleConfig {
            severity: Some(Severity::Error),
            options: Some(serde_json::Value::String(s.clone())),
          }
        }
      }
      RuleConfigValue::Array(items) => {
        let first = items.first();
        // Check if the first element is a known severity string.
        let is_severity_str = first
          .and_then(|v| v.as_str())
          .map(|s| {
            matches!(
              s.to_lowercase().as_str(),
              "error" | "warning" | "warn" | "off"
            )
          })
          .unwrap_or(false);
        let is_bool = first.map(|v| v.is_boolean()).unwrap_or(false);

        // Stylelint allows secondary options to override severity in
        // every array form: `[true, { severity: "warning" }]`,
        // `["error", { severity: "warning" }]`, and — the form real
        // configs use most — `["never", { severity: "warning" }]`.
        let secondary_severity = severity_from_secondary(items.get(1));

        if is_severity_str {
          // First element is a severity: ["error", { options }]
          let severity =
            secondary_severity.or_else(|| first.and_then(|v| v.as_str()).map(parse_severity));
          let options = items.get(1).cloned();
          RuleConfig { severity, options }
        } else if is_bool {
          // First element is a boolean: [true, { options }]
          let enabled = first.and_then(|v| v.as_bool()).unwrap_or(true);
          let severity = if enabled {
            secondary_severity.or(Some(Severity::Error))
          } else {
            // An explicit `false` disables the rule outright; a
            // secondary severity cannot resurrect it.
            Some(Severity::Off)
          };
          let options = items.get(1).cloned();
          RuleConfig { severity, options }
        } else {
          // First element is a primary option (e.g. "always", 4):
          // ["always", { except: [...] }] or [4, { ... }]
          // Severity defaults to Error (enabled) unless the secondary
          // options carry an explicit `severity`.  The entire array is
          // stored as options so rules can access options[0] for the
          // primary option and options[1] for secondary options.
          let options = Some(serde_json::Value::Array(items.clone()));
          RuleConfig {
            severity: secondary_severity.or(Some(Severity::Error)),
            options,
          }
        }
      }
      RuleConfigValue::Object(map) => {
        // A bare object is treated as error severity with the object as options.
        RuleConfig {
          severity: Some(Severity::Error),
          options: Some(serde_json::Value::Object(map.clone())),
        }
      }
    }
  }
}

/// Extract an explicit `severity` from a rule's secondary options object.
///
/// Stylelint lets any rule downgrade or upgrade itself via the secondary
/// options, e.g. `"color-named": ["never", { "severity": "warning" }]`.
fn severity_from_secondary(secondary: Option<&serde_json::Value>) -> Option<Severity> {
  let serde_json::Value::Object(obj) = secondary? else {
    return None;
  };
  let serde_json::Value::String(s) = obj.get("severity")? else {
    return None;
  };
  is_severity_string(s).then(|| parse_severity(s))
}

/// Returns `true` if the string is a known severity keyword.
fn is_severity_string(s: &str) -> bool {
  matches!(
    s.to_lowercase().as_str(),
    "error" | "warning" | "warn" | "off"
  )
}

/// Maps a severity keyword to its enum; anything unrecognised is `Off`.
fn parse_severity(s: &str) -> Severity {
  match s.to_lowercase().as_str() {
    "error" => Severity::Error,
    "warning" | "warn" => Severity::Warning,
    _ => Severity::Off,
  }
}

/// Check whether a raw `RuleConfigValue` explicitly sets a severity level.
///
/// Returns `true` for values like `"error"`, `"warning"`, `["error", { ... }]`,
/// or `[true, { severity: "warning" }]`.  Returns `false` for `true`, `"without-alpha"`,
/// `["always", { ... }]` (no explicit severity in secondary), etc.
pub(crate) fn has_explicit_severity_in_value(v: &RuleConfigValue) -> bool {
  match v {
    RuleConfigValue::Null(_) => true,
    RuleConfigValue::Bool(false) => true,
    RuleConfigValue::Bool(true) => false,
    RuleConfigValue::Number(_) => false,
    RuleConfigValue::Severity(s) => is_severity_string(s),
    RuleConfigValue::Array(items) => {
      // Check if first element is a severity string.
      let first_is_severity = items
        .first()
        .and_then(|v| v.as_str())
        .map(is_severity_string)
        .unwrap_or(false);
      if first_is_severity {
        return true;
      }
      // Check secondary options for { severity: "..." }.
      if let Some(serde_json::Value::Object(obj)) = items.get(1)
        && let Some(serde_json::Value::String(s)) = obj.get("severity")
      {
        return is_severity_string(s);
      }
      false
    }
    RuleConfigValue::Object(_) => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_rule_bool() {
    let v = RuleConfigValue::Bool(true);
    let r = v.resolve();
    assert_eq!(r.severity, Some(Severity::Error));
    assert!(r.options.is_none());

    let v = RuleConfigValue::Bool(false);
    let r = v.resolve();
    assert_eq!(r.severity, Some(Severity::Off));
  }

  #[test]
  fn parse_rule_string() {
    let v = RuleConfigValue::Severity("warning".to_string());
    let r = v.resolve();
    assert_eq!(r.severity, Some(Severity::Warning));
  }

  #[test]
  fn parse_rule_array() {
    let v: RuleConfigValue = serde_json::from_str(r#"["error", {"max": 3}]"#).unwrap();
    let r = v.resolve();
    assert_eq!(r.severity, Some(Severity::Error));
    assert!(r.options.is_some());
  }

  #[test]
  fn extends_string_json() {
    let json = r#"{ "extends": "gale:recommended" }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    assert_eq!(raw.extends, Some(vec!["gale:recommended".to_string()]));
  }

  #[test]
  fn extends_array_json() {
    let json = r#"{ "extends": ["gale:recommended", "gale:all"] }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["gale:recommended".to_string(), "gale:all".to_string()])
    );
  }

  #[test]
  fn overrides_parsed_from_json() {
    let json = r#"{
            "extends": "gale:recommended",
            "overrides": [
                {
                    "files": "**/*.scss",
                    "rules": {
                        "no-duplicate-selectors": null,
                        "comment-no-empty": null
                    }
                }
            ]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    assert!(raw.overrides.is_some());
    let overrides = raw.overrides.unwrap();
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].files, Some(vec!["**/*.scss".to_string()]));
    let rules = overrides[0].rules.as_ref().unwrap();
    assert_eq!(rules.len(), 2);
  }

  #[test]
  fn overrides_files_string_or_vec() {
    // Single string
    let json = r#"{ "overrides": [{ "files": "**/*.scss", "rules": {} }] }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let overrides = raw.overrides.unwrap();
    assert_eq!(overrides[0].files, Some(vec!["**/*.scss".to_string()]));

    // Array
    let json = r#"{ "overrides": [{ "files": ["**/*.scss", "**/*.less"], "rules": {} }] }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let overrides = raw.overrides.unwrap();
    assert_eq!(
      overrides[0].files,
      Some(vec!["**/*.scss".to_string(), "**/*.less".to_string()])
    );
  }

  #[test]
  fn overrides_with_extends() {
    let json = r#"{
            "overrides": [
                {
                    "files": "**/*.scss",
                    "extends": "stylelint-config-standard-scss"
                }
            ]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let overrides = raw.overrides.unwrap();
    assert_eq!(
      overrides[0].extends,
      Some(vec!["stylelint-config-standard-scss".to_string()])
    );
  }

  #[test]
  fn custom_syntax_parsed_from_json_override() {
    let json = r#"{
            "overrides": [{
                "files": "**/*.md",
                "customSyntax": "postcss-markdown",
                "rules": { "block-no-empty": true }
            }]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let overrides = raw.overrides.unwrap();
    assert_eq!(
      overrides[0].custom_syntax,
      Some(serde_json::Value::String("postcss-markdown".to_string()))
    );
  }

  #[test]
  fn custom_syntax_parsed_from_json_top_level() {
    let json = r#"{
            "customSyntax": "postcss-scss",
            "rules": { "block-no-empty": true }
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    assert_eq!(
      raw.custom_syntax,
      Some(serde_json::Value::String("postcss-scss".to_string()))
    );
  }
}
