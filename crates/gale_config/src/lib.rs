use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use globset::{Glob, GlobMatcher};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("failed to read config file: {0}")]
  Io(#[from] std::io::Error),

  #[error("failed to parse JSON: {0}")]
  Json(#[from] serde_json::Error),

  #[error("failed to parse YAML: {0}")]
  Yaml(#[from] serde_yaml::Error),

  #[error("failed to parse TOML: {0}")]
  Toml(#[from] toml::de::Error),

  #[error("unsupported config file format: {0}")]
  UnsupportedFormat(String),
}

// ---------------------------------------------------------------------------
// Public enums
// ---------------------------------------------------------------------------

/// Rule severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  Error,
  Warning,
  Off,
}

/// Output formatter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FormatterType {
  #[default]
  Text,
  Json,
  Compact,
}

// ---------------------------------------------------------------------------
// Resolved config (public API)
// ---------------------------------------------------------------------------

/// A single rule's resolved configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleConfig {
  pub severity: Option<Severity>,
  pub options: Option<serde_json::Value>,
}

/// A resolved override entry: a set of glob patterns and the rules to apply
/// when a file matches any of them.
#[derive(Debug, Clone)]
pub struct ResolvedOverride {
  pub file_patterns: Vec<String>,
  matchers: Vec<GlobMatcher>,
  exclude_matchers: Vec<GlobMatcher>,
  pub rules: HashMap<String, RuleConfig>,
  /// The `customSyntax` value from this override, if any.
  /// `None` means no custom syntax was specified (use default detection).
  /// `Some(name)` is the raw string value (e.g. `"postcss-markdown"`).
  pub custom_syntax: Option<String>,
}

impl ResolvedOverride {
  /// Create a new resolved override from glob pattern strings, exclusion
  /// patterns, rules, and an optional custom syntax name.
  pub fn new(
    file_patterns: Vec<String>,
    ignore_patterns: Vec<String>,
    rules: HashMap<String, RuleConfig>,
    custom_syntax: Option<String>,
  ) -> Self {
    let matchers = file_patterns
      .iter()
      .filter_map(|pat| Glob::new(pat).ok().map(|g| g.compile_matcher()))
      .collect();
    let exclude_matchers = ignore_patterns
      .iter()
      .filter_map(|pat| Glob::new(pat).ok().map(|g| g.compile_matcher()))
      .collect();
    Self {
      file_patterns,
      matchers,
      exclude_matchers,
      rules,
      custom_syntax,
    }
  }

  /// Check whether a file path matches any of this override's glob patterns
  /// and is NOT excluded by the `ignoreFiles` patterns.
  pub fn matches(&self, file_path: &str) -> bool {
    let path = Path::new(file_path);
    let included = self.matchers.iter().any(|m| m.is_match(path));
    if !included {
      return false;
    }
    // Check exclusions
    !self.exclude_matchers.iter().any(|m| m.is_match(path))
  }

  /// Check whether a file path matches the override's `files` patterns.
  pub fn matches_files(&self, file_path: &str) -> bool {
    let path = Path::new(file_path);
    self.matchers.iter().any(|m| m.is_match(path))
  }

  /// Check whether a file path is excluded by this override's `ignoreFiles`
  /// patterns.
  pub fn is_excluded(&self, file_path: &str) -> bool {
    let path = Path::new(file_path);
    self.exclude_matchers.iter().any(|m| m.is_match(path))
  }
}

/// How autofix behaves when it is switched on from the config file.
///
/// Mirrors the `--fix` flag: `Strict` skips files with parse errors, `Lax`
/// fixes them anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixMode {
  Strict,
  Lax,
}

/// The fully-resolved configuration used by the linter at runtime.
#[derive(Debug, Clone)]
pub struct GaleConfig {
  pub rules: HashMap<String, RuleConfig>,
  pub ignore_patterns: Vec<String>,
  pub formatter: FormatterType,
  pub overrides: Vec<ResolvedOverride>,
  /// Directory containing the config file.  Used by [`rules_for_file`] to
  /// resolve override glob patterns relative to the config location.
  pub config_dir: Option<PathBuf>,
  /// Plugin names declared in the config file (extracted from the `plugins` field).
  pub plugins: Vec<String>,
  /// When `true`, report `stylelint-disable` comments that don't actually
  /// suppress any warnings (Stylelint's `reportNeedlessDisables` option).
  pub report_needless_disables: bool,
  /// The default severity for rules that don't specify their own severity.
  /// Corresponds to Stylelint's `defaultSeverity` option.  When `None`, rules
  /// use their built-in default severity.
  pub default_severity: Option<Severity>,
  /// Top-level `customSyntax` value from the config file.
  /// When set to an unsupported syntax, all files should be skipped.
  pub custom_syntax: Option<String>,
  /// Stylelint's `ignoreDisables`: report problems even inside
  /// `stylelint-disable` ranges.
  pub ignore_disables: bool,
  /// Stylelint's `reportInvalidScopeDisables`: report disable comments that
  /// name a rule which is not configured.
  pub report_invalid_scope_disables: bool,
  /// Stylelint's `reportDescriptionlessDisables`: report disable comments
  /// that carry no `-- description`.
  pub report_descriptionless_disables: bool,
  /// Stylelint's `reportUnscopedDisables`: report disable comments that
  /// name no rule at all.
  pub report_unscoped_disables: bool,
  /// Stylelint's `allowEmptyInput`: succeed when no files match.
  pub allow_empty_input: bool,
  /// Stylelint's `quiet`: only report error-severity problems.
  pub quiet: bool,
  /// Stylelint's `fix`: autofix without passing `--fix`.  `None` leaves
  /// fixing off unless the CLI asks for it.
  pub fix: Option<FixMode>,
  /// Stylelint's `cache`: skip files that were clean on the previous run.
  pub cache: bool,
  /// Stylelint's `cacheLocation`, relative to the working directory.
  pub cache_location: Option<PathBuf>,
  /// Stylelint's `cacheStrategy`: `"metadata"` or `"content"`.  Validated
  /// by the CLI, which owns the cache.
  pub cache_strategy: Option<String>,
}

impl GaleConfig {
  /// Return the effective rules for a given file path.
  ///
  /// Starts with the base rules, then applies each matching override in order.
  /// Later overrides win over earlier ones.
  ///
  /// Override glob patterns are matched against the file path relative to the
  /// config directory (if known), matching Stylelint's behaviour.
  pub fn rules_for_file(&self, file_path: &str) -> HashMap<String, RuleConfig> {
    if self.overrides.is_empty() {
      return self.rules.clone();
    }

    // Compute the file path relative to the config directory for glob
    // matching.  Fall back to the original path when no config_dir is set.
    let relative_path: std::borrow::Cow<'_, str> = if let Some(ref config_dir) = self.config_dir {
      let abs_file = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
      } else {
        std::env::current_dir().unwrap_or_default().join(file_path)
      };
      match abs_file.strip_prefix(config_dir) {
        Ok(rel) => rel.to_string_lossy().into_owned().into(),
        Err(_) => file_path.into(),
      }
    } else {
      file_path.into()
    };

    let mut rules = self.rules.clone();
    for override_entry in &self.overrides {
      if override_entry.matches(&relative_path) || override_entry.matches(file_path) {
        for (name, cfg) in &override_entry.rules {
          if cfg.severity == Some(Severity::Off) {
            rules.remove(name);
          } else {
            rules.insert(name.clone(), cfg.clone());
          }
        }
      }
    }
    rules
  }

  /// Returns `true` if any overrides are configured.
  pub fn has_overrides(&self) -> bool {
    !self.overrides.is_empty()
  }

  /// Returns `true` when a file matches an override's `files` patterns AND
  /// that same override's `ignoreFiles` patterns.  Stylelint treats such
  /// files as fully ignored (`"ignored": true`) — no linting at all.
  ///
  /// The check is performed against both the relative path (relative to the
  /// config directory) and the raw file path, mirroring
  /// [`rules_for_file`]'s matching logic.
  pub fn is_file_ignored_by_override(&self, file_path: &str) -> bool {
    if self.overrides.is_empty() {
      return false;
    }

    let relative_path: std::borrow::Cow<'_, str> = if let Some(ref config_dir) = self.config_dir {
      let abs_file = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
      } else {
        std::env::current_dir().unwrap_or_default().join(file_path)
      };
      match abs_file.strip_prefix(config_dir) {
        Ok(rel) => rel.to_string_lossy().into_owned().into(),
        Err(_) => file_path.into(),
      }
    } else {
      file_path.into()
    };

    for ov in &self.overrides {
      let matches_files = ov.matches_files(&relative_path) || ov.matches_files(file_path);
      let excluded = ov.is_excluded(&relative_path) || ov.is_excluded(file_path);
      if matches_files && excluded {
        return true;
      }
    }
    false
  }

  /// The `customSyntax` that applies to `file_path`, if the config names one.
  ///
  /// The first override whose globs match the file wins; otherwise the
  /// top-level `customSyntax` applies.  Whether Gale can actually parse the
  /// named syntax is the caller's decision, since the parsers live outside
  /// this crate.
  pub fn custom_syntax_for_file(&self, file_path: &str) -> Option<&str> {
    // Check overrides first — they take precedence over top-level.
    if !self.overrides.is_empty() {
      let relative_path: std::borrow::Cow<'_, str> = if let Some(ref config_dir) = self.config_dir {
        let abs_file = if Path::new(file_path).is_absolute() {
          PathBuf::from(file_path)
        } else {
          std::env::current_dir().unwrap_or_default().join(file_path)
        };
        match abs_file.strip_prefix(config_dir) {
          Ok(rel) => rel.to_string_lossy().into_owned().into(),
          Err(_) => file_path.into(),
        }
      } else {
        file_path.into()
      };

      for ov in &self.overrides {
        let matches = ov.matches(&relative_path) || ov.matches(file_path);
        if matches && let Some(ref syntax_name) = ov.custom_syntax {
          return Some(syntax_name);
        }
      }
    }

    self.custom_syntax.as_deref()
  }
}

impl Default for GaleConfig {
  fn default() -> Self {
    Self {
      rules: HashMap::new(),
      ignore_patterns: Vec::new(),
      formatter: FormatterType::Text,
      overrides: Vec::new(),
      config_dir: None,
      plugins: Vec::new(),
      report_needless_disables: false,
      default_severity: None,
      custom_syntax: None,
      ignore_disables: false,
      report_invalid_scope_disables: false,
      report_descriptionless_disables: false,
      report_unscoped_disables: false,
      allow_empty_input: false,
      quiet: false,
      fix: None,
      cache: false,
      cache_location: None,
      cache_strategy: None,
    }
  }
}

// ---------------------------------------------------------------------------
// Raw config file representation (serde)
// ---------------------------------------------------------------------------

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
  #[serde(default, deserialize_with = "string_or_vec")]
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
  #[serde(default, deserialize_with = "string_or_vec")]
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
fn has_explicit_severity_in_value(v: &RuleConfigValue) -> bool {
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

// ---------------------------------------------------------------------------
// Built-in presets
// ---------------------------------------------------------------------------

/// All rule names that exist in the linter registry.
/// Kept in sync with `gale_linter::rules::register_all`.
const ALL_RULE_NAMES: &[&str] = &[
  "@stylistic/at-rule-name-case",
  "@stylistic/at-rule-name-space-after",
  "@stylistic/at-rule-semicolon-newline-after",
  "@stylistic/at-rule-semicolon-space-before",
  "@stylistic/block-closing-brace-empty-line-before",
  "@stylistic/block-closing-brace-newline-after",
  "@stylistic/block-closing-brace-newline-before",
  "@stylistic/block-closing-brace-space-before",
  "@stylistic/block-opening-brace-newline-after",
  "@stylistic/block-opening-brace-space-after",
  "@stylistic/block-opening-brace-space-before",
  "@stylistic/color-hex-case",
  "@stylistic/declaration-bang-space-after",
  "@stylistic/declaration-bang-space-before",
  "@stylistic/declaration-block-semicolon-newline-after",
  "@stylistic/declaration-block-semicolon-newline-before",
  "@stylistic/declaration-block-semicolon-space-after",
  "@stylistic/declaration-block-semicolon-space-before",
  "@stylistic/declaration-block-trailing-semicolon",
  "@stylistic/declaration-colon-newline-after",
  "@stylistic/declaration-colon-space-after",
  "@stylistic/declaration-colon-space-before",
  "@stylistic/function-comma-newline-after",
  "@stylistic/function-comma-space-after",
  "@stylistic/function-comma-space-before",
  "@stylistic/function-max-empty-lines",
  "@stylistic/function-parentheses-newline-inside",
  "@stylistic/function-parentheses-space-inside",
  "@stylistic/function-whitespace-after",
  "@stylistic/indentation",
  "@stylistic/max-empty-lines",
  "@stylistic/media-feature-colon-space-after",
  "@stylistic/media-feature-colon-space-before",
  "@stylistic/media-feature-name-case",
  "@stylistic/media-feature-parentheses-space-inside",
  "@stylistic/media-feature-range-operator-space-after",
  "@stylistic/media-feature-range-operator-space-before",
  "@stylistic/media-query-list-comma-newline-after",
  "@stylistic/media-query-list-comma-space-after",
  "@stylistic/media-query-list-comma-space-before",
  "@stylistic/no-empty-first-line",
  "@stylistic/no-eol-whitespace",
  "@stylistic/no-extra-semicolons",
  "@stylistic/no-missing-end-of-source-newline",
  "@stylistic/number-leading-zero",
  "@stylistic/number-no-trailing-zeros",
  "@stylistic/property-case",
  "@stylistic/selector-attribute-brackets-space-inside",
  "@stylistic/selector-attribute-operator-space-after",
  "@stylistic/selector-attribute-operator-space-before",
  "@stylistic/selector-combinator-space-after",
  "@stylistic/selector-combinator-space-before",
  "@stylistic/selector-descendant-combinator-no-non-space",
  "@stylistic/selector-list-comma-newline-after",
  "@stylistic/selector-list-comma-newline-before",
  "@stylistic/selector-list-comma-space-after",
  "@stylistic/selector-list-comma-space-before",
  "@stylistic/selector-max-empty-lines",
  "@stylistic/selector-pseudo-class-case",
  "@stylistic/selector-pseudo-class-parentheses-space-inside",
  "@stylistic/selector-pseudo-element-case",
  "@stylistic/string-quotes",
  "@stylistic/unicode-bom",
  "@stylistic/unit-case",
  "@stylistic/value-list-comma-newline-after",
  "@stylistic/value-list-comma-newline-before",
  "@stylistic/value-list-comma-space-after",
  "@stylistic/value-list-comma-space-before",
  "@stylistic/value-list-max-empty-lines",
  "alpha-value-notation",
  "annotation-no-unknown",
  "at-rule-allowed-list",
  "at-rule-descriptor-no-unknown",
  "at-rule-descriptor-value-no-unknown",
  "at-rule-disallowed-list",
  "at-rule-empty-line-before",
  "at-rule-no-deprecated",
  "at-rule-no-unknown",
  "at-rule-no-vendor-prefix",
  "at-rule-prelude-no-invalid",
  "at-rule-property-required-list",
  "block-no-empty",
  "block-no-redundant-nested-style-rules",
  "color-function-alias-notation",
  "color-function-notation",
  "color-hex-alpha",
  "color-hex-case",
  "color-hex-length",
  "color-named",
  "color-no-hex",
  "color-no-invalid-hex",
  "comment-empty-line-before",
  "comment-no-empty",
  "comment-pattern",
  "comment-whitespace-inside",
  "comment-word-disallowed-list",
  "container-name-pattern",
  "csstools/value-no-unknown-custom-properties",
  "custom-media-pattern",
  "custom-property-empty-line-before",
  "custom-property-no-missing-var-function",
  "custom-property-pattern",
  "declaration-block-no-duplicate-custom-properties",
  "declaration-block-no-duplicate-properties",
  "declaration-block-no-redundant-longhand-properties",
  "declaration-block-no-shorthand-property-overrides",
  "declaration-block-single-line-max-declarations",
  "declaration-empty-line-before",
  "declaration-no-important",
  "declaration-property-max-values",
  "declaration-property-unit-allowed-list",
  "declaration-property-unit-disallowed-list",
  "declaration-property-value-allowed-list",
  "declaration-property-value-disallowed-list",
  "declaration-property-value-keyword-no-deprecated",
  "declaration-property-value-no-unknown",
  "display-notation",
  "font-family-name-quotes",
  "font-family-no-duplicate-names",
  "font-family-no-missing-generic-family-keyword",
  "font-weight-notation",
  "function-allowed-list",
  "function-calc-no-unspaced-operator",
  "function-disallowed-list",
  "function-linear-gradient-no-nonstandard-direction",
  "function-name-case",
  "function-no-unknown",
  "function-url-no-scheme-relative",
  "function-url-quotes",
  "function-url-scheme-allowed-list",
  "function-url-scheme-disallowed-list",
  "hue-degree-notation",
  "import-notation",
  "keyframe-block-no-duplicate-selectors",
  "keyframe-declaration-no-important",
  "keyframe-selector-notation",
  "keyframes-name-pattern",
  "layer-name-pattern",
  "length-zero-no-unit",
  "lightness-notation",
  "material/no-prefixes",
  "max-line-length",
  "max-nesting-depth",
  "media-feature-name-allowed-list",
  "media-feature-name-disallowed-list",
  "media-feature-name-no-unknown",
  "media-feature-name-no-vendor-prefix",
  "media-feature-name-unit-allowed-list",
  "media-feature-name-value-allowed-list",
  "media-feature-name-value-no-unknown",
  "media-feature-range-notation",
  "media-query-no-invalid",
  "media-type-no-deprecated",
  "named-grid-areas-no-invalid",
  "nesting-selector-no-missing-scoping-root",
  "no-descending-specificity",
  "no-duplicate-at-import-rules",
  "no-duplicate-selectors",
  "no-empty-source",
  "no-invalid-double-slash-comments",
  "no-invalid-position-at-import-rule",
  "no-invalid-position-declaration",
  "no-irregular-whitespace",
  "no-unknown-animations",
  "number-max-precision",
  "order/order",
  "order/properties-alphabetical-order",
  "order/properties-order",
  "plugin/browser-compat",
  "plugin/enforce-variable-for-property",
  "plugin/no-unknown-custom-properties",
  "plugin/no-unused-custom-properties",
  "plugin/require-file-header-comment",
  "property-allowed-list",
  "property-disallowed-list",
  "property-no-deprecated",
  "property-no-unknown",
  "property-no-vendor-prefix",
  "rule-empty-line-before",
  "rule-nesting-at-rule-required-list",
  "rule-selector-property-disallowed-list",
  "scss/at-else-closing-brace-newline-after",
  "scss/at-else-closing-brace-space-after",
  "scss/at-else-empty-line-before",
  "scss/at-else-if-parentheses-space-before",
  "scss/at-extend-no-missing-placeholder",
  "scss/at-function-parentheses-space-before",
  "scss/at-function-pattern",
  "scss/at-if-closing-brace-newline-after",
  "scss/at-if-closing-brace-space-after",
  "scss/at-if-no-null",
  "scss/at-import-partial-extension",
  "scss/at-import-partial-extension-disallowed-list",
  "scss/at-mixin-argumentless-call-parentheses",
  "scss/at-mixin-disallowed-list",
  "scss/at-mixin-parentheses-space-before",
  "scss/at-mixin-pattern",
  "scss/at-rule-conditional-no-parentheses",
  "scss/at-rule-no-unknown",
  "scss/comment-no-empty",
  "scss/comment-no-loud",
  "scss/declaration-nested-properties",
  "scss/declaration-nested-properties-no-divided-groups",
  "scss/dollar-variable-colon-space-after",
  "scss/dollar-variable-colon-space-before",
  "scss/dollar-variable-empty-line-before",
  "scss/dollar-variable-no-missing-interpolation",
  "scss/dollar-variable-pattern",
  "scss/double-slash-comment-empty-line-before",
  "scss/double-slash-comment-inline",
  "scss/double-slash-comment-whitespace-inside",
  "scss/function-disallowed-list",
  "scss/function-no-unknown",
  "scss/function-quote-no-quoted-strings-inside",
  "scss/function-unquote-no-unquoted-strings-inside",
  "scss/load-no-partial-leading-underscore",
  "scss/load-partial-extension",
  "scss/no-duplicate-dollar-variables",
  "scss/no-duplicate-mixins",
  "scss/no-global-function-names",
  "scss/operator-no-newline-after",
  "scss/operator-no-newline-before",
  "scss/operator-no-unspaced",
  "scss/partial-no-import",
  "scss/percent-placeholder-pattern",
  "scss/selector-no-redundant-nesting-selector",
  "selector-anb-no-unmatchable",
  "selector-attribute-name-disallowed-list",
  "selector-attribute-operator-allowed-list",
  "selector-attribute-operator-disallowed-list",
  "selector-attribute-quotes",
  "selector-class-pattern",
  "selector-combinator-allowed-list",
  "selector-combinator-disallowed-list",
  "selector-disallowed-list",
  "selector-id-pattern",
  "selector-max-attribute",
  "selector-max-class",
  "selector-max-combinators",
  "selector-max-compound-selectors",
  "selector-max-id",
  "selector-max-pseudo-class",
  "selector-max-specificity",
  "selector-max-type",
  "selector-max-universal",
  "selector-nested-pattern",
  "selector-no-qualifying-type",
  "selector-no-vendor-prefix",
  "selector-not-notation",
  "selector-pseudo-class-allowed-list",
  "selector-pseudo-class-disallowed-list",
  "selector-pseudo-class-no-unknown",
  "selector-pseudo-element-allowed-list",
  "selector-pseudo-element-colon-notation",
  "selector-pseudo-element-disallowed-list",
  "selector-pseudo-element-no-unknown",
  "selector-type-case",
  "selector-type-no-unknown",
  "shorthand-property-no-redundant-values",
  "spectrum-tools/no-unknown-custom-properties",
  "string-no-newline",
  "string-quotes",
  "syntax-string-no-invalid",
  "time-min-milliseconds",
  "unit-allowed-list",
  "unit-disallowed-list",
  "unit-no-unknown",
  "value-keyword-case",
  "value-no-vendor-prefix",
];

/// The rule names that make up `gale:recommended`, error rules first.
///
/// Exposed so other crates (the LSP) can fall back to the same default set
/// instead of inventing their own.
pub fn recommended_rule_names() -> Vec<&'static str> {
  RECOMMENDED_ERROR_RULES
    .iter()
    .chain(RECOMMENDED_WARNING_RULES.iter())
    .copied()
    .collect()
}

/// Rules enabled at **error** severity in `gale:recommended`.
const RECOMMENDED_ERROR_RULES: &[&str] = &[
  "block-no-empty",
  "color-no-invalid-hex",
  "declaration-block-no-duplicate-properties",
  "declaration-block-no-duplicate-custom-properties",
  "font-family-no-duplicate-names",
  "no-duplicate-at-import-rules",
  "no-duplicate-selectors",
  "no-empty-source",
  "property-no-unknown",
  "selector-pseudo-class-no-unknown",
  "selector-pseudo-element-no-unknown",
  "selector-type-no-unknown",
  "unit-no-unknown",
  "no-descending-specificity",
  "keyframe-block-no-duplicate-selectors",
];

/// Rules enabled at **warning** severity in `gale:recommended`.
const RECOMMENDED_WARNING_RULES: &[&str] = &[
  "color-hex-length",
  "color-hex-case",
  "length-zero-no-unit",
  "declaration-no-important",
  "selector-pseudo-element-colon-notation",
  "no-invalid-double-slash-comments",
  "function-name-case",
  "shorthand-property-no-redundant-values",
  "at-rule-no-vendor-prefix",
  "property-no-vendor-prefix",
  "value-no-vendor-prefix",
  "value-keyword-case",
  "function-url-quotes",
  "number-max-precision",
];

/// Rules enabled at **warning** severity in `stylelint-config-recommended`.
///
/// These mirror the official `stylelint-config-recommended` package, filtered
/// to only include rules that Gale has implemented.
const STYLELINT_RECOMMENDED_RULES: &[&str] = &[
  "annotation-no-unknown",
  "at-rule-descriptor-no-unknown",
  "at-rule-descriptor-value-no-unknown",
  "at-rule-no-deprecated",
  "at-rule-no-unknown",
  "at-rule-prelude-no-invalid",
  "block-no-empty",
  "comment-no-empty",
  "custom-property-no-missing-var-function",
  "declaration-block-no-duplicate-custom-properties",
  "declaration-block-no-duplicate-properties",
  "declaration-block-no-shorthand-property-overrides",
  "declaration-property-value-keyword-no-deprecated",
  "declaration-property-value-no-unknown",
  "font-family-no-duplicate-names",
  "font-family-no-missing-generic-family-keyword",
  "function-calc-no-unspaced-operator",
  "function-no-unknown",
  "keyframe-block-no-duplicate-selectors",
  "keyframe-declaration-no-important",
  "media-feature-name-no-unknown",
  "media-feature-name-value-no-unknown",
  "media-query-no-invalid",
  "media-type-no-deprecated",
  "named-grid-areas-no-invalid",
  "nesting-selector-no-missing-scoping-root",
  "no-descending-specificity",
  "no-duplicate-at-import-rules",
  "no-duplicate-selectors",
  "no-empty-source",
  "no-invalid-double-slash-comments",
  "no-invalid-position-at-import-rule",
  "no-invalid-position-declaration",
  "no-irregular-whitespace",
  "property-no-deprecated",
  "property-no-unknown",
  "selector-anb-no-unmatchable",
  "selector-pseudo-class-no-unknown",
  "selector-pseudo-element-no-unknown",
  "selector-type-no-unknown",
  "string-no-newline",
  "syntax-string-no-invalid",
];

/// Additional rules that `stylelint-config-standard` adds on top of
/// `stylelint-config-recommended`.  All enabled at warning severity.
///
/// Filtered to only include rules that Gale has implemented.
const STYLELINT_STANDARD_EXTRA_RULES: &[&str] = &[
  "alpha-value-notation",
  "at-rule-empty-line-before",
  "at-rule-no-vendor-prefix",
  "block-no-redundant-nested-style-rules",
  "color-function-alias-notation",
  "color-function-notation",
  "color-hex-length",
  "comment-empty-line-before",
  "comment-whitespace-inside",
  "container-name-pattern",
  "custom-media-pattern",
  "custom-property-empty-line-before",
  "custom-property-pattern",
  "declaration-block-no-redundant-longhand-properties",
  "declaration-block-single-line-max-declarations",
  "declaration-empty-line-before",
  "font-family-name-quotes",
  "function-name-case",
  "function-url-quotes",
  "hue-degree-notation",
  "import-notation",
  "keyframe-selector-notation",
  "keyframes-name-pattern",
  "layer-name-pattern",
  "length-zero-no-unit",
  "lightness-notation",
  "media-feature-name-no-vendor-prefix",
  "media-feature-range-notation",
  "number-max-precision",
  "property-no-vendor-prefix",
  "rule-empty-line-before",
  "selector-attribute-quotes",
  "selector-class-pattern",
  "selector-id-pattern",
  "selector-no-vendor-prefix",
  "selector-not-notation",
  "selector-pseudo-element-colon-notation",
  "selector-type-case",
  "shorthand-property-no-redundant-values",
  "value-keyword-case",
  "value-no-vendor-prefix",
];

/// SCSS-specific rules enabled by `stylelint-config-recommended-scss`.
const SCSS_RECOMMENDED_RULES: &[&str] = &[
  "scss/at-extend-no-missing-placeholder",
  "scss/at-if-no-null",
  "scss/at-rule-no-unknown",
  "scss/comment-no-empty",
  "scss/declaration-nested-properties-no-divided-groups",
  "scss/dollar-variable-no-missing-interpolation",
  "scss/function-quote-no-quoted-strings-inside",
  "scss/function-unquote-no-unquoted-strings-inside",
  "scss/load-no-partial-leading-underscore",
  "scss/load-partial-extension",
  "scss/no-duplicate-mixins",
  "scss/no-global-function-names",
  "scss/operator-no-newline-after",
  "scss/operator-no-newline-before",
  "scss/operator-no-unspaced",
];

/// Resolve a built-in preset name into a map of rule configurations.
///
/// Returns `None` if the preset name is not recognised.
pub fn resolve_preset(name: &str) -> Option<HashMap<String, RuleConfig>> {
  match name {
    "gale:recommended" => {
      let mut rules = HashMap::new();
      for &rule in RECOMMENDED_ERROR_RULES {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Error),
            options: None,
          },
        );
      }
      for &rule in RECOMMENDED_WARNING_RULES {
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
      for &rule in ALL_RULE_NAMES {
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
      for &rule in STYLELINT_RECOMMENDED_RULES {
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
      for &rule in STYLELINT_RECOMMENDED_RULES {
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
      for &rule in SCSS_RECOMMENDED_RULES {
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
      for &rule in STYLELINT_RECOMMENDED_RULES {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      for &rule in STYLELINT_STANDARD_EXTRA_RULES {
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
      for &rule in STYLELINT_RECOMMENDED_RULES {
        rules.insert(
          rule.to_string(),
          RuleConfig {
            severity: Some(Severity::Warning),
            options: None,
          },
        );
      }
      for &rule in STYLELINT_STANDARD_EXTRA_RULES {
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
      for &rule in SCSS_RECOMMENDED_RULES {
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

// ---------------------------------------------------------------------------
// Config file search
// ---------------------------------------------------------------------------

/// Well-known config file names in priority order.
const CONFIG_FILENAMES: &[&str] = &[
  "gale.json",
  "gale.toml",
  ".stylelintrc",
  ".stylelintrc.json",
  ".stylelintrc.yml",
  ".stylelintrc.yaml",
  "stylelint.config.js",
  "stylelint.config.mjs",
  "stylelint.config.cjs",
  ".stylelintrc.js",
  ".stylelintrc.cjs",
  ".stylelintrc.mjs",
];

/// Walk upward from `start_dir` looking for the first config file.
///
/// After checking all well-known config filenames, also checks for a
/// `"stylelint"` field inside `package.json` (lowest priority, matching
/// Stylelint's cosmiconfig-based resolution).
pub fn find_config(start_dir: &Path) -> Option<PathBuf> {
  let mut dir = start_dir.to_path_buf();
  loop {
    for name in CONFIG_FILENAMES {
      let candidate = dir.join(name);
      if candidate.is_file() {
        return Some(candidate);
      }
    }
    // Lowest priority: check for a `"stylelint"` field in package.json.
    let pkg_path = dir.join("package.json");
    if pkg_path.is_file()
      && let Ok(content) = std::fs::read_to_string(&pkg_path)
      && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
      && pkg.get("stylelint").is_some()
    {
      return Some(pkg_path);
    }
    if !dir.pop() {
      return None;
    }
  }
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/// Load and parse a config file at the given path.
///
/// When `path` points to a `package.json`, the `"stylelint"` field is
/// extracted and used as the config.  If the field is a string it is
/// treated as `{ "extends": "<that-string>" }`.
pub fn load_config(path: &Path) -> Result<GaleConfig, ConfigError> {
  let contents = std::fs::read_to_string(path)?;
  let file_name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or_default();

  let base_dir = path.parent().unwrap_or(Path::new("."));

  let raw = if file_name == "package.json" {
    parse_package_json_stylelint(&contents)?
  } else {
    parse_config_file(file_name, &contents, Some(base_dir))?
  };
  Ok(resolve_raw(raw, base_dir))
}

/// Extract and parse the `"stylelint"` field from the contents of a
/// `package.json` file.
///
/// - If the field is an object it is deserialized as a [`ConfigFile`].
/// - If the field is a string it is treated as `{ "extends": "<value>" }`.
/// - If the field is missing an error is returned.
fn parse_package_json_stylelint(contents: &str) -> Result<ConfigFile, ConfigError> {
  let pkg: serde_json::Value = serde_json::from_str(contents)?;
  match pkg.get("stylelint") {
    Some(value) if value.is_object() => Ok(serde_json::from_value::<ConfigFile>(value.clone())?),
    Some(value) if value.is_string() => {
      let extends_str = value.as_str().unwrap().to_string();
      Ok(ConfigFile {
        extends: Some(vec![extends_str]),
        ..Default::default()
      })
    }
    Some(_) => Err(ConfigError::UnsupportedFormat(
      "package.json \"stylelint\" field must be an object or a string".to_string(),
    )),
    None => Err(ConfigError::UnsupportedFormat(
      "package.json has no \"stylelint\" field".to_string(),
    )),
  }
}

// ---------------------------------------------------------------------------
// JavaScript config parsing (module.exports = { ... })
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ESM / CJS import resolution
// ---------------------------------------------------------------------------

/// Extract import bindings from JS source.
///
/// Recognises:
/// - `import <name> from '<path>'`  (ESM default import)
/// - `const <name> = require('<path>')` (CJS require)
///
/// Returns `(variable_name, module_path)` pairs.
fn extract_imports(source: &str) -> Vec<(String, String)> {
  let mut imports = Vec::new();

  for line in source.lines() {
    let trimmed = line.trim();

    // ESM: import <name> from '<path>'
    if let Some(after_import) = trimmed.strip_prefix("import ") {
      // Skip destructured imports like `import { foo } from ...`
      let rest = after_import.trim_start();
      if rest.starts_with('{') || rest.starts_with('*') {
        continue;
      }
      // Extract: <name> from '<path>'
      if let Some(from_idx) = rest.find(" from ") {
        let var_name = rest[..from_idx].trim().to_string();
        // Validate variable name (simple identifier)
        if var_name.is_empty()
          || !var_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
          continue;
        }
        let path_part = rest[from_idx + " from ".len()..].trim();
        if let Some(path) = extract_string_literal(path_part) {
          imports.push((var_name, path));
        }
      }
    }

    // CJS: const <name> = require('<path>')
    if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
      let rest = if let Some(r) = trimmed.strip_prefix("const ") {
        r
      } else if let Some(r) = trimmed.strip_prefix("let ") {
        r
      } else if let Some(r) = trimmed.strip_prefix("var ") {
        r
      } else {
        unreachable!()
      };
      let rest = rest.trim_start();
      // Skip destructured: const { foo } = require(...)
      if rest.starts_with('{') || rest.starts_with('[') {
        continue;
      }
      // Find `= require(`
      if let Some(eq_idx) = rest.find('=') {
        let var_name = rest[..eq_idx].trim().to_string();
        if var_name.is_empty()
          || !var_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
          continue;
        }
        let after_eq = rest[eq_idx + 1..].trim();
        if let Some(inside) = after_eq.strip_prefix("require(")
          && let Some(path) = extract_string_literal(inside)
        {
          imports.push((var_name, path));
        }
      }
    }
  }

  imports
}

/// Extract a string literal value from text that starts with `'`, `"`, or a
/// backtick. Returns the content without quotes.
/// Detect re-export patterns like `module.exports = require("./path")`.
///
/// Returns the require path if the entire JS file is a re-export, i.e. the
/// exported value is a `require()` call rather than an object literal.
fn extract_reexport_require(source: &str) -> Option<String> {
  for line in source.lines() {
    let trimmed = line.trim();
    // Skip comments and empty lines
    if trimmed.is_empty()
      || trimmed.starts_with("//")
      || trimmed.starts_with("/*")
      || trimmed.starts_with('*')
      || trimmed.starts_with('"')
    {
      continue;
    }
    // Match: module.exports = require("./path")
    // or:    module.exports=require('./path');
    for prefix in &["module.exports", "exports"] {
      if let Some(rest) = trimmed.strip_prefix(prefix) {
        let rest = rest.trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
          let rest = rest.trim_start();
          if let Some(rest) = rest.strip_prefix("require(") {
            let rest = rest.trim_start();
            if let Some(path) = extract_string_literal(rest) {
              return Some(path);
            }
          }
        }
      }
    }
  }
  None
}

fn extract_string_literal(s: &str) -> Option<String> {
  let s = s.trim();
  let quote = s.chars().next()?;
  if quote != '\'' && quote != '"' {
    return None;
  }
  let rest = &s[1..];
  let end = rest.find(quote)?;
  Some(rest[..end].to_string())
}

/// Try to resolve an import path to an actual file on disk.
/// Tries the path as-is, then with `.js` and `.json` extensions, then as
/// a directory with `index.js`.
fn resolve_import_path(file_dir: &Path, rel_path: &str) -> Option<PathBuf> {
  let base = file_dir.join(rel_path);

  // Try as-is first
  if base.is_file() {
    return Some(base);
  }

  // Try with extensions
  for ext in &[".js", ".mjs", ".cjs", ".json"] {
    let with_ext = PathBuf::from(format!("{}{}", base.display(), ext));
    if with_ext.is_file() {
      return Some(with_ext);
    }
  }

  // Try as directory with index.js
  let index = base.join("index.js");
  if index.is_file() {
    return Some(index);
  }

  None
}

/// Resolve an npm package import by walking up the directory tree looking for
/// `node_modules/<package>/`.  Tries the package's `main` field, then
/// `index.js`, then `index.cjs`.
#[allow(dead_code)]
fn resolve_npm_package_path(file_dir: &Path, package: &str) -> Option<PathBuf> {
  // Split into package name and optional subpath for exports resolution
  let (pkg_name, subpath) = split_npm_package_subpath(package);

  let mut dir = file_dir.to_path_buf();
  loop {
    let candidate = dir.join("node_modules").join(pkg_name);
    if candidate.is_dir() {
      // Try package.json "exports" field first, then "main"
      let pkg_json = candidate.join("package.json");
      if pkg_json.is_file()
        && let Ok(contents) = std::fs::read_to_string(&pkg_json)
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&contents)
      {
        // Try exports field
        let exports_subpath = subpath.unwrap_or(".");
        if let Some(resolved) = resolve_package_exports(&parsed, exports_subpath) {
          let export_path = candidate.join(&resolved);
          if export_path.is_file() {
            return Some(export_path);
          }
        }
        // Try main field (only when no subpath)
        if subpath.is_none()
          && let Some(main) = parsed.get("main").and_then(|v| v.as_str())
        {
          let main_path = candidate.join(main);
          if main_path.is_file() {
            return Some(main_path);
          }
        }
      }

      if subpath.is_none() {
        // Try index.js, index.cjs
        for name in &["index.js", "index.cjs", "index.mjs"] {
          let idx = candidate.join(name);
          if idx.is_file() {
            return Some(idx);
          }
        }
      }
    }

    // Also try as a file directly (e.g. scoped package with subpath)
    let candidate_file = dir.join("node_modules").join(package);
    for ext in &["", ".js", ".cjs", ".json", ".mjs"] {
      let with_ext = PathBuf::from(format!("{}{}", candidate_file.display(), ext));
      if with_ext.is_file() {
        return Some(with_ext);
      }
    }
    if !dir.pop() {
      return None;
    }
  }
}

/// Extract the default export value from a JS source file.
///
/// Handles:
/// - `export default <value>`
/// - `module.exports = <value>`
/// - `const X = <value>; export default X` (variable indirection)
fn extract_default_export(source: &str) -> Option<String> {
  // First try direct `export default <literal>` or `module.exports = <literal>`
  let export_markers: &[&str] = &["export default ", "module.exports =", "module.exports="];

  for marker in export_markers {
    if let Some(pos) = source.find(marker) {
      let after = source[pos + marker.len()..].trim_start();
      // If it starts with a literal value (array or object), extract it
      if after.starts_with('[')
        && let Some(val) = extract_balanced(after, '[', ']')
      {
        return Some(val);
      }
      if after.starts_with('{')
        && let Some(val) = extract_balanced(after, '{', '}')
      {
        return Some(val);
      }
      // Otherwise it might be a variable name: `export default X`
      // Find the identifier
      let ident_end = after
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(after.len());
      let ident = after[..ident_end].trim();
      if !ident.is_empty() {
        // Look for `const <ident> = <value>` in the source
        if let Some(val) = find_variable_value(source, ident) {
          return Some(val);
        }
      }
    }
  }

  None
}

/// Find the value assigned to a `const`/`let`/`var` variable in JS source.
///
/// Handles object literals (`{…}`), array literals (`[…]`), and scalar values
/// (e.g. `null`, `true`, `false`, numbers, quoted strings).
fn find_variable_value(source: &str, var_name: &str) -> Option<String> {
  for keyword in &["const ", "let ", "var "] {
    let pattern = format!("{}{}", keyword, var_name);
    if let Some(pos) = source.find(&pattern) {
      let after_name = &source[pos + pattern.len()..];
      let after_name = after_name.trim_start();
      if !after_name.starts_with('=') {
        continue;
      }
      let after_eq = after_name[1..].trim_start();
      if after_eq.starts_with('[') {
        return extract_balanced(after_eq, '[', ']');
      }
      if after_eq.starts_with('{') {
        return extract_balanced(after_eq, '{', '}');
      }
      // Scalar value: read up to `;` or end of line.
      let end = after_eq.find([';', '\n']).unwrap_or(after_eq.len());
      let val = after_eq[..end].trim();
      if !val.is_empty() {
        return Some(val.to_string());
      }
    }
  }
  None
}

/// Extract a balanced bracket/brace expression, respecting strings and nesting.
fn extract_balanced(s: &str, open: char, close: char) -> Option<String> {
  let mut depth = 0i32;
  let mut result = String::new();
  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  for c in s.chars() {
    if escape_next {
      result.push(c);
      escape_next = false;
      continue;
    }
    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      continue;
    }
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
    } else if !in_single && !in_template && c == '"' {
      in_double = !in_double;
    } else if !in_single && !in_double && c == '`' {
      in_template = !in_template;
    }

    if !in_single && !in_double && !in_template {
      if c == open {
        depth += 1;
      } else if c == close {
        depth -= 1;
      }
    }

    result.push(c);

    if depth == 0 {
      return Some(result);
    }
  }

  None
}

/// Resolve relative imports in JS source by reading imported files and
/// substituting variable references with the literal exported values.
fn resolve_js_imports(source: &str, file_dir: Option<&Path>) -> String {
  let file_dir = match file_dir {
    Some(d) => d,
    None => return source.to_string(),
  };

  let imports = extract_imports(source);
  if imports.is_empty() {
    return source.to_string();
  }

  let mut result = source.to_string();

  for (var_name, rel_path) in &imports {
    let import_path = if rel_path.starts_with("./") || rel_path.starts_with("../") {
      // Relative import — resolve from file_dir.
      match resolve_import_path(file_dir, rel_path) {
        Some(p) => p,
        None => continue,
      }
    } else {
      // npm package import — try to resolve it and inline only if the
      // exported value converts cleanly to valid JSON (e.g. an array of
      // strings like @github/browserslist-config).  Packages that export
      // non-serialisable JS (functions, shorthand properties, etc.) are
      // left as-is so the JS→JSON converter can substitute `null` for
      // unresolved identifiers rather than producing invalid JSON.
      let candidate = match resolve_npm_package_path(file_dir, rel_path) {
        Some(p) => p,
        None => continue,
      };
      let npm_source = match std::fs::read_to_string(&candidate) {
        Ok(s) => s,
        Err(_) => continue,
      };
      if let Some(raw_value) = extract_default_export(&npm_source) {
        let json_value = js_object_to_json(&raw_value);
        // Only inline if the result is valid JSON.
        if serde_json::from_str::<serde_json::Value>(&json_value).is_ok() {
          result = replace_whole_word(&result, var_name, &json_value);
        }
      }
      continue;
    };

    let imported_source = match std::fs::read_to_string(&import_path) {
      Ok(s) => s,
      Err(_) => continue,
    };

    if let Some(value) = extract_default_export(&imported_source) {
      // Replace occurrences of the variable name with the literal value.
      // We need to be careful to only replace whole-word occurrences
      // (not substrings of other identifiers).
      result = replace_whole_word(&result, var_name, &value);
    }
  }

  result
}

/// Replace whole-word occurrences of `word` with `replacement` in `source`.
/// A word boundary is a position where the adjacent character is not
/// alphanumeric or `_` or `$`.
fn replace_whole_word(source: &str, word: &str, replacement: &str) -> String {
  if word.is_empty() {
    return source.to_string();
  }
  let mut result = String::with_capacity(source.len());
  let mut remaining = source;

  while let Some(pos) = remaining.find(word) {
    // Check character before the match
    let before_ok = if pos == 0 {
      true
    } else {
      let ch = remaining[..pos].chars().next_back().unwrap_or('\0');
      !ch.is_alphanumeric() && ch != '_' && ch != '$'
    };
    // Check character after the match
    let after_pos = pos + word.len();
    let after_ok = if after_pos >= remaining.len() {
      true
    } else {
      let ch = remaining[after_pos..].chars().next().unwrap_or('\0');
      !ch.is_alphanumeric() && ch != '_' && ch != '$'
    };

    if before_ok && after_ok {
      result.push_str(&remaining[..pos]);
      result.push_str(replacement);
      remaining = &remaining[after_pos..];
    } else {
      result.push_str(&remaining[..after_pos]);
      remaining = &remaining[after_pos..];
    }
  }
  result.push_str(remaining);
  result
}

/// Strip `import ... from ...` and `const/let/var ... = require(...)` lines
/// from the source so they don't confuse the JS-to-JSON converter.
fn strip_import_lines(source: &str) -> String {
  let mut lines: Vec<&str> = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim();
    // Skip ESM import lines
    if trimmed.starts_with("import ") && trimmed.contains(" from ") {
      continue;
    }
    // Skip CJS require lines
    if (trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var "))
      && trimmed.contains("require(")
    {
      continue;
    }
    lines.push(line);
  }
  lines.join("\n")
}

/// Substitute scalar `const`/`let`/`var` variables in JS source.
///
/// Finds declarations like `const OFF = null;` and replaces every subsequent
/// bare-identifier usage of `OFF` (in a value position) with `null`.  This
/// allows the JS→JSON converter to handle files that use named constants for
/// rule severities or other scalar values.
fn substitute_scalar_vars(source: &str) -> String {
  // Collect all scalar variable declarations: `const/let/var NAME = VALUE;`
  let mut vars: Vec<(String, String)> = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim();
    for keyword in &["const ", "let ", "var "] {
      if let Some(rest) = trimmed.strip_prefix(keyword) {
        // `NAME = VALUE;` or `NAME = VALUE`
        if let Some(eq_pos) = rest.find('=') {
          let name = rest[..eq_pos].trim();
          // Only simple identifiers (no destructuring).
          if name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            && !name.is_empty()
          {
            let val = rest[eq_pos + 1..].trim();
            let val = val.strip_suffix(';').unwrap_or(val).trim();
            // Only scalar values: null, true, false, numbers, quoted strings.
            // Skip objects/arrays (handled elsewhere) and complex expressions.
            let is_scalar = val == "null"
              || val == "true"
              || val == "false"
              || val == "undefined"
              || val.parse::<f64>().is_ok()
              || (val.starts_with('\'') && val.ends_with('\''))
              || (val.starts_with('"') && val.ends_with('"'));
            if is_scalar {
              vars.push((name.to_string(), val.to_string()));
            }
          }
        }
      }
    }
  }

  if vars.is_empty() {
    return source.to_string();
  }

  // Replace bare identifier usages with their values.
  // We do a simple word-boundary replacement: replace occurrences that are
  // not preceded or followed by an alphanumeric or underscore character.
  let mut result = source.to_string();
  for (name, value) in &vars {
    let mut new_result = String::with_capacity(result.len());
    let name_len = name.len();
    let mut i = 0;
    let bytes = result.as_bytes();

    while i < bytes.len() {
      if i + name_len <= bytes.len() && &result[i..i + name_len] == name.as_str() {
        // Check word boundary before
        let before_ok = i == 0
          || !(bytes[i - 1] as char).is_alphanumeric()
            && bytes[i - 1] != b'_'
            && bytes[i - 1] != b'$';
        // Check word boundary after
        let after_ok = i + name_len >= bytes.len()
          || !(bytes[i + name_len] as char).is_alphanumeric()
            && bytes[i + name_len] != b'_'
            && bytes[i + name_len] != b'$';

        // Don't replace in declaration context (const NAME =)
        let in_decl = {
          let prefix = &result[..i];
          let trimmed = prefix.trim_end();
          trimmed.ends_with("const") || trimmed.ends_with("let") || trimmed.ends_with("var")
        };

        if before_ok && after_ok && !in_decl {
          new_result.push_str(value);
          i += name_len;
          continue;
        }
      }
      new_result.push(bytes[i] as char);
      i += 1;
    }
    result = new_result;
  }

  result
}

/// Parse a JavaScript config file that uses `module.exports = { ... }`,
/// `exports = { ... }`, or `export default { ... }` with a static object literal.
///
/// This does NOT execute JavaScript — it extracts the object literal and converts
/// JS object syntax into valid JSON before deserializing.
///
/// When `file_dir` is provided, relative imports (`./` or `../`) are resolved
/// by reading the imported file and inlining the exported value.
fn parse_js_config(source: &str, file_dir: Option<&Path>) -> Result<ConfigFile, ConfigError> {
  // Handle re-export pattern: `module.exports = require("./relative")`
  // This is common in npm config packages where index.js delegates to another file.
  if let Some(file_dir) = file_dir
    && let Some(reexport_path) = extract_reexport_require(source)
    && (reexport_path.starts_with("./") || reexport_path.starts_with("../"))
    && let Some(resolved) = resolve_import_path(file_dir, &reexport_path)
    && let Ok(contents) = std::fs::read_to_string(&resolved)
  {
    return parse_js_config(&contents, resolved.parent());
  }

  // Pre-process: resolve relative imports and inline their exported values.
  let source = resolve_js_imports(source, file_dir);
  // Strip remaining import/require lines that would confuse the JSON converter.
  let source = strip_import_lines(&source);
  // Substitute scalar const/let/var variables (e.g. `const OFF = null;`)
  // so that bare identifiers in the exported object get replaced with their
  // literal values before the JS→JSON conversion.
  let source = substitute_scalar_vars(&source);

  // Find the start of the exported object literal.
  let markers = ["module.exports", "exports", "export default"];

  let mut obj_start: Option<usize> = None;

  for marker in &markers {
    if let Some(pos) = source.find(marker) {
      // Find the `=` after the marker (for module.exports / exports), or
      // no `=` for `export default`.
      let after_marker = pos + marker.len();
      let rest = &source[after_marker..];
      let brace_search = if *marker == "export default" {
        rest
      } else {
        // Skip past the `=`
        if let Some(eq_pos) = rest.find('=') {
          &rest[eq_pos + 1..]
        } else {
          continue;
        }
      };

      let trimmed = brace_search.trim_start();

      // Check if the export is a variable reference (e.g. `export default config`)
      // rather than a direct object literal.
      if !trimmed.starts_with('{')
        && trimmed
          .chars()
          .next()
          .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
      {
        let ident_end = trimmed
          .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
          .unwrap_or(trimmed.len());
        let var_name = &trimmed[..ident_end];
        // Look for `const <var_name> = {` in the source
        if let Some(value) = find_variable_value(&source, var_name)
          && value.starts_with('{')
        {
          // Replace the variable reference with the inlined object
          let source = format!(
            "{}{}{}",
            &source[..pos],
            if *marker == "export default" {
              "export default "
            } else {
              "module.exports = "
            },
            value
          );
          let new_marker_pos = pos;
          let new_after = new_marker_pos
            + if *marker == "export default" {
              "export default ".len()
            } else {
              "module.exports = ".len()
            };
          if let Some(brace_offset) = source[new_after..].find('{') {
            // We need to use the modified source for extraction
            let extracted =
              extract_braced_object(&source[new_after + brace_offset..]).ok_or_else(|| {
                ConfigError::UnsupportedFormat(
                  "failed to extract object literal from JS config".to_string(),
                )
              })?;
            let json = js_object_to_json(&extracted);
            return serde_json::from_str::<ConfigFile>(&json).map_err(ConfigError::from);
          }
        }
      }

      // Find the opening `{`
      if let Some(brace_offset) = brace_search.find('{') {
        let absolute_offset = source.len() - brace_search.len() + brace_offset;
        obj_start = Some(absolute_offset);
        break;
      }
    }
  }

  let start = obj_start.ok_or_else(|| {
    ConfigError::UnsupportedFormat(
      "no module.exports/export default object found in JS config".to_string(),
    )
  })?;

  // Extract from opening `{` to the matching `}`, respecting strings and comments.
  let extracted = extract_braced_object(&source[start..]).ok_or_else(|| {
    ConfigError::UnsupportedFormat("failed to extract object literal from JS config".to_string())
  })?;

  // Convert JS object syntax to JSON.
  let json = js_object_to_json(&extracted);

  serde_json::from_str::<ConfigFile>(&json).map_err(ConfigError::from)
}

/// Extract a brace-balanced substring starting from the opening `{`.
/// Returns the substring including the outer braces.
fn extract_braced_object(s: &str) -> Option<String> {
  let mut depth = 0i32;
  let mut result = String::new();
  let mut chars = s.chars().peekable();
  let mut in_single_quote = false;
  let mut in_double_quote = false;
  let mut in_template = false;
  let mut escape_next = false;

  while let Some(c) = chars.next() {
    if escape_next {
      result.push(c);
      escape_next = false;
      continue;
    }

    if c == '\\' && (in_single_quote || in_double_quote || in_template) {
      result.push(c);
      escape_next = true;
      continue;
    }

    // String handling
    if !in_double_quote && !in_template && c == '\'' {
      in_single_quote = !in_single_quote;
      result.push(c);
      continue;
    }
    if !in_single_quote && !in_template && c == '"' {
      in_double_quote = !in_double_quote;
      result.push(c);
      continue;
    }
    if !in_single_quote && !in_double_quote && c == '`' {
      in_template = !in_template;
      result.push(c);
      continue;
    }

    if in_single_quote || in_double_quote || in_template {
      result.push(c);
      continue;
    }

    // Line comments
    if c == '/' {
      if chars.peek() == Some(&'/') {
        // Skip until end of line
        for nc in chars.by_ref() {
          if nc == '\n' {
            break;
          }
        }
        result.push(' '); // replace comment with space
        continue;
      }
      if chars.peek() == Some(&'*') {
        // Block comment — skip until */
        chars.next(); // consume *
        loop {
          match chars.next() {
            Some('*') if chars.peek() == Some(&'/') => {
              chars.next(); // consume /
              break;
            }
            None => break,
            _ => {}
          }
        }
        result.push(' ');
        continue;
      }
    }

    if c == '{' {
      depth += 1;
    } else if c == '}' {
      depth -= 1;
    }

    result.push(c);

    if depth == 0 {
      return Some(result);
    }
  }

  None // unbalanced braces
}

/// Convert a JS object literal string to valid JSON.
///
/// Handles:
/// - Arrow function values → replaced with `null`
/// - Single-quoted strings → double-quoted
/// - Unquoted keys → double-quoted
/// - Trailing commas → removed
/// - Spread operator entries (`...foo`) → skipped
fn js_object_to_json(js: &str) -> String {
  // Step 0a: Replace arrow function expressions with null (before quote
  // conversion so we can still distinguish template literals).
  let s = replace_arrow_functions(js);

  // Step 0b: Remove method calls on arrays/values (e.g. `.map( require.resolve )`).
  let s = remove_method_calls(&s);

  // Step 0c: Replace RegExp literals with their string pattern representation
  // (e.g. `/^[a-z]+$/i` → `"^[a-z]+$"`). Must run before quote conversion.
  let s = replace_regexp_literals(&s);

  // Step 1: Replace single-quoted strings with double-quoted strings.
  let s = convert_single_to_double_quotes(&s);

  // Step 1b: Concatenate adjacent string literals joined by `+`
  // (e.g. `"foo" + "bar"` → `"foobar"`). Must run after quote conversion.
  let s = concat_adjacent_strings(&s);

  // Step 2: Quote unquoted keys.
  let s = quote_unquoted_keys(&s);

  // Step 3: Remove trailing commas.
  let s = remove_trailing_commas(&s);

  // Step 4: Remove spread entries.
  let s = remove_spread_entries(&s);

  // Step 5: Replace bare identifier values with null.
  // After all other transformations, any remaining bare identifiers in value
  // positions (e.g. `"customSyntax": postcssScss`) are unresolved variable
  // references.  Replace them with `null` so the JSON is valid.
  replace_bare_identifier_values(&s)
}

/// Replace JavaScript RegExp literals with their pattern as a JSON string.
///
/// For example, `/^[a-z]+$/i` becomes `"^[a-z]+$"` (flags are dropped since
/// Stylelint regex options are always strings).
///
/// We distinguish RegExp `/` from division by checking the preceding non-
/// whitespace character: a regex can only appear after `:`, `[`, `,`, `(`, `=`,
/// `!`, `|`, `&`, `?`, `;`, `{`, or at the start of input — i.e. positions
/// where a value (not an operator) is expected.
fn replace_regexp_literals(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;

  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    // String state tracking
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_template && c == '"' {
      in_double = !in_double;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_double && c == '`' {
      in_template = !in_template;
      result.push(c);
      i += 1;
      continue;
    }
    if in_single || in_double || in_template {
      result.push(c);
      i += 1;
      continue;
    }

    // Check for line comments (//) and block comments (/*) — not regex.
    if c == '/' && i + 1 < len && (chars[i + 1] == '/' || chars[i + 1] == '*') {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect potential regex literal.
    if c == '/' {
      // Look back at the last non-whitespace character in result.
      let prev = result.chars().rev().find(|ch| !ch.is_ascii_whitespace());
      let is_regex_context = match prev {
        None => true, // start of input
        Some(ch) => matches!(
          ch,
          ':'
            | '['
            | ','
            | '('
            | '='
            | '!'
            | '|'
            | '&'
            | '?'
            | ';'
            | '{'
            | '+'
            | '-'
            | '~'
            | '^'
            | '%'
            | '<'
            | '>'
        ),
      };

      if is_regex_context {
        // Parse the regex literal: collect chars until unescaped `/`.
        let mut pattern = String::new();
        i += 1; // skip opening `/`
        let mut regex_escape = false;
        let mut in_char_class = false;
        let mut valid = true;

        while i < len {
          let rc = chars[i];
          if regex_escape {
            pattern.push(rc);
            regex_escape = false;
            i += 1;
            continue;
          }
          if rc == '\\' {
            pattern.push(rc);
            regex_escape = true;
            i += 1;
            continue;
          }
          if rc == '[' {
            in_char_class = true;
            pattern.push(rc);
            i += 1;
            continue;
          }
          if rc == ']' && in_char_class {
            in_char_class = false;
            pattern.push(rc);
            i += 1;
            continue;
          }
          if rc == '/' && !in_char_class {
            i += 1; // skip closing `/`
            break;
          }
          if rc == '\n' {
            // Newline inside regex => not actually a regex.
            valid = false;
            break;
          }
          pattern.push(rc);
          i += 1;
        }

        if !valid || pattern.is_empty() {
          // Not a valid regex; emit the `/` and continue.
          result.push('/');
          // Reset i to after the opening `/` — we already advanced.
          // But `pattern` consumed chars, so we need to backtrack.
          // Simplest: just push the pattern back and the `/`.
          result.push_str(&pattern);
          continue;
        }

        // Skip optional flags (g, i, m, s, u, y, d, v)
        while i < len && chars[i].is_ascii_alphabetic() {
          i += 1;
        }

        // Emit the pattern as a double-quoted JSON string.
        // Escape any double-quotes and backslashes for JSON.
        result.push('"');
        for pc in pattern.chars() {
          if pc == '"' || pc == '\\' {
            result.push('\\');
          }
          result.push(pc);
        }
        result.push('"');
        continue;
      }
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Concatenate adjacent string literals joined by `+`.
///
/// After quote conversion, strings are double-quoted.  This function finds
/// patterns like `"foo" + "bar"` and merges them into `"foobar"`.
fn concat_adjacent_strings(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;

  while i < len {
    let c = chars[i];

    // When we see a closing `"`, check if a `+` follows with another string.
    if c == '"' {
      // Collect the entire string (including opening quote).
      result.push('"');
      i += 1;
      // Scan to closing quote
      while i < len {
        let sc = chars[i];
        if sc == '\\' && i + 1 < len {
          result.push(sc);
          result.push(chars[i + 1]);
          i += 2;
          continue;
        }
        if sc == '"' {
          // Before pushing closing quote, check for `+ "..."`
          let mut j = i + 1;
          // Skip whitespace
          while j < len && chars[j].is_ascii_whitespace() {
            j += 1;
          }
          if j < len && chars[j] == '+' {
            j += 1;
            while j < len && chars[j].is_ascii_whitespace() {
              j += 1;
            }
            if j < len && chars[j] == '"' {
              // Skip the closing `"`, `+`, whitespace, and opening `"`
              // of the next string — effectively merging them.
              i = j + 1;
              continue; // continue scanning the merged string
            }
          }
          // No concatenation — emit closing quote.
          result.push('"');
          i += 1;
          break;
        }
        result.push(sc);
        i += 1;
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Replace arrow function expressions with `null`.
///
/// Detects `=>` outside of strings and backtracks to remove the entire
/// parameter list, then skips the function body (which may be a template
/// literal, string, block `{…}`, or other expression up to the next `,` or
/// `}` at the same brace depth).
fn replace_arrow_functions(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;

  // Track whether we're inside a string so we don't match `=>` in strings.
  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    // String state tracking
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_template && c == '"' {
      in_double = !in_double;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_double && c == '`' {
      in_template = !in_template;
      result.push(c);
      i += 1;
      continue;
    }
    if in_single || in_double || in_template {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect `=>` outside of strings
    if c == '=' && i + 1 < len && chars[i + 1] == '>' {
      // Backtrack in `result` to remove the arrow function parameter list.
      // The params end with `)` (possibly with whitespace before `=>`),
      // or it's a bare identifier.
      // Remove trailing whitespace first.
      let trimmed = result.trim_end().len();
      result.truncate(trimmed);

      // Capture the parameter name while removing it from result.
      let param_name: String = if result.ends_with(')') {
        // Remove the parenthesized parameter list by finding the matching `(`.
        let mut removed = Vec::new();
        let mut depth = 0i32;
        while let Some(ch) = result.pop() {
          if ch == ')' {
            depth += 1;
          } else if ch == '(' {
            depth -= 1;
            if depth == 0 {
              break;
            }
          }
          if depth > 0 {
            removed.push(ch);
          }
        }
        removed.reverse();
        removed.iter().collect::<String>().trim().to_string()
      } else {
        // Bare identifier: remove word characters
        let mut removed = Vec::new();
        while result.ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
          if let Some(ch) = result.pop() {
            removed.push(ch);
          }
        }
        removed.reverse();
        removed.iter().collect()
      };

      // Skip past `=>`
      i += 2;

      // Skip whitespace
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }

      // Check if body is a template literal
      if i < len && chars[i] == '`' {
        // Extract template literal content
        i += 1; // skip opening backtick
        let mut template_content = String::new();
        let mut tmpl_depth = 0i32;
        while i < len {
          if chars[i] == '\\' && i + 1 < len {
            template_content.push(chars[i]);
            template_content.push(chars[i + 1]);
            i += 2;
            continue;
          }
          if chars[i] == '$' && i + 1 < len && chars[i + 1] == '{' {
            tmpl_depth += 1;
            template_content.push('$');
            template_content.push('{');
            i += 2;
            continue;
          }
          if chars[i] == '}' && tmpl_depth > 0 {
            tmpl_depth -= 1;
            template_content.push('}');
            i += 1;
            continue;
          }
          if chars[i] == '`' && tmpl_depth == 0 {
            i += 1; // skip closing backtick
            break;
          }
          template_content.push(chars[i]);
          i += 1;
        }

        // Replace ${paramName} with ${name} to standardize placeholder
        let standardized = if !param_name.is_empty() && param_name != "name" {
          let placeholder = format!("${{{}}}", param_name);
          template_content.replace(&placeholder, "${name}")
        } else {
          template_content
        };

        // Escape double quotes in content
        let escaped = standardized.replace('"', "\\\"");

        // Emit as JSON double-quoted string
        result.push('"');
        result.push_str(&escaped);
        result.push('"');
      } else {
        // Skip the arrow function body.
        if i < len {
          i = skip_js_expression(&chars, i);
        }

        // Emit `null` in place of the arrow function.
        result.push_str("null");
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Skip a single JS expression starting at position `i`.
/// Returns the index just past the expression.
/// Handles:
/// - Template literals (`` ` ... ` ``)
/// - String literals (`'...'` or `"..."`)
/// - Block bodies (`{ ... }`)
/// - Array literals (`[ ... ]`)
/// - Parenthesized expressions (`( ... )`)
/// - Simple expressions (until `,`, `}`, `]` at depth 0, or newline
///   followed by a key-like pattern)
fn skip_js_expression(chars: &[char], start: usize) -> usize {
  let len = chars.len();
  let mut i = start;

  if i >= len {
    return i;
  }

  match chars[i] {
    '`' => {
      // Template literal — skip to matching backtick, handling `${…}`.
      i += 1;
      let mut tmpl_depth = 0i32; // for nested `${…}`
      while i < len {
        if chars[i] == '\\' {
          i += 2; // skip escaped char
          continue;
        }
        if chars[i] == '$' && i + 1 < len && chars[i + 1] == '{' {
          tmpl_depth += 1;
          i += 2;
          continue;
        }
        if chars[i] == '}' && tmpl_depth > 0 {
          tmpl_depth -= 1;
          i += 1;
          continue;
        }
        if chars[i] == '`' && tmpl_depth == 0 {
          i += 1; // past closing backtick
          break;
        }
        i += 1;
      }
    }
    '\'' | '"' => {
      let quote = chars[i];
      i += 1;
      while i < len {
        if chars[i] == '\\' {
          i += 2;
          continue;
        }
        if chars[i] == quote {
          i += 1;
          break;
        }
        i += 1;
      }
    }
    '{' => {
      i = skip_balanced(chars, i, '{', '}');
    }
    '(' => {
      i = skip_balanced(chars, i, '(', ')');
    }
    '[' => {
      i = skip_balanced(chars, i, '[', ']');
    }
    _ => {
      // Simple expression — advance until `,`, `}`, `]` at depth 0.
      let mut depth = 0i32;
      while i < len {
        match chars[i] {
          '(' | '[' | '{' => depth += 1,
          ')' | ']' | '}' => {
            if depth == 0 {
              break;
            }
            depth -= 1;
          }
          ',' if depth == 0 => break,
          _ => {}
        }
        i += 1;
      }
    }
  }

  i
}

/// Skip a balanced pair of delimiters (e.g. `{…}`, `(…)`, `[…]`),
/// respecting strings and nesting. Returns the index just past the
/// closing delimiter.
fn skip_balanced(chars: &[char], start: usize, open: char, close: char) -> usize {
  let len = chars.len();
  let mut i = start;
  let mut depth = 0i32;
  let mut in_sq = false;
  let mut in_dq = false;
  let mut in_tmpl = false;
  let mut esc = false;

  while i < len {
    let c = chars[i];
    if esc {
      esc = false;
      i += 1;
      continue;
    }
    if c == '\\' && (in_sq || in_dq || in_tmpl) {
      esc = true;
      i += 1;
      continue;
    }
    if !in_dq && !in_tmpl && c == '\'' {
      in_sq = !in_sq;
    } else if !in_sq && !in_tmpl && c == '"' {
      in_dq = !in_dq;
    } else if !in_sq && !in_dq && c == '`' {
      in_tmpl = !in_tmpl;
    }
    if !in_sq && !in_dq && !in_tmpl {
      if c == open {
        depth += 1;
      } else if c == close {
        depth -= 1;
        if depth == 0 {
          return i + 1;
        }
      }
    }
    i += 1;
  }
  i
}

/// Remove method calls like `.map(…)` or `.filter(…)` that appear after `]`
/// (array literals) or after identifiers. These are JS-only constructs that
/// have no JSON equivalent. The method call and its argument are simply
/// dropped so the preceding array value remains intact.
fn remove_method_calls(s: &str) -> String {
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut result = String::with_capacity(len);
  let mut i = 0;
  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }
    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_template && c == '"' {
      in_double = !in_double;
      result.push(c);
      i += 1;
      continue;
    }
    if !in_single && !in_double && c == '`' {
      in_template = !in_template;
      result.push(c);
      i += 1;
      continue;
    }
    if in_single || in_double || in_template {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect `.identifier(` pattern — a method call.
    if c == '.' && i + 1 < len && chars[i + 1].is_ascii_alphabetic() {
      // Check if this looks like a method call by scanning for an identifier
      // followed by `(`.
      let mut j = i + 1;
      while j < len && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
        j += 1;
      }
      // Skip whitespace between identifier and `(`
      let mut k = j;
      while k < len && chars[k].is_ascii_whitespace() {
        k += 1;
      }
      if k < len && chars[k] == '(' {
        // This is a method call — skip `.identifier(…)`
        let end = skip_balanced(&chars, k, '(', ')');
        i = end;
        continue;
      }
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Convert single-quoted strings to double-quoted strings.
/// Handles escaping: internal `'` becomes `\'` → `"` becomes itself,
/// and internal unescaped `"` gets escaped.
fn convert_single_to_double_quotes(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let mut chars = s.chars().peekable();
  let mut in_double_quote = false;
  let mut escape_next = false;

  while let Some(c) = chars.next() {
    if escape_next {
      result.push(c);
      escape_next = false;
      continue;
    }

    // Convert template literals (backtick strings) to double-quoted
    // strings, same as single quotes.  Template literals in config files
    // are virtually always plain strings without `${...}` interpolation.
    if c == '`' && !in_double_quote {
      result.push('"');
      // Collect until closing backtick.
      loop {
        match chars.next() {
          None => break,
          Some('\\') => {
            if let Some(ec) = chars.next() {
              if ec == '`' {
                result.push('`');
              } else {
                result.push('\\');
                result.push(ec);
              }
            }
          }
          Some('`') => {
            result.push('"');
            break;
          }
          Some('"') => {
            // Escape inner double quotes.
            result.push('\\');
            result.push('"');
          }
          Some(ch) => result.push(ch),
        }
      }
      continue;
    }

    if c == '\\' && in_double_quote {
      result.push(c);
      escape_next = true;
      continue;
    }

    if c == '"' && !in_double_quote {
      // Not inside a double-quoted string, not starting one unless context says so
      // Check if we're about to start a double-quoted string
      in_double_quote = true;
      result.push('"');
      continue;
    }

    if c == '"' && in_double_quote {
      in_double_quote = false;
      result.push('"');
      continue;
    }

    if in_double_quote {
      if c == '\\' {
        escape_next = true;
      }
      result.push(c);
      continue;
    }

    // Outside any string context
    if c == '\'' {
      // Start of single-quoted string — collect until closing '
      result.push('"');
      loop {
        match chars.next() {
          None => break,
          Some('\\') => {
            // Next char is escaped
            if let Some(ec) = chars.next() {
              if ec == '\'' {
                // Escaped single quote — just emit the quote
                result.push('\'');
              } else {
                result.push('\\');
                result.push(ec);
              }
            }
          }
          Some('\'') => {
            // End of single-quoted string
            result.push('"');
            break;
          }
          Some('"') => {
            // Unescaped double quote inside single-quoted string — escape it
            result.push('\\');
            result.push('"');
          }
          Some(ch) => result.push(ch),
        }
      }
      continue;
    }

    result.push(c);
  }

  result
}

/// Add double quotes around unquoted object keys.
/// An unquoted key looks like `identifier:` or `identifier :` at the start
/// of a line or after `{` or `,`.
fn quote_unquoted_keys(s: &str) -> String {
  // Use a regex-like approach: scan for patterns like `word :` or `word:`
  // that are not inside strings.
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    // Check if we're at the start of an unquoted key.
    // An unquoted key is an identifier ([a-zA-Z_$][a-zA-Z0-9_$-]*)
    // followed by optional whitespace and a `:`.
    if c.is_ascii_alphabetic() || c == '_' || c == '$' {
      // Check what came before — should be start, `{`, `,`, or whitespace/newline after one of those.
      let before_is_valid = {
        let trimmed_before = result.trim_end();
        trimmed_before.is_empty()
          || trimmed_before.ends_with('{')
          || trimmed_before.ends_with(',')
          || trimmed_before.ends_with('\n')
      };

      if before_is_valid {
        // Collect the identifier
        let key_start = i;
        while i < len
          && (chars[i].is_ascii_alphanumeric()
            || chars[i] == '_'
            || chars[i] == '$'
            || chars[i] == '-')
        {
          i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();

        // Skip whitespace
        let ws_start = i;
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }

        // Check for `:`
        if i < len && chars[i] == ':' {
          // It's an unquoted key — quote it
          result.push('"');
          result.push_str(&key);
          result.push('"');
          // Push any whitespace that was between key and `:`
          let ws: String = chars[ws_start..i].iter().collect();
          result.push_str(&ws);
          result.push(':');
          i += 1; // skip the `:`
          continue;
        } else {
          // Not a key — push as-is
          result.push_str(&key);
          let ws: String = chars[ws_start..i].iter().collect();
          result.push_str(&ws);
          continue;
        }
      }
    }

    result.push(c);
    i += 1;
  }
  result
}

/// Remove trailing commas before `}` or `]`.
fn remove_trailing_commas(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    if c == ',' {
      // Look ahead past whitespace for `}` or `]`
      let mut j = i + 1;
      while j < len && chars[j].is_ascii_whitespace() {
        j += 1;
      }
      if j < len && (chars[j] == '}' || chars[j] == ']') {
        // Skip this comma (trailing comma)
        i += 1;
        continue;
      }
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Remove spread operator entries like `...something` from object/array literals.
fn remove_spread_entries(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    // Detect `...identifier`
    if c == '.' && i + 2 < len && chars[i + 1] == '.' && chars[i + 2] == '.' {
      // Skip `...` and the following identifier
      i += 3;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
        i += 1;
      }
      // Also skip a trailing comma if present
      // Skip whitespace first
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if i < len && chars[i] == ',' {
        i += 1;
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Replace bare identifier values with `null` in a JSON-like string.
///
/// After all other JS→JSON transformations, any remaining bare identifiers in
/// value positions (e.g. `"customSyntax": postcssScss`) are unresolved variable
/// references.  This function replaces them with `null` so that `serde_json`
/// can parse the result.
///
/// Only replaces identifiers that appear after `:` (value position) or as array
/// elements (after `[` or `,`).  Does not touch keys (already quoted), strings,
/// or JSON literals (`true`, `false`, `null`).
fn replace_bare_identifier_values(s: &str) -> String {
  let mut result = String::with_capacity(s.len());
  let chars: Vec<char> = s.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_string = false;
  let mut escape_next = false;

  while i < len {
    let c = chars[i];

    if escape_next {
      result.push(c);
      escape_next = false;
      i += 1;
      continue;
    }

    if c == '\\' && in_string {
      result.push(c);
      escape_next = true;
      i += 1;
      continue;
    }

    if c == '"' {
      in_string = !in_string;
      result.push(c);
      i += 1;
      continue;
    }

    if in_string {
      result.push(c);
      i += 1;
      continue;
    }

    // Check for a bare identifier in a value position.
    // Value positions come after `:`, after `[`, or after `,`.
    if (c.is_alphabetic() || c == '_' || c == '$') && !is_preceded_by_quote(&result) {
      // Read the full identifier.
      let start = i;
      while i < len
        && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$' || chars[i] == '.')
      {
        i += 1;
      }
      let ident = &s[start..i];
      // Preserve JSON literals.
      match ident {
        "true" | "false" | "null" => result.push_str(ident),
        _ => result.push_str("null"),
      }
      continue;
    }

    result.push(c);
    i += 1;
  }

  result
}

/// Check if the last non-whitespace character in `s` indicates we're in a
/// value position (after `:`, `[`, or `,`).
fn is_preceded_by_quote(s: &str) -> bool {
  // Returns true if the last non-whitespace char is `"`, indicating this
  // identifier is likely a key or part of a string context.
  // Returns false if it's `:`, `[`, `,`, or other value-context chars.
  let trimmed = s.trim_end();
  trimmed.ends_with('"')
}

fn parse_config_file(
  file_name: &str,
  contents: &str,
  file_dir: Option<&Path>,
) -> Result<ConfigFile, ConfigError> {
  if file_name.ends_with(".json") || file_name == "gale.json" {
    Ok(serde_json::from_str(contents)?)
  } else if file_name.ends_with(".toml") || file_name == "gale.toml" {
    Ok(toml::from_str(contents)?)
  } else if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
    Ok(serde_yaml::from_str(contents)?)
  } else if file_name.ends_with(".js") || file_name.ends_with(".mjs") || file_name.ends_with(".cjs")
  {
    parse_js_config(contents, file_dir)
  } else if file_name == ".stylelintrc" {
    // Try JSON first, fall back to YAML.
    serde_json::from_str(contents)
      .map_err(ConfigError::from)
      .or_else(|_| serde_yaml::from_str(contents).map_err(ConfigError::from))
  } else {
    Err(ConfigError::UnsupportedFormat(file_name.to_string()))
  }
}

/// Read a file and parse it as a [`ConfigFile`], inferring format from the file name.
fn load_config_file(path: &Path) -> Result<ConfigFile, ConfigError> {
  let contents = std::fs::read_to_string(path)?;
  let file_name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or_default();
  let file_dir = path.parent();

  // Handle package.json specially: extract the "stylelint" field.
  if file_name == "package.json" {
    return parse_package_json_stylelint(&contents);
  }

  // For files without a recognised extension (e.g. bare `.stylelintrc`),
  // or JSON files, try JSON first then YAML as a fallback.
  if file_name.ends_with(".json") {
    Ok(serde_json::from_str(&contents)?)
  } else if file_name.ends_with(".toml") {
    Ok(toml::from_str(&contents)?)
  } else if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
    Ok(serde_yaml::from_str(&contents)?)
  } else if file_name.ends_with(".js") || file_name.ends_with(".mjs") || file_name.ends_with(".cjs")
  {
    parse_js_config(&contents, file_dir)
  } else {
    // Unknown extension — try JSON, then YAML.
    serde_json::from_str(&contents)
      .map_err(ConfigError::from)
      .or_else(|_| serde_yaml::from_str(&contents).map_err(ConfigError::from))
  }
}

// ---------------------------------------------------------------------------
// npm / node_modules resolution
// ---------------------------------------------------------------------------

/// Walk upward from `start_dir` looking for a `node_modules` directory.
fn find_node_modules(start_dir: &Path) -> Option<PathBuf> {
  let mut dir = start_dir.to_path_buf();
  loop {
    let nm = dir.join("node_modules");
    if nm.is_dir() {
      return Some(nm);
    }
    if !dir.pop() {
      return None;
    }
  }
}

/// Resolve a subpath from a package.json `exports` field.
///
/// Handles these `exports` shapes:
/// - `"exports": { "./sub": "./sub.mjs" }` (subpath → string)
/// - `"exports": { "./sub": { "default": "./sub.mjs" } }` (subpath → condition map)
/// - `"exports": { ".": "./index.mjs" }` (main entry point)
/// - `"exports": "./index.mjs"` (sugar for `{ ".": "./index.mjs" }`)
///
/// `subpath` should be the bare subpath (e.g. `"stylelint"` not `"./stylelint"`).
/// The special value `"."` resolves the package main entry point.
fn resolve_package_exports(pkg: &serde_json::Value, subpath: &str) -> Option<String> {
  let exports = pkg.get("exports")?;

  // Normalise the lookup key: `"."` stays as-is, otherwise prepend `"./"`
  let key = if subpath == "." {
    ".".to_string()
  } else {
    format!("./{subpath}")
  };

  // `exports` can be a string (sugar for `{ ".": "<string>" }`)
  if let Some(s) = exports.as_str() {
    if key == "." {
      return Some(s.to_string());
    }
    return None;
  }

  let exports_obj = exports.as_object()?;
  let entry = exports_obj.get(&key)?;

  // The entry can be a string or a condition map
  if let Some(s) = entry.as_str() {
    return Some(s.to_string());
  }

  // Condition map: try "default", "require", "import" in that order
  if let Some(obj) = entry.as_object() {
    for condition in &["default", "require", "import"] {
      if let Some(val) = obj.get(*condition).and_then(|v| v.as_str()) {
        return Some(val.to_string());
      }
    }
  }

  None
}

/// Try to load a [`ConfigFile`] from an npm package installed in `node_modules`.
///
/// Supports subpath imports like `@scope/package/subpath` which resolves to
/// `node_modules/@scope/package/subpath.js` (or `.json`).
fn resolve_npm_config(package_name: &str, base_dir: &Path) -> Option<ConfigFile> {
  let (pkg_name, subpath) = split_npm_package_subpath(package_name);

  // Walk up directory tree trying each `node_modules` found, so that nested
  // packages (e.g. `stylelint-config-recommended-scss` inside
  // `stylelint-config-standard-scss/node_modules/`) are resolved correctly.
  let mut dir = base_dir.to_path_buf();
  loop {
    let nm = dir.join("node_modules");
    if nm.is_dir() {
      let pkg_dir = nm.join(pkg_name);
      if pkg_dir.is_dir()
        && let Some(config) = try_load_npm_pkg(&pkg_dir, subpath)
      {
        return Some(config);
      }
    }
    if !dir.pop() {
      break;
    }
  }
  None
}

/// Attempt to load a config from an already-located package directory.
fn try_load_npm_pkg(pkg_dir: &Path, subpath: Option<&str>) -> Option<ConfigFile> {
  // If there's a subpath, first try the `exports` field in package.json,
  // then fall back to direct file resolution with common extensions.
  if let Some(sub) = subpath {
    // Try `exports` field in package.json first (e.g. `"./stylelint": "./stylelint.mjs"`)
    let pkg_json_path = pkg_dir.join("package.json");
    if let Ok(pkg_json) = std::fs::read_to_string(&pkg_json_path)
      && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&pkg_json)
      && let Some(resolved) = resolve_package_exports(&pkg, sub)
    {
      let export_path = pkg_dir.join(&resolved);
      if export_path.is_file()
        && let Ok(config) = load_config_file(&export_path)
      {
        return Some(config);
      }
    }
    // Fall back: try with common extensions
    for ext in &["", ".js", ".json", ".cjs", ".mjs"] {
      let path = pkg_dir.join(format!("{sub}{ext}"));
      if path.is_file()
        && let Ok(config) = load_config_file(&path)
      {
        return Some(config);
      }
    }
    return None;
  }

  // 1. Try package.json "exports" field for the main entry point (".")
  // 2. Then try "main" field
  let pkg_json_path = pkg_dir.join("package.json");
  if let Ok(pkg_json) = std::fs::read_to_string(&pkg_json_path)
    && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&pkg_json)
  {
    // Try `exports["."]` first
    if let Some(resolved) = resolve_package_exports(&pkg, ".") {
      let export_path = pkg_dir.join(&resolved);
      if export_path.is_file()
        && let Ok(config) = load_config_file(&export_path)
      {
        return Some(config);
      }
    }
    // Try `main` field
    if let Some(main) = pkg.get("main").and_then(|m| m.as_str()) {
      let main_path = pkg_dir.join(main);
      if let Ok(config) = load_config_file(&main_path) {
        return Some(config);
      }
    }
  }

  // 2. Try common config file names
  for name in &[
    "index.json",
    "index.js",
    "stylelint.config.js",
    "stylelint.config.cjs",
    "stylelint.config.mjs",
    ".stylelintrc.json",
    ".stylelintrc",
  ] {
    let path = pkg_dir.join(name);
    if let Ok(config) = load_config_file(&path) {
      return Some(config);
    }
  }

  None
}

/// Split an npm package name into the package name and optional subpath.
///
/// Examples:
/// - `"@scope/pkg/sub/path"` → `("@scope/pkg", Some("sub/path"))`
/// - `"@scope/pkg"` → `("@scope/pkg", None)`
/// - `"pkg/sub"` → `("pkg", Some("sub"))`
/// - `"pkg"` → `("pkg", None)`
fn split_npm_package_subpath(name: &str) -> (&str, Option<&str>) {
  if let Some(rest) = name.strip_prefix('@') {
    // Scoped package: @scope/pkg[/subpath]
    // Find the second `/` (after the scope)
    if let Some(slash1) = rest.find('/') {
      let after_scope = &rest[slash1 + 1..];
      if let Some(slash2) = after_scope.find('/') {
        let pkg_end = 1 + slash1 + 1 + slash2; // "@" + "scope/" + "pkg"
        return (&name[..pkg_end], Some(&name[pkg_end + 1..]));
      }
    }
    (name, None)
  } else {
    // Unscoped package: pkg[/subpath]
    if let Some(slash) = name.find('/') {
      (&name[..slash], Some(&name[slash + 1..]))
    } else {
      (name, None)
    }
  }
}

/// Resolve a relative path (starting with `./` or `../`) to a [`ConfigFile`].
///
/// If the exact path doesn't exist, tries adding common config file extensions
/// (`.js`, `.json`, `.cjs`, `.mjs`). Also checks for `index.js` if the path
/// resolves to a directory.
fn resolve_relative_config(rel_path: &str, base_dir: &Path) -> Option<ConfigFile> {
  let path = base_dir.join(rel_path);

  // Try exact path first.
  if let Ok(config) = load_config_file(&path) {
    return Some(config);
  }

  // Try with common extensions.
  for ext in &[".js", ".json", ".cjs", ".mjs"] {
    let with_ext = base_dir.join(format!("{rel_path}{ext}"));
    if with_ext.is_file()
      && let Ok(config) = load_config_file(&with_ext)
    {
      return Some(config);
    }
  }

  // If the path is a directory, try index files.
  if path.is_dir() {
    for name in &["index.js", "index.json"] {
      let index_path = path.join(name);
      if let Ok(config) = load_config_file(&index_path) {
        return Some(config);
      }
    }
  }

  None
}

/// Recursively collect rules from a list of `extends` entries, with cycle detection.
fn collect_rules_from_extends(
  extends: &[String],
  base_dir: &Path,
  visited: &mut HashSet<String>,
) -> (HashMap<String, RuleConfig>, Vec<ConfigOverride>) {
  let mut rules = HashMap::new();
  let mut overrides: Vec<ConfigOverride> = Vec::new();

  for preset_name in extends {
    if visited.contains(preset_name) {
      continue; // cycle detection
    }
    visited.insert(preset_name.clone());

    if preset_name.starts_with("gale:") {
      // gale: presets are always built-in.
      if let Some(preset_rules) = resolve_preset(preset_name) {
        rules.extend(preset_rules);
      } else {
        eprintln!("warning: unknown preset '{preset_name}', skipping");
      }
    } else {
      // For non-gale presets: try npm/file first, fall back to built-in.
      // This ensures the real npm package (with exact options) wins over
      // our approximate built-in presets.
      let config = if preset_name.starts_with("./") || preset_name.starts_with("../") {
        resolve_relative_config(preset_name, base_dir)
      } else {
        resolve_npm_config(preset_name, base_dir)
      };

      if let Some(config) = config {
        let sub_base = if preset_name.starts_with("./") || preset_name.starts_with("../") {
          // If the path is a directory (resolved via index.js), use
          // the directory itself as the base so that relative extends
          // inside it resolve correctly.
          let joined = base_dir.join(preset_name);
          if joined.is_dir() {
            joined
          } else {
            joined.parent().unwrap_or(base_dir).to_path_buf()
          }
        } else {
          // For npm packages, use the package root directory as the
          // base for resolving relative extends within the package.
          let (pkg_name, _) = split_npm_package_subpath(preset_name);
          find_node_modules(base_dir)
            .map(|nm| nm.join(pkg_name))
            .unwrap_or_else(|| base_dir.to_path_buf())
        };

        // Recursively resolve this config's extends first
        if let Some(ref sub_extends) = config.extends {
          let (sub_rules, sub_overrides) =
            collect_rules_from_extends(sub_extends, &sub_base, visited);
          rules.extend(sub_rules);
          overrides.extend(sub_overrides);
        }
        // Then apply this config's own rules (later configs win).
        // Off rules are kept in the map (not removed) so that they
        // propagate upward through recursive extends and override
        // rules enabled by earlier extends.  They are stripped out
        // at the very end in `resolve_raw`.
        let config_rules_raw = config.rules.unwrap_or_default();
        for (name, value) in &config_rules_raw {
          let resolved = value.resolve();
          rules.insert(name.clone(), resolved);
        }
        // Collect rules explicitly set to null/Off by this config.
        // These "nullified" rules must remain off even when an
        // override from the same config re-enables them via extends.
        let null_rules: HashSet<String> = config_rules_raw
          .iter()
          .filter(|(_, v)| v.resolve().severity == Some(Severity::Off))
          .map(|(k, _)| k.clone())
          .collect();

        // Collect this config's overrides (extended configs' overrides
        // come before the user's own overrides).
        if let Some(mut config_overrides) = config.overrides {
          // Inject the parent config's null rules into each override
          // so they take precedence over the override's extends.
          // Only inject if the override doesn't explicitly set that
          // rule itself (explicit override rules still win).
          if !null_rules.is_empty() {
            for ov in &mut config_overrides {
              let ov_explicit: HashSet<String> = ov
                .rules
                .as_ref()
                .map(|r| r.keys().cloned().collect())
                .unwrap_or_default();
              let rules_map = ov.rules.get_or_insert_with(HashMap::new);
              for null_rule in &null_rules {
                if !ov_explicit.contains(null_rule) {
                  rules_map.insert(null_rule.clone(), RuleConfigValue::Null(None));
                }
              }
            }
          }
          overrides.extend(config_overrides);
        }
      } else {
        // npm/file not found — warn and skip (treat as empty config).
        // Do NOT fall back to built-in presets: approximate presets may
        // enable rules the real config would disable, causing thousands
        // of false positives.
        eprintln!("warning: could not resolve extends '{preset_name}', skipping");
      }
    }
  }

  (rules, overrides)
}

// ---------------------------------------------------------------------------
// Plugin recognition
// ---------------------------------------------------------------------------

/// Known Stylelint plugins whose rules Gale implements as built-in rules.
/// These plugins are silently accepted when found in the `plugins` config field.
const KNOWN_PLUGINS: &[&str] = &[
  "stylelint-scss",
  "stylelint-order",
  "@stylistic/stylelint-plugin",
  "stylelint-no-unsupported-browser-features",
  "stylelint-declaration-block-no-ignored-properties",
  "stylelint-value-no-unknown-custom-properties",
];

/// Rule prefixes associated with known plugins.  Used to determine whether an
/// unrecognised rule belongs to a known plugin (and thus should produce a
/// "not yet supported" warning rather than being silently dropped).
const KNOWN_PLUGIN_RULE_PREFIXES: &[&str] =
  &["scss/", "order/", "@stylistic/", "stylistic/", "csstools/"];

/// Standalone rule names from known plugins (no prefix).
const KNOWN_PLUGIN_STANDALONE_RULES: &[&str] = &[
  "plugin/no-unsupported-browser-features",
  "plugin/declaration-block-no-ignored-properties",
];

/// Extract plugin name strings from the raw `plugins` JSON value.
///
/// Stylelint's `plugins` field can be a single string or an array of strings.
/// Some entries may be paths (e.g. `./my-plugin.js`) — we normalise by
/// extracting the last path component without the extension when it looks
/// like a file path.
fn extract_plugin_names(value: &serde_json::Value) -> Vec<String> {
  let mut names = Vec::new();
  match value {
    serde_json::Value::String(s) => {
      names.push(s.clone());
    }
    serde_json::Value::Array(arr) => {
      for item in arr {
        if let serde_json::Value::String(s) = item {
          names.push(s.clone());
        }
      }
    }
    _ => {}
  }
  names
}

/// Check whether a plugin name matches one of the known plugins.
///
/// Uses substring matching so that `./node_modules/stylelint-scss` or
/// `stylelint-scss/lib/index.js` still matches `stylelint-scss`.
pub fn is_known_plugin(plugin: &str) -> bool {
  KNOWN_PLUGINS.iter().any(|known| plugin.contains(known))
}

/// Check whether a rule name looks like it belongs to a known plugin.
pub fn is_known_plugin_rule(rule_name: &str) -> bool {
  KNOWN_PLUGIN_RULE_PREFIXES
    .iter()
    .any(|prefix| rule_name.starts_with(prefix))
    || KNOWN_PLUGIN_STANDALONE_RULES.contains(&rule_name)
}

/// Convert a raw [`ConfigFile`] into the resolved [`GaleConfig`].
///
/// `base_dir` is used when resolving `extends` that reference npm packages or
/// relative paths.
fn resolve_raw(raw: ConfigFile, base_dir: &Path) -> GaleConfig {
  // 1. Start with rules from extended presets / npm configs (in order).
  //    Also collect any overrides defined in extended configs.
  let mut rules: HashMap<String, RuleConfig> = HashMap::new();
  let mut extended_overrides: Vec<ConfigOverride> = Vec::new();
  if let Some(ref extends) = raw.extends {
    let mut visited = HashSet::new();
    let (ext_rules, ext_overrides) = collect_rules_from_extends(extends, base_dir, &mut visited);
    rules = ext_rules;
    extended_overrides = ext_overrides;
  }

  // 1b. Strip rules that extended configs disabled (severity == Off).
  //     These Off entries were kept in the map during recursive extends
  //     resolution so they could propagate upward and override rules
  //     enabled by earlier extends.
  rules.retain(|_, rc| rc.severity != Some(Severity::Off));

  // 2. Overlay user rules on top — user always wins.
  let raw_rules = raw.rules.unwrap_or_default();
  for (name, value) in &raw_rules {
    let resolved = value.resolve();
    // If the user set a rule to Off, remove it from the map entirely
    // so the linter doesn't run it at all.
    if resolved.severity == Some(Severity::Off) {
      rules.remove(name);
    } else {
      rules.insert(name.clone(), resolved);
    }
  }

  // 2b. Apply `defaultSeverity` if set.
  // In Stylelint, `defaultSeverity` replaces the implicit "error" default
  // for rules that don't specify their own severity.  We apply it to all
  // rules whose resolved severity is Error and whose raw config value did
  // not explicitly contain "error"/"warning"/"off" as a severity keyword.
  let default_sev_str = raw.default_severity.as_deref();
  if let Some(ds) = default_sev_str {
    let default_sev = match ds {
      "warning" => Some(Severity::Warning),
      "error" => Some(Severity::Error),
      _ => None,
    };
    if let Some(sev) = default_sev {
      // Apply to ALL rules (both extended and user-defined).
      // Skip rules whose raw config explicitly sets a severity keyword.
      for (name, rc) in rules.iter_mut() {
        if rc.severity == Some(Severity::Error) {
          // Check if this rule's raw value explicitly specifies severity.
          let has_explicit_severity = raw_rules
            .get(name)
            .is_some_and(has_explicit_severity_in_value);
          if !has_explicit_severity {
            rc.severity = Some(sev);
          }
        }
      }
    }
  }

  // Merge both `ignorePatterns` and `ignoreFiles` (Stylelint's field name).
  let mut ignore_patterns = raw.ignore_patterns.unwrap_or_default();
  if let Some(ignore_files) = raw.ignore_files {
    ignore_patterns.extend(ignore_files);
  }

  let formatter = raw
    .formatter
    .as_deref()
    .map(|s| match s.to_lowercase().as_str() {
      "json" => FormatterType::Json,
      "compact" => FormatterType::Compact,
      _ => FormatterType::Text,
    })
    .unwrap_or_default();

  // 3. Resolve overrides.
  //    Extended configs' overrides come first, then the user's own overrides.
  let resolve_override = |ov: ConfigOverride| -> Option<ResolvedOverride> {
    let file_patterns = ov.files.unwrap_or_default();
    if file_patterns.is_empty() {
      return None;
    }

    // Start with rules from the override's extends.
    let mut ov_rules: HashMap<String, RuleConfig> = HashMap::new();
    if let Some(ref extends) = ov.extends {
      let mut visited = HashSet::new();
      let (ext_rules, _ext_overrides) = collect_rules_from_extends(extends, base_dir, &mut visited);
      ov_rules = ext_rules;
      // Note: nested overrides within an override's extends are not
      // propagated (matching Stylelint behavior).
    }

    // Overlay the override's own rules on top.
    for (name, value) in ov.rules.unwrap_or_default() {
      let resolved = value.resolve();
      if resolved.severity == Some(Severity::Off) {
        // For overrides, keep Off entries so they can remove
        // base rules when applied per-file.
        ov_rules.insert(name, resolved);
      } else {
        ov_rules.insert(name, resolved);
      }
    }

    let ignore_patterns = ov.ignore_files.unwrap_or_default();

    // Extract customSyntax as a string (if present).
    let custom_syntax = ov
      .custom_syntax
      .as_ref()
      .and_then(|v| v.as_str().map(String::from));

    Some(ResolvedOverride::new(
      file_patterns,
      ignore_patterns,
      ov_rules,
      custom_syntax,
    ))
  };

  // Combine: extended overrides first, then user overrides.
  let mut all_raw_overrides = extended_overrides;
  if let Some(user_overrides) = raw.overrides {
    all_raw_overrides.extend(user_overrides);
  }

  let overrides: Vec<ResolvedOverride> = all_raw_overrides
    .into_iter()
    .filter_map(resolve_override)
    .collect();

  // 4. Extract plugin names from the config.
  let plugins = raw
    .plugins
    .as_ref()
    .map(extract_plugin_names)
    .unwrap_or_default();

  // 5. Parse the disable-report switches.  Each accepts `true`, `false`, or
  //    Stylelint's `[bool, { except, severity }]` array form, of which only
  //    the leading boolean is honoured.
  let report_needless_disables = disable_report_enabled(raw.report_needless_disables.as_ref());
  let report_invalid_scope_disables =
    disable_report_enabled(raw.report_invalid_scope_disables.as_ref());
  let report_descriptionless_disables =
    disable_report_enabled(raw.report_descriptionless_disables.as_ref());
  let report_unscoped_disables = disable_report_enabled(raw.report_unscoped_disables.as_ref());

  // 6. Parse defaultSeverity.
  let default_severity = raw.default_severity.as_deref().and_then(|s| match s {
    "warning" => Some(Severity::Warning),
    "error" => Some(Severity::Error),
    _ => None,
  });

  // 7. Extract top-level customSyntax as a string (if present).
  let custom_syntax = raw
    .custom_syntax
    .as_ref()
    .and_then(|v| v.as_str().map(String::from));

  // 8. The CLI-equivalent switches.
  let fix = raw.fix.as_ref().and_then(fix_mode_from_value);
  let cache_location = raw.cache_location.as_deref().map(PathBuf::from);

  GaleConfig {
    rules,
    ignore_patterns,
    formatter,
    overrides,
    config_dir: Some(base_dir.to_path_buf()),
    plugins,
    report_needless_disables,
    default_severity,
    custom_syntax,
    ignore_disables: raw.ignore_disables.unwrap_or(false),
    report_invalid_scope_disables,
    report_descriptionless_disables,
    report_unscoped_disables,
    allow_empty_input: raw.allow_empty_input.unwrap_or(false),
    quiet: raw.quiet.unwrap_or(false),
    fix,
    cache: raw.cache.unwrap_or(false),
    cache_location,
    cache_strategy: raw.cache_strategy,
  }
}

/// Interpret one of Stylelint's `report*Disables` settings.
///
/// `true` / `false` are the common forms.  The array form
/// `[bool, { except, severity }]` is read for its leading boolean; any other
/// truthy value enables the report.
fn disable_report_enabled(value: Option<&serde_json::Value>) -> bool {
  match value {
    None => false,
    Some(serde_json::Value::Bool(b)) => *b,
    Some(serde_json::Value::Array(items)) => {
      items.first().and_then(|v| v.as_bool()).unwrap_or(true)
    }
    Some(serde_json::Value::Null) => false,
    Some(_) => true,
  }
}

/// Interpret Stylelint's `fix` config value.
///
/// `true` and `"strict"` enable strict fixing, `"lax"` enables lax fixing,
/// and anything else leaves fixing off.
fn fix_mode_from_value(value: &serde_json::Value) -> Option<FixMode> {
  match value {
    serde_json::Value::Bool(true) => Some(FixMode::Strict),
    serde_json::Value::String(s) => match s.to_lowercase().as_str() {
      "strict" => Some(FixMode::Strict),
      "lax" => Some(FixMode::Lax),
      _ => None,
    },
    _ => None,
  }
}

// ---------------------------------------------------------------------------
// High-level convenience
// ---------------------------------------------------------------------------

/// Find a config file starting from `start_dir`, load it, or return `None` if
/// no config file is found anywhere in the directory hierarchy.
///
/// When a config file is found but fails to load, a warning is printed and a
/// default (empty-rules) config is returned — this is still `Some` because the
/// user *has* a config file and we should respect its (empty) rule set rather
/// than falling back to "enable every rule".
pub fn resolve_config(start_dir: &Path) -> Option<GaleConfig> {
  let path = find_config(start_dir)?;
  Some(load_config(&path).unwrap_or_else(|err| {
    eprintln!("Warning: failed to load config {}: {err}", path.display());
    GaleConfig::default()
  }))
}

/// Find the config file that applies to a specific file path.
///
/// Walks up from the file's parent directory looking for the nearest config
/// file — matching Stylelint's cosmiconfig-based per-file resolution.
pub fn find_config_for_file(file_path: &Path) -> Option<PathBuf> {
  let dir = if file_path.is_dir() {
    file_path.to_path_buf()
  } else {
    file_path.parent()?.to_path_buf()
  };
  find_config(&dir)
}

/// Resolve the effective configuration for a specific file path.
///
/// Walks up from the file's parent directory to find the nearest config file,
/// then loads and resolves it.  This matches Stylelint's behaviour where each
/// file is linted with the closest config in the directory hierarchy (not
/// necessarily the CWD config).
pub fn resolve_config_for_file(file_path: &Path) -> Option<GaleConfig> {
  let path = find_config_for_file(file_path)?;
  Some(load_config(&path).unwrap_or_else(|err| {
    eprintln!("Warning: failed to load config {}: {err}", path.display());
    GaleConfig::default()
  }))
}

/// A config resolver that caches resolved configs by the config file path.
///
/// In monorepo-style projects (like Carbon), different subdirectories may have
/// their own stylelint config that overrides the root config.  This resolver
/// ensures each file is linted with its nearest config (matching Stylelint's
/// cosmiconfig behaviour) while avoiding redundant config resolution.
pub struct ConfigResolver {
  /// Cache: config file path -> resolved config.
  cache: HashMap<PathBuf, GaleConfig>,
  /// Cache: directory path -> config file path (or None if no config found).
  dir_cache: HashMap<PathBuf, Option<PathBuf>>,
}

impl ConfigResolver {
  pub fn new() -> Self {
    Self {
      cache: HashMap::new(),
      dir_cache: HashMap::new(),
    }
  }

  /// Seed the resolver with a known config, so that files resolving to this
  /// config file path get the pre-loaded config without re-loading from disk.
  pub fn seed(&mut self, config_path: PathBuf, config: GaleConfig) {
    self.cache.insert(config_path, config);
  }

  /// Find the config file path for a given file, using the directory cache.
  fn find_config_path(&mut self, file_path: &Path) -> Option<PathBuf> {
    let dir = if file_path.is_dir() {
      file_path.to_path_buf()
    } else {
      file_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    // Check directory cache first.
    if let Some(cached) = self.dir_cache.get(&dir) {
      return cached.clone();
    }

    let config_path = find_config(&dir);
    self.dir_cache.insert(dir, config_path.clone());
    config_path
  }

  /// Resolve the effective config for a given file path.
  ///
  /// Returns `None` if no config file is found anywhere in the directory
  /// hierarchy.  Results are cached by config file path.
  pub fn resolve_for_file(&mut self, file_path: &Path) -> Option<&GaleConfig> {
    let config_path = self.find_config_path(file_path)?;

    // Load and cache the config if not already cached.
    if !self.cache.contains_key(&config_path) {
      let config = load_config(&config_path).unwrap_or_else(|err| {
        eprintln!(
          "Warning: failed to load config {}: {err}",
          config_path.display()
        );
        GaleConfig::default()
      });
      self.cache.insert(config_path.clone(), config);
    }

    self.cache.get(&config_path)
  }

  /// Check whether there are multiple distinct config files in play.
  pub fn has_multiple_configs(&self) -> bool {
    self.cache.len() > 1
  }

  /// Pre-resolve configs for a batch of files and return an `Arc`-based
  /// lookup table keyed by parent directory.
  ///
  /// This is designed to be called **once** before parallel linting so that
  /// the hot loop can do a simple `HashMap` lookup without any `Mutex`.
  /// Files whose parent directory maps to the same config file will share
  /// a single `Arc<GaleConfig>`.
  pub fn resolve_all_for_files(
    &mut self,
    files: &[PathBuf],
    fallback: &GaleConfig,
  ) -> HashMap<PathBuf, Arc<GaleConfig>> {
    // Deduplicate directories first to minimise I/O.
    let mut dirs: Vec<PathBuf> = files
      .iter()
      .filter_map(|f| f.parent().map(|p| p.to_path_buf()))
      .collect();
    dirs.sort();
    dirs.dedup();

    // Resolve each directory's config (populates internal caches).
    // Build an Arc-based config cache keyed by config file path so that
    // directories sharing the same config file share one Arc.
    let mut arc_cache: HashMap<PathBuf, Arc<GaleConfig>> = HashMap::new();
    let fallback_arc = Arc::new(fallback.clone());

    let mut dir_to_arc: HashMap<PathBuf, Arc<GaleConfig>> = HashMap::with_capacity(dirs.len());

    for dir in &dirs {
      // Synthesize a dummy file path in this directory so find_config_path works.
      let dummy = dir.join("__dummy__");
      let config_arc = if let Some(config_path) = self.find_config_path(&dummy) {
        // Load config if not already cached.
        if !self.cache.contains_key(&config_path) {
          let config = load_config(&config_path).unwrap_or_else(|err| {
            eprintln!(
              "Warning: failed to load config {}: {err}",
              config_path.display()
            );
            GaleConfig::default()
          });
          self.cache.insert(config_path.clone(), config);
        }
        arc_cache
          .entry(config_path.clone())
          .or_insert_with(|| Arc::new(self.cache[&config_path].clone()))
          .clone()
      } else {
        fallback_arc.clone()
      };
      dir_to_arc.insert(dir.clone(), config_arc);
    }

    dir_to_arc
  }
}

impl Default for ConfigResolver {
  fn default() -> Self {
    Self::new()
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_config() {
    let cfg = GaleConfig::default();
    assert!(cfg.rules.is_empty());
    assert!(cfg.ignore_patterns.is_empty());
    assert_eq!(cfg.formatter, FormatterType::Text);
  }

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
  fn parse_json_config() {
    let json = r#"{
            "rules": {
                "color-no-invalid-hex": true,
                "block-no-empty": "warning",
                "declaration-no-important": ["error", {"severity": "strict"}]
            },
            "ignorePatterns": ["dist/**", "node_modules/**"],
            "formatter": "json"
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert_eq!(cfg.rules.len(), 3);
    assert_eq!(cfg.ignore_patterns.len(), 2);
    assert_eq!(cfg.formatter, FormatterType::Json);
  }

  #[test]
  fn parse_toml_config() {
    let t = r#"
            formatter = "compact"

            [rules]
            color-no-invalid-hex = true
            block-no-empty = "off"
        "#;
    let raw: ConfigFile = toml::from_str(t).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    // block-no-empty is "off" so it gets removed; only color-no-invalid-hex remains.
    assert_eq!(cfg.rules.len(), 1);
    assert!(cfg.rules.contains_key("color-no-invalid-hex"));
    assert_eq!(cfg.formatter, FormatterType::Compact);
  }

  #[test]
  fn parse_yaml_config() {
    let yaml = r#"
rules:
  color-no-invalid-hex: true
  block-no-empty: warning
ignorePatterns:
  - "dist/**"
formatter: text
"#;
    let raw: ConfigFile = serde_yaml::from_str(yaml).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert_eq!(cfg.rules.len(), 2);
    assert_eq!(cfg.ignore_patterns.len(), 1);
    assert_eq!(cfg.formatter, FormatterType::Text);
  }

  // -----------------------------------------------------------------------
  // Preset / extends tests
  // -----------------------------------------------------------------------

  #[test]
  fn resolve_recommended_preset() {
    let preset = resolve_preset("gale:recommended").unwrap();
    // Should contain all error + warning rules.
    assert_eq!(
      preset.len(),
      RECOMMENDED_ERROR_RULES.len() + RECOMMENDED_WARNING_RULES.len()
    );
    // Spot-check severities.
    assert_eq!(preset["block-no-empty"].severity, Some(Severity::Error));
    assert_eq!(preset["color-hex-length"].severity, Some(Severity::Warning));
  }

  #[test]
  fn resolve_all_preset() {
    let preset = resolve_preset("gale:all").unwrap();
    assert_eq!(preset.len(), ALL_RULE_NAMES.len());
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
  fn extends_loads_preset_rules() {
    let json = r#"{ "extends": "gale:recommended" }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    // Should have all recommended rules.
    assert!(cfg.rules.contains_key("block-no-empty"));
    assert!(cfg.rules.contains_key("color-hex-length"));
    assert_eq!(
      cfg.rules.len(),
      RECOMMENDED_ERROR_RULES.len() + RECOMMENDED_WARNING_RULES.len()
    );
  }

  #[test]
  fn user_rules_override_preset() {
    let json = r#"{
            "extends": "gale:recommended",
            "rules": {
                "block-no-empty": "warning"
            }
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    // Preset sets block-no-empty to error, user overrides to warning.
    assert_eq!(
      cfg.rules["block-no-empty"].severity,
      Some(Severity::Warning)
    );
  }

  #[test]
  fn user_can_disable_preset_rule() {
    let json = r#"{
            "extends": "gale:recommended",
            "rules": {
                "block-no-empty": "off"
            }
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    // The rule should be removed entirely.
    assert!(!cfg.rules.contains_key("block-no-empty"));
  }

  #[test]
  fn user_can_disable_preset_rule_with_null() {
    // Gutenberg uses `'no-descending-specificity': null` to disable the rule.
    // PatternFly uses `"no-descending-specificity": null` similarly.
    // When a rule value is null, the rule should be completely disabled
    // even if an extended preset enables it.
    let json = r#"{
            "extends": "gale:recommended",
            "rules": {
                "no-descending-specificity": null
            }
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    // no-descending-specificity is in gale:recommended but null should disable it.
    assert!(
      !cfg.rules.contains_key("no-descending-specificity"),
      "null rule value should completely disable the rule, \
             but no-descending-specificity is still in the resolved rules"
    );
    // Other recommended rules should still be present.
    assert!(cfg.rules.contains_key("block-no-empty"));
    assert!(cfg.rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn user_can_disable_preset_rule_with_false() {
    let json = r#"{
            "extends": "gale:recommended",
            "rules": {
                "block-no-empty": false
            }
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert!(!cfg.rules.contains_key("block-no-empty"));
  }

  #[test]
  fn unknown_preset_is_skipped_gracefully() {
    let json = r#"{
            "extends": ["gale:nonexistent", "gale:recommended"],
            "rules": {}
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    // Should still have recommended rules despite the unknown preset.
    assert!(cfg.rules.contains_key("block-no-empty"));
  }

  #[test]
  fn later_preset_overrides_earlier() {
    // gale:recommended sets block-no-empty to error.
    // gale:all sets everything to warning.
    // gale:all comes second, so it should win.
    let json = r#"{ "extends": ["gale:recommended", "gale:all"] }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert_eq!(
      cfg.rules["block-no-empty"].severity,
      Some(Severity::Warning)
    );
  }

  #[test]
  fn extends_toml_string() {
    let t = r#"
            extends = "gale:recommended"

            [rules]
            block-no-empty = "off"
        "#;
    let raw: ConfigFile = toml::from_str(t).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert!(!cfg.rules.contains_key("block-no-empty"));
    // Other recommended rules should still be present.
    assert!(cfg.rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn extends_toml_array() {
    let t = r#"
            extends = ["gale:recommended"]
        "#;
    let raw: ConfigFile = toml::from_str(t).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert!(cfg.rules.contains_key("block-no-empty"));
  }

  #[test]
  fn no_extends_works_as_before() {
    let json = r#"{
            "rules": {
                "color-no-invalid-hex": true
            }
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert_eq!(cfg.rules.len(), 1);
    assert!(cfg.rules.contains_key("color-no-invalid-hex"));
  }

  // -----------------------------------------------------------------------
  // JavaScript config parsing tests
  // -----------------------------------------------------------------------

  #[test]
  fn js_config_module_exports_single_quoted_keys() {
    let js = r#"
'use strict';

module.exports = {
  'extends': ['stylelint-config-standard'],
  'rules': {
    'alpha-value-notation': null,
    'color-named': 'never',
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("color-named"));
  }

  #[test]
  fn js_config_trailing_commas() {
    let js = r#"
module.exports = {
  "rules": {
    "block-no-empty": true,
    "color-no-invalid-hex": true,
  },
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 2);
    assert!(rules.contains_key("block-no-empty"));
    assert!(rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn js_config_null_values() {
    let js = r#"
module.exports = {
  'rules': {
    'alpha-value-notation': null,
    'declaration-no-important': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 2);
  }

  #[test]
  fn js_config_array_values() {
    let js = r#"
module.exports = {
  'rules': {
    'font-weight-notation': ['numeric', { 'ignore': ['relative'] }],
    'selector-max-id': 0,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("font-weight-notation"));
    assert!(rules.contains_key("selector-max-id"));
  }

  #[test]
  fn js_config_nested_objects() {
    let js = r#"
module.exports = {
  'rules': {
    'font-weight-notation': ['numeric', { 'ignore': ['relative'] }],
    'color-named': 'never',
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 2);
  }

  #[test]
  fn js_config_export_default() {
    let js = r#"
export default {
  "extends": ["stylelint-config-standard"],
  "rules": {
    "block-no-empty": true
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));
  }

  #[test]
  fn js_config_unquoted_keys() {
    let js = r#"
module.exports = {
  extends: ['stylelint-config-standard'],
  rules: {
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
  }

  #[test]
  fn js_config_with_comments() {
    let js = r#"
// Main config
module.exports = {
  /* extends from standard */
  'extends': ['stylelint-config-standard'],
  'rules': {
    // Disable this rule
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert!(raw.extends.is_some());
  }

  #[test]
  fn js_config_with_spread_skipped() {
    let js = r#"
module.exports = {
  ...baseConfig,
  'rules': {
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));
  }

  #[test]
  fn js_config_no_exports_returns_error() {
    let js = r#"
const config = {
  rules: { 'block-no-empty': true }
};
"#;
    assert!(parse_js_config(js, None).is_err());
  }

  #[test]
  fn js_config_parse_config_file_dispatches() {
    let js = r#"
module.exports = {
  'rules': {
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_config_file("stylelint.config.js", js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));

    // Also works for .mjs
    let raw2 = parse_config_file("stylelint.config.mjs", js, None).unwrap();
    assert!(raw2.rules.is_some());

    // Also works for .cjs
    let raw3 = parse_config_file("stylelint.config.cjs", js, None).unwrap();
    assert!(raw3.rules.is_some());
  }

  #[test]
  fn js_config_full_realistic_example() {
    let js = r#"
'use strict';

module.exports = {
  'extends': ['stylelint-config-standard'],
  'rules': {
    'alpha-value-notation': null,
    'color-named': 'never',
    'declaration-no-important': true,
    'font-weight-notation': ['numeric', { 'ignore': ['relative'] }],
    'selector-max-id': 0,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    // Verify parsing succeeded and extracted the right structure.
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 5);
    assert!(rules.contains_key("alpha-value-notation"));
    assert!(rules.contains_key("color-named"));
    assert!(rules.contains_key("declaration-no-important"));
    assert!(rules.contains_key("font-weight-notation"));
    assert!(rules.contains_key("selector-max-id"));
  }

  // -----------------------------------------------------------------------
  // ESM/CJS import resolution tests
  // -----------------------------------------------------------------------

  #[test]
  fn extract_imports_esm_default() {
    let src = r#"
import propertyGroups from './groups.js'
import something from 'not-relative'
const config = {}
"#;
    let imports = extract_imports(src);
    assert_eq!(imports.len(), 2);
    assert_eq!(
      imports[0],
      ("propertyGroups".to_string(), "./groups.js".to_string())
    );
    assert_eq!(
      imports[1],
      ("something".to_string(), "not-relative".to_string())
    );
  }

  #[test]
  fn extract_imports_cjs_require() {
    let src = r#"
const groups = require('./groups')
const stylelint = require('stylelint')
"#;
    let imports = extract_imports(src);
    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0], ("groups".to_string(), "./groups".to_string()));
    assert_eq!(
      imports[1],
      ("stylelint".to_string(), "stylelint".to_string())
    );
  }

  #[test]
  fn extract_imports_skips_destructured() {
    let src = r#"
import { foo } from './bar'
const { baz } = require('./qux')
"#;
    let imports = extract_imports(src);
    assert!(imports.is_empty());
  }

  #[test]
  fn extract_default_export_direct_array() {
    let src = r#"
const foo = 'bar'
export default [1, 2, 3]
"#;
    let val = extract_default_export(src).unwrap();
    assert_eq!(val, "[1, 2, 3]");
  }

  #[test]
  fn extract_default_export_via_variable() {
    let src = r#"
const myArray = [
    { "properties": ["a", "b"] },
    { "properties": ["c", "d"] }
]
export default myArray
"#;
    let val = extract_default_export(src).unwrap();
    assert!(val.starts_with('['));
    assert!(val.contains("\"properties\""));
    assert!(val.ends_with(']'));
  }

  #[test]
  fn extract_default_export_module_exports() {
    let src = r#"
const data = [1, 2, 3]
module.exports = data
"#;
    let val = extract_default_export(src).unwrap();
    assert_eq!(val, "[1, 2, 3]");
  }

  #[test]
  fn extract_reexport_require_detects_pattern() {
    let src = r#""use strict"
module.exports = require("./stylelint.config")
"#;
    let path = extract_reexport_require(src);
    assert_eq!(path, Some("./stylelint.config".to_string()));
  }

  #[test]
  fn extract_reexport_require_no_match_for_object() {
    let src = r#"module.exports = { rules: {} }"#;
    let path = extract_reexport_require(src);
    assert_eq!(path, None);
  }

  #[test]
  fn extract_reexport_require_single_quotes() {
    let src = "module.exports = require('./config');\n";
    let path = extract_reexport_require(src);
    assert_eq!(path, Some("./config".to_string()));
  }

  #[test]
  fn strip_import_lines_removes_imports() {
    let src = "import foo from './bar'\nconst x = require('./baz')\nexport default {}";
    let stripped = strip_import_lines(src);
    assert!(!stripped.contains("import foo"));
    assert!(!stripped.contains("require"));
    assert!(stripped.contains("export default {}"));
  }

  #[test]
  fn replace_whole_word_basic() {
    let src = "foo + foobar + foo";
    let result = replace_whole_word(src, "foo", "42");
    assert_eq!(result, "42 + foobar + 42");
  }

  #[test]
  fn js_config_with_esm_import_resolution() {
    // Create a temporary directory with a mock file structure
    let tmp = std::env::temp_dir().join("gale_test_esm_import");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // Write the imported file: groups.js
    std::fs::write(
      tmp.join("groups.js"),
      r#"
const propertyGroups = [
    { "properties": ["position", "top", "right"] },
    { "properties": ["display", "flex"] }
]

export default propertyGroups
"#,
    )
    .unwrap();

    // Write the main config file (uses the common pattern:
    // const config = {...}; export default config)
    let config_src = r#"
import propertyGroups from './groups.js'

const config = {
    plugins: ['stylelint-order'],
    rules: {
        'order/properties-order': propertyGroups,
    },
}

export default config
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("order/properties-order"));

    // The value should be an array with the property groups
    let val = rules.get("order/properties-order").unwrap();
    match val {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
        // Verify first group has the right properties
        assert!(arr[0].is_object());
      }
      _ => panic!(
        "Expected array value for order/properties-order, got {:?}",
        val
      ),
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_with_esm_import_with_comments() {
    // Simulates the real stylelint-config-recess-order pattern
    // where groups.js has JS comments inside the array.
    let tmp = std::env::temp_dir().join("gale_test_esm_import_comments");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    std::fs::write(
      tmp.join("groups.js"),
      r#"
/**
 * @typedef {Object} Group
 * @property {Array<string>} properties
 */

/** @type {Group[]} */
const propertyGroups = [
    {
        /**
         * Compose rules from other selectors in CSS Modules.
         * @see https://github.com/css-modules/css-modules#composition
         */
        properties: ['composes'],
    },
    {
        // Must be first (unless using the above).
        properties: ['all'],
    },
    {
        // Position.
        properties: [
            'position',
            'top',
            'right',
            'bottom',
            'left',
        ],
    },
    {
        // Display.
        properties: ['display', 'flex'],
    },
]

export default propertyGroups
"#,
    )
    .unwrap();

    let config_src = r#"
import propertyGroups from './groups.js'

const config = {
    plugins: ['stylelint-order'],
    rules: {
        'order/properties-order': propertyGroups,
    },
}

export default config
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(
      rules.contains_key("order/properties-order"),
      "should have order/properties-order rule"
    );

    let val = rules.get("order/properties-order").unwrap();
    match val {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 4, "should have 4 property groups");
        assert!(arr[0].is_object());
      }
      _ => panic!(
        "Expected array value for order/properties-order, got {:?}",
        val
      ),
    }

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_recess_order_real_files() {
    // Test with the actual recess-order files if available.
    let recess_dir = std::path::Path::new(
      "../../benchmarks/.repos/bootstrap/node_modules/stylelint-config-recess-order",
    );
    if !recess_dir.exists() {
      // Skip if the benchmark repo isn't set up.
      return;
    }

    let index_path = recess_dir.join("index.js");
    let contents = std::fs::read_to_string(&index_path).unwrap();
    let result = parse_js_config(&contents, Some(recess_dir));
    match &result {
      Ok(config) => {
        let rules = config.rules.as_ref().expect("should have rules");
        assert!(
          rules.contains_key("order/properties-order"),
          "recess-order config should have order/properties-order"
        );
        let val = rules.get("order/properties-order").unwrap();
        match val {
          RuleConfigValue::Array(arr) => {
            assert!(
              arr.len() > 5,
              "should have many property groups, got {}",
              arr.len()
            );
          }
          other => panic!(
            "Expected Array for order/properties-order, got {:?}",
            std::mem::discriminant(other)
          ),
        }
      }
      Err(e) => {
        panic!("Failed to parse recess-order config: {e}");
      }
    }
  }

  #[test]
  fn js_config_with_cjs_import_resolution() {
    let tmp = std::env::temp_dir().join("gale_test_cjs_import");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // Write imported file
    std::fs::write(
      tmp.join("groups.js"),
      r#"
module.exports = [
    { "properties": ["color", "background"] }
]
"#,
    )
    .unwrap();

    let config_src = r#"
const groups = require('./groups')

module.exports = {
    rules: {
        'order/properties-order': groups,
    },
}
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("order/properties-order"));

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_import_missing_file_graceful() {
    // When the imported file doesn't exist, parsing should still work
    // (the variable just won't be replaced, and parsing continues)
    let config_src = r#"
import stuff from './nonexistent.js'

export default {
    rules: {
        'block-no-empty': true,
    },
}
"#;
    let tmp = std::env::temp_dir().join("gale_test_missing_import");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_npm_import_not_resolved() {
    // npm package imports should not be resolved (no relative path).
    // The import line is stripped, and string plugin names work fine.
    let config_src = r#"
import something from 'stylelint-order'

export default {
    plugins: ['stylelint-order'],
    rules: {
        'block-no-empty': true,
    },
}
"#;
    let raw = parse_js_config(config_src, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));
  }

  #[test]
  fn js_config_import_with_extension_resolution() {
    // Test that we try adding .js extension when path has no extension
    let tmp = std::env::temp_dir().join("gale_test_ext_resolution");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    std::fs::write(tmp.join("data.js"), "export default [\"a\", \"b\"]\n").unwrap();

    let config_src = r#"
import items from './data'

export default {
    rules: {
        'my-rule': items,
    },
}
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("my-rule"));
    match rules.get("my-rule").unwrap() {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
      }
      other => panic!("Expected array, got {:?}", other),
    }

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_regexp_literals() {
    let js = r#"
module.exports = {
    rules: {
        'selector-class-pattern': [/^[a-z]([a-z0-9-_!])*$/, {
            resolveNestedSelectors: true
        }],
        'selector-id-pattern': /^[a-z]([a-z0-9-])*$/i,
    }
}
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    match rules.get("selector-class-pattern").unwrap() {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], serde_json::json!("^[a-z]([a-z0-9-_!])*$"));
      }
      other => panic!("Expected array, got {:?}", other),
    }
    match rules.get("selector-id-pattern").unwrap() {
      RuleConfigValue::Severity(s) => {
        assert_eq!(s, "^[a-z]([a-z0-9-])*$");
      }
      other => panic!("Expected string, got {:?}", other),
    }
  }

  #[test]
  fn js_config_string_concatenation() {
    let js = r#"
module.exports = {
    rules: {
        'selector-class-pattern': ['pattern', {
            message: 'Class names may only contain [a-z0-9-_!] characters and ' +
                'must start with [a-z]'
        }]
    }
}
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    match rules.get("selector-class-pattern").unwrap() {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
        let opts = arr[1].as_object().unwrap();
        assert_eq!(
          opts.get("message").unwrap().as_str().unwrap(),
          "Class names may only contain [a-z0-9-_!] characters and must start with [a-z]"
        );
      }
      other => panic!("Expected array, got {:?}", other),
    }
  }

  // -----------------------------------------------------------------------
  // Override tests
  // -----------------------------------------------------------------------

  #[test]
  fn resolved_override_matches_glob() {
    let ov = ResolvedOverride::new(vec!["**/*.scss".to_string()], vec![], HashMap::new(), None);
    assert!(ov.matches("src/styles/main.scss"));
    assert!(ov.matches("main.scss"));
    assert!(!ov.matches("main.css"));
    assert!(!ov.matches("main.less"));
  }

  #[test]
  fn resolved_override_matches_multiple_patterns() {
    let ov = ResolvedOverride::new(
      vec!["**/*.scss".to_string(), "**/*.less".to_string()],
      vec![],
      HashMap::new(),
      None,
    );
    assert!(ov.matches("main.scss"));
    assert!(ov.matches("main.less"));
    assert!(!ov.matches("main.css"));
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
  fn resolve_config_with_overrides() {
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
    let cfg = resolve_raw(raw, Path::new("."));
    assert_eq!(cfg.overrides.len(), 1);
    assert!(cfg.overrides[0].matches("main.scss"));
    assert!(!cfg.overrides[0].matches("main.css"));
  }

  #[test]
  fn rules_for_file_applies_overrides() {
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
    let cfg = resolve_raw(raw, Path::new("."));

    // CSS files should have the base rules unchanged.
    let css_rules = cfg.rules_for_file("main.css");
    assert!(css_rules.contains_key("no-duplicate-selectors"));

    // SCSS files should have the overridden rules removed.
    let scss_rules = cfg.rules_for_file("main.scss");
    assert!(!scss_rules.contains_key("no-duplicate-selectors"));
    assert!(!scss_rules.contains_key("comment-no-empty"));
    // But other base rules should still be present.
    assert!(scss_rules.contains_key("block-no-empty"));
  }

  #[test]
  fn rules_for_file_no_overrides_returns_base() {
    let json = r#"{
            "extends": "gale:recommended"
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));

    let rules = cfg.rules_for_file("main.scss");
    assert_eq!(rules, cfg.rules);
  }

  #[test]
  fn multiple_overrides_applied_in_order() {
    let json = r#"{
            "rules": {
                "block-no-empty": true,
                "color-no-invalid-hex": true
            },
            "overrides": [
                {
                    "files": "**/*.scss",
                    "rules": {
                        "block-no-empty": "warning"
                    }
                },
                {
                    "files": "**/*.scss",
                    "rules": {
                        "block-no-empty": "off"
                    }
                }
            ]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));

    // The second override should win: block-no-empty should be removed.
    let scss_rules = cfg.rules_for_file("main.scss");
    assert!(!scss_rules.contains_key("block-no-empty"));
    // color-no-invalid-hex should remain untouched.
    assert!(scss_rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn override_adds_new_rule() {
    let json = r#"{
            "rules": {
                "block-no-empty": true
            },
            "overrides": [
                {
                    "files": "**/*.scss",
                    "rules": {
                        "color-no-invalid-hex": "warning"
                    }
                }
            ]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));

    // CSS file should only have the base rule.
    let css_rules = cfg.rules_for_file("main.css");
    assert_eq!(css_rules.len(), 1);

    // SCSS file should have both base + override rule.
    let scss_rules = cfg.rules_for_file("main.scss");
    assert_eq!(scss_rules.len(), 2);
    assert!(scss_rules.contains_key("block-no-empty"));
    assert!(scss_rules.contains_key("color-no-invalid-hex"));
    assert_eq!(
      scss_rules["color-no-invalid-hex"].severity,
      Some(Severity::Warning)
    );
  }

  #[test]
  fn override_with_extends_preset() {
    let json = r#"{
            "rules": {
                "block-no-empty": true
            },
            "overrides": [
                {
                    "files": "**/*.scss",
                    "extends": "stylelint-config-recommended-scss",
                    "rules": {
                        "no-duplicate-selectors": null
                    }
                }
            ]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));

    // SCSS file should have rules from the preset minus the null override.
    let scss_rules = cfg.rules_for_file("main.scss");
    assert!(!scss_rules.contains_key("no-duplicate-selectors"));
    // Should have rules from the preset (at-rule-no-unknown is off in recommended-scss).
    assert!(!scss_rules.contains_key("at-rule-no-unknown"));
    // Should still have the base rule.
    assert!(scss_rules.contains_key("block-no-empty"));
  }

  #[test]
  fn override_empty_files_skipped() {
    let json = r#"{
            "rules": {
                "block-no-empty": true
            },
            "overrides": [
                {
                    "rules": {
                        "block-no-empty": "off"
                    }
                }
            ]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));

    // Override with no files should be skipped entirely.
    assert_eq!(cfg.overrides.len(), 0);
    let rules = cfg.rules_for_file("main.css");
    assert!(rules.contains_key("block-no-empty"));
  }

  #[test]
  fn override_has_overrides_flag() {
    let json_with = r#"{
            "overrides": [{ "files": "**/*.scss", "rules": {} }]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json_with).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert!(cfg.has_overrides());

    let json_without = r#"{ "rules": {} }"#;
    let raw: ConfigFile = serde_json::from_str(json_without).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert!(!cfg.has_overrides());
  }

  #[test]
  fn yaml_config_with_overrides() {
    let yaml = r#"
extends: gale:recommended
overrides:
  - files: "**/*.scss"
    rules:
      no-duplicate-selectors: null
      comment-no-empty: null
"#;
    let raw: ConfigFile = serde_yaml::from_str(yaml).unwrap();
    assert!(raw.overrides.is_some());
    let cfg = resolve_raw(raw, Path::new("."));
    assert_eq!(cfg.overrides.len(), 1);

    let scss_rules = cfg.rules_for_file("main.scss");
    assert!(!scss_rules.contains_key("no-duplicate-selectors"));
    assert!(!scss_rules.contains_key("comment-no-empty"));
  }

  // -----------------------------------------------------------------------
  // package.json "stylelint" field
  // -----------------------------------------------------------------------

  #[test]
  fn find_config_discovers_package_json_stylelint_field() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("package.json");
    std::fs::write(
      &pkg,
      r#"{
                "name": "my-project",
                "stylelint": {
                    "rules": { "block-no-empty": true }
                }
            }"#,
    )
    .unwrap();

    let found = find_config(tmp.path());
    assert_eq!(found, Some(pkg));
  }

  #[test]
  fn find_config_prefers_stylelintrc_over_package_json() {
    let tmp = tempfile::tempdir().unwrap();
    // Both .stylelintrc.json and package.json with stylelint field exist.
    std::fs::write(
      tmp.path().join(".stylelintrc.json"),
      r#"{ "rules": { "color-no-invalid-hex": true } }"#,
    )
    .unwrap();
    std::fs::write(
      tmp.path().join("package.json"),
      r#"{ "name": "x", "stylelint": { "rules": { "block-no-empty": true } } }"#,
    )
    .unwrap();

    let found = find_config(tmp.path()).unwrap();
    assert_eq!(found.file_name().unwrap(), ".stylelintrc.json");
  }

  #[test]
  fn find_config_ignores_package_json_without_stylelint_field() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
      tmp.path().join("package.json"),
      r#"{ "name": "no-lint-config" }"#,
    )
    .unwrap();

    assert_eq!(find_config(tmp.path()), None);
  }

  #[test]
  fn load_config_from_package_json_object() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("package.json");
    std::fs::write(
      &pkg,
      r#"{
                "name": "my-project",
                "stylelint": {
                    "rules": {
                        "block-no-empty": true,
                        "color-no-invalid-hex": "warning"
                    }
                }
            }"#,
    )
    .unwrap();

    let cfg = load_config(&pkg).unwrap();
    assert_eq!(cfg.rules.len(), 2);
    assert!(cfg.rules.contains_key("block-no-empty"));
    assert!(cfg.rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn load_config_from_package_json_string_extends() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("package.json");
    std::fs::write(
      &pkg,
      r#"{
                "name": "my-project",
                "stylelint": "stylelint-config-standard"
            }"#,
    )
    .unwrap();

    // load_config will try to resolve extends, which will fail for
    // "stylelint-config-standard" without node_modules.  Instead,
    // test parse_package_json_stylelint directly.
    let contents = std::fs::read_to_string(&pkg).unwrap();
    let raw = parse_package_json_stylelint(&contents).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
  }

  #[test]
  fn parse_package_json_stylelint_missing_field() {
    let contents = r#"{ "name": "no-lint" }"#;
    assert!(parse_package_json_stylelint(contents).is_err());
  }

  #[test]
  fn rules_for_file_override_null_with_config_dir() {
    // Reproduce Carbon's scenario: a config in a subdirectory has
    // overrides that null-out a rule for files matching a relative glob.
    // The config_dir must be used to match the glob correctly.
    let json = r#"{
            "extends": "gale:recommended",
            "overrides": [
                {
                    "files": ["src/components/**/*.scss"],
                    "rules": {
                        "max-nesting-depth": null
                    }
                }
            ]
        }"#;
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("packages").join("web-components");
    std::fs::create_dir_all(&sub).unwrap();
    let mut cfg = resolve_raw(raw, &sub);
    cfg.config_dir = Some(sub.clone());

    // A file that matches the override pattern (relative to the
    // config dir) should have max-nesting-depth removed.
    let file_abs = sub.join("src/components/grid/grid-story.scss");
    let file_str = file_abs.to_string_lossy();
    let rules = cfg.rules_for_file(&file_str);
    assert!(
      !rules.contains_key("max-nesting-depth"),
      "max-nesting-depth should be disabled by the override, but it is present"
    );

    // A file that does NOT match the override pattern should still have
    // max-nesting-depth from the base config (gale:recommended does not
    // include it, so it should not appear regardless).
    let other = sub.join("src/other/main.css");
    let other_str = other.to_string_lossy();
    let other_rules = cfg.rules_for_file(&other_str);
    // max-nesting-depth is not in gale:recommended, so this just verifies
    // the override path doesn't accidentally affect non-matching files.
    assert!(!other_rules.contains_key("max-nesting-depth"));
  }

  #[test]
  fn per_file_config_resolver_finds_nested_config() {
    // Simulate a monorepo where a subdirectory has its own config that
    // overrides the root config.
    let tmp = tempfile::tempdir().unwrap();

    // Root config: enables max-nesting-depth with max 3.
    let root_cfg = r#"{
            "rules": {
                "block-no-empty": true,
                "max-nesting-depth": 3
            }
        }"#;
    std::fs::write(tmp.path().join(".stylelintrc.json"), root_cfg).unwrap();

    // Sub-package config: extends root but overrides max-nesting-depth
    // to null for component files.
    let sub = tmp.path().join("packages").join("web-components");
    std::fs::create_dir_all(&sub).unwrap();
    let sub_cfg = r#"{
            "rules": {
                "block-no-empty": true,
                "max-nesting-depth": null
            }
        }"#;
    std::fs::write(sub.join(".stylelintrc.json"), sub_cfg).unwrap();

    // A file under the sub-package should use the sub-package config.
    let file = sub.join("src/components/test.scss");
    let mut resolver = ConfigResolver::new();
    let resolved = resolver.resolve_for_file(&file).unwrap();
    assert!(
      !resolved.rules.contains_key("max-nesting-depth"),
      "max-nesting-depth should be disabled by the sub-package config"
    );
    assert!(resolved.rules.contains_key("block-no-empty"));

    // A file at the root should use the root config (max-nesting-depth enabled).
    let root_file = tmp.path().join("src/main.css");
    let root_resolved = resolver.resolve_for_file(&root_file).unwrap();
    assert!(
      root_resolved.rules.contains_key("max-nesting-depth"),
      "max-nesting-depth should be enabled in the root config"
    );
  }

  // -----------------------------------------------------------------------
  // customSyntax skip tests
  // -----------------------------------------------------------------------

  #[test]
  fn custom_syntax_for_file_reads_a_matching_override() {
    let config = GaleConfig {
      overrides: vec![ResolvedOverride::new(
        vec!["**/*.md".to_string()],
        vec![],
        HashMap::new(),
        Some("postcss-markdown".to_string()),
      )],
      ..Default::default()
    };
    assert_eq!(
      config.custom_syntax_for_file("docs/readme.md"),
      Some("postcss-markdown")
    );
    // A file the override's globs don't match has no customSyntax.
    assert_eq!(config.custom_syntax_for_file("src/main.scss"), None);
  }

  #[test]
  fn custom_syntax_for_file_is_none_without_config() {
    let config = GaleConfig {
      rules: HashMap::new(),
      ..Default::default()
    };
    assert_eq!(config.custom_syntax_for_file("src/styles.css"), None);
  }

  #[test]
  fn custom_syntax_for_file_falls_back_to_the_top_level() {
    let config = GaleConfig {
      custom_syntax: Some("postcss-html".to_string()),
      ..Default::default()
    };
    assert_eq!(
      config.custom_syntax_for_file("src/styles.css"),
      Some("postcss-html")
    );
  }

  #[test]
  fn override_custom_syntax_takes_precedence_over_top_level() {
    let config = GaleConfig {
      custom_syntax: Some("postcss-markdown".to_string()),
      overrides: vec![ResolvedOverride::new(
        vec!["**/*.scss".to_string()],
        vec![],
        HashMap::new(),
        Some("postcss-scss".to_string()),
      )],
      ..Default::default()
    };
    assert_eq!(
      config.custom_syntax_for_file("src/main.scss"),
      Some("postcss-scss")
    );
    // A CSS file doesn't match the override, so the top-level value applies.
    assert_eq!(
      config.custom_syntax_for_file("src/main.css"),
      Some("postcss-markdown")
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

  // -- CLI-equivalent config keys --------------------------------------

  fn resolve_json(json: &str) -> GaleConfig {
    let raw: ConfigFile = serde_json::from_str(json).unwrap();
    resolve_raw(raw, Path::new("."))
  }

  #[test]
  fn cli_equivalent_keys_default_to_off() {
    let cfg = resolve_json(r#"{ "rules": {} }"#);
    assert!(!cfg.ignore_disables);
    assert!(!cfg.report_invalid_scope_disables);
    assert!(!cfg.report_descriptionless_disables);
    assert!(!cfg.report_unscoped_disables);
    assert!(!cfg.allow_empty_input);
    assert!(!cfg.quiet);
    assert_eq!(cfg.fix, None);
    assert!(!cfg.cache);
    assert_eq!(cfg.cache_location, None);
    assert_eq!(cfg.cache_strategy, None);
  }

  #[test]
  fn cli_equivalent_boolean_keys_are_read() {
    let cfg = resolve_json(
      r#"{
                "rules": {},
                "ignoreDisables": true,
                "allowEmptyInput": true,
                "quiet": true,
                "cache": true,
                "cacheLocation": "tmp/lint.cache",
                "cacheStrategy": "content"
            }"#,
    );
    assert!(cfg.ignore_disables);
    assert!(cfg.allow_empty_input);
    assert!(cfg.quiet);
    assert!(cfg.cache);
    assert_eq!(cfg.cache_location, Some(PathBuf::from("tmp/lint.cache")));
    assert_eq!(cfg.cache_strategy.as_deref(), Some("content"));
  }

  #[test]
  fn disable_report_keys_accept_bool_and_array_forms() {
    let cfg = resolve_json(
      r#"{
                "rules": {},
                "reportInvalidScopeDisables": true,
                "reportDescriptionlessDisables": [true, { "except": ["a"] }],
                "reportUnscopedDisables": true
            }"#,
    );
    assert!(cfg.report_invalid_scope_disables);
    assert!(cfg.report_descriptionless_disables);
    assert!(cfg.report_unscoped_disables);

    let cfg = resolve_json(
      r#"{
                "rules": {},
                "reportInvalidScopeDisables": false,
                "reportDescriptionlessDisables": [false, { "except": ["a"] }],
                "reportNeedlessDisables": null
            }"#,
    );
    assert!(!cfg.report_invalid_scope_disables);
    assert!(!cfg.report_descriptionless_disables);
    assert!(!cfg.report_needless_disables);
  }

  #[test]
  fn fix_key_maps_to_fix_mode() {
    assert_eq!(
      resolve_json(r#"{ "rules": {}, "fix": true }"#).fix,
      Some(FixMode::Strict)
    );
    assert_eq!(
      resolve_json(r#"{ "rules": {}, "fix": "strict" }"#).fix,
      Some(FixMode::Strict)
    );
    assert_eq!(
      resolve_json(r#"{ "rules": {}, "fix": "lax" }"#).fix,
      Some(FixMode::Lax)
    );
    assert_eq!(resolve_json(r#"{ "rules": {}, "fix": false }"#).fix, None);
    assert_eq!(resolve_json(r#"{ "rules": {}, "fix": "bogus" }"#).fix, None);
  }

  #[test]
  fn cli_equivalent_keys_parse_from_yaml() {
    let yaml = "rules: {}\nignoreDisables: true\nquiet: true\nfix: lax\n";
    let raw: ConfigFile = serde_yaml::from_str(yaml).unwrap();
    let cfg = resolve_raw(raw, Path::new("."));
    assert!(cfg.ignore_disables);
    assert!(cfg.quiet);
    assert_eq!(cfg.fix, Some(FixMode::Lax));
  }
}
