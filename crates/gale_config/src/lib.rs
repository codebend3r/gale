use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use gale_linter::rules::Preset;
use globset::{Glob, GlobMatcher};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod js;

use js::parse_js_config;

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
  /// The override this one came from through its `extends`, which it only
  /// applies within: a file must match both.
  scope: Option<Box<ResolvedOverride>>,
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
      scope: None,
    }
  }

  /// This override, applying only to files `outer` matches too.
  fn within(mut self, outer: &ResolvedOverride) -> Self {
    self.scope = Some(Box::new(match self.scope.take() {
      Some(scope) => scope.within(outer),
      None => ResolvedOverride {
        rules: HashMap::new(),
        ..outer.clone()
      },
    }));
    self
  }

  /// Whether the file is inside the override this one is scoped to, if any.
  fn in_scope(&self, file_path: &str) -> bool {
    self
      .scope
      .as_ref()
      .is_none_or(|scope| scope.matches(file_path))
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
    !self.exclude_matchers.iter().any(|m| m.is_match(path)) && self.in_scope(file_path)
  }

  /// Check whether a file path matches the override's `files` patterns.
  pub fn matches_files(&self, file_path: &str) -> bool {
    let path = Path::new(file_path);
    self.matchers.iter().any(|m| m.is_match(path)) && self.in_scope(file_path)
  }

  /// Check whether a file path is excluded by this override's `ignoreFiles`
  /// patterns.
  pub fn is_excluded(&self, file_path: &str) -> bool {
    let path = Path::new(file_path);
    self.exclude_matchers.iter().any(|m| m.is_match(path))
  }

  /// The override's `ignoreFiles` patterns that compiled, as written.
  pub fn ignore_patterns(&self) -> impl Iterator<Item = &str> {
    self.exclude_matchers.iter().map(|m| m.glob().glob())
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
  /// Stylelint's `formatter`, exactly as the config wrote it.  The CLI
  /// validates the name (an unknown one is an error) and uses it when no
  /// `--formatter` flag is given.
  pub formatter: Option<String>,
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
  /// An empty config: no rules, no overrides, text output.
  fn default() -> Self {
    Self {
      rules: HashMap::new(),
      ignore_patterns: Vec::new(),
      formatter: None,
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
    if let Some(found) = config_in_dir(&dir) {
      return Some(found);
    }
    if !dir.pop() {
      return None;
    }
  }
}

/// The config file `dir` itself provides, without looking at its ancestors:
/// the first of [`CONFIG_FILENAMES`] present, else a `package.json` with a
/// `"stylelint"` field.
fn config_in_dir(dir: &Path) -> Option<PathBuf> {
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
  None
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

/// Parses config contents using the format implied by the file name.
/// `.stylelintrc` has no extension, so it is tried as JSON then YAML.
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
pub(crate) fn resolve_package_exports(pkg: &serde_json::Value, subpath: &str) -> Option<String> {
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
pub(crate) fn split_npm_package_subpath(name: &str) -> (&str, Option<&str>) {
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

/// Warnings already printed this run, so a config that every directory
/// shares (and that is loaded more than once) warns once.
static WARNED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Print `message` to stderr unless it was already printed this run.
fn warn_once(message: String) {
  let mut warned = WARNED.lock().unwrap_or_else(|e| e.into_inner());
  if warned.insert(message.clone()) {
    eprintln!("{message}");
  }
}

/// The warning for an `extends` entry that resolves to nothing.
fn unresolved_extends_warning(entry: &str, base_dir: &Path) -> String {
  if entry == UNEVALUATED_EXTENDS {
    format!(
      "warning: could not resolve an extends entry in {}: it is not a string \
       (gale reads JavaScript configs statically), so its rules were skipped",
      base_dir.display()
    )
  } else {
    format!(
      "warning: could not resolve extends \"{entry}\" from {} (is it installed?), \
       so its rules were skipped",
      base_dir.display()
    )
  }
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

    if preset_name == UNEVALUATED_EXTENDS {
      warn_once(unresolved_extends_warning(preset_name, base_dir));
      continue;
    }

    if preset_name.starts_with("gale:") {
      // gale: presets are always built-in.
      if let Some(preset_rules) = resolve_preset(preset_name) {
        rules.extend(preset_rules);
      } else {
        warn_once(format!("warning: unknown preset '{preset_name}', skipping"));
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
        warn_once(unresolved_extends_warning(preset_name, base_dir));
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

  let formatter = raw.formatter;

  // 3. Resolve overrides.
  //    Extended configs' overrides come first, then the user's own overrides.
  // Combine: extended overrides first, then user overrides.
  let mut all_raw_overrides = extended_overrides;
  if let Some(user_overrides) = raw.overrides {
    all_raw_overrides.extend(user_overrides);
  }

  let overrides: Vec<ResolvedOverride> = all_raw_overrides
    .into_iter()
    .flat_map(|ov| resolve_override(ov, base_dir, 0))
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

/// How deep overrides may nest through `extends` before gale stops
/// following them, as a guard against configs that extend each other.
const MAX_OVERRIDE_NESTING: usize = 8;

/// Resolve one `overrides` entry into the overrides gale applies, in order.
///
/// The override's `extends` supply rules under its own.  When an extended
/// config has `overrides` of its own (`stylelint-config-standard-vue`
/// extends `stylelint-config-recommended-vue`, whose Vue rules sit in an
/// override), Stylelint applies those too, to the files both match.  Such
/// an entry becomes three steps with Stylelint's precedence: the extended
/// rules, then the extended configs' overrides, then the entry's own rules.
fn resolve_override(ov: ConfigOverride, base_dir: &Path, depth: usize) -> Vec<ResolvedOverride> {
  let file_patterns = ov.files.unwrap_or_default();
  if file_patterns.is_empty() {
    return Vec::new();
  }
  let ignore_patterns = ov.ignore_files.unwrap_or_default();
  let custom_syntax = ov
    .custom_syntax
    .as_ref()
    .and_then(|v| v.as_str().map(String::from));

  // Rules from the override's extends, and the overrides they bring.
  let (mut ov_rules, nested) = match ov.extends {
    Some(ref extends) => collect_rules_from_extends(extends, base_dir, &mut HashSet::new()),
    None => (HashMap::new(), Vec::new()),
  };
  // The override's own rules.  Off entries are kept so they can remove
  // base rules when applied per-file.
  let own_rules: HashMap<String, RuleConfig> = ov
    .rules
    .unwrap_or_default()
    .into_iter()
    .map(|(name, value)| (name, value.resolve()))
    .collect();

  if nested.is_empty() || depth >= MAX_OVERRIDE_NESTING {
    ov_rules.extend(own_rules);
    return vec![ResolvedOverride::new(
      file_patterns,
      ignore_patterns,
      ov_rules,
      custom_syntax,
    )];
  }

  let outer = ResolvedOverride::new(
    file_patterns.clone(),
    ignore_patterns.clone(),
    ov_rules,
    custom_syntax,
  );
  let mut resolved = vec![outer.clone()];
  for inner in nested {
    resolved.extend(
      resolve_override(inner, base_dir, depth + 1)
        .into_iter()
        .map(|r| r.within(&outer)),
    );
  }
  resolved.push(ResolvedOverride::new(
    file_patterns,
    ignore_patterns,
    own_rules,
    None,
  ));
  resolved
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
  /// An empty resolver with cold config and directory caches.
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
    self.find_config_path_for_dir(dir)
  }

  /// The config file that applies to files in `dir`, as [`find_config`]
  /// would find it.
  ///
  /// A directory's answer is its own config file when it has one and its
  /// parent's answer otherwise, so every directory passed on the way up is
  /// cached too.  Each directory is examined at most once per resolver, no
  /// matter how many of its descendants are asked about.
  fn find_config_path_for_dir(&mut self, dir: PathBuf) -> Option<PathBuf> {
    let mut visited: Vec<PathBuf> = Vec::new();
    let mut cur = dir;
    let found = loop {
      if let Some(cached) = self.dir_cache.get(&cur) {
        break cached.clone();
      }
      let own = config_in_dir(&cur);
      visited.push(cur.clone());
      if own.is_some() {
        break own;
      }
      // The same step `find_config` takes, so both visit the same chain.
      if !cur.pop() {
        break None;
      }
    };
    for dir in visited {
      self.dir_cache.insert(dir, found.clone());
    }
    found
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
      let config_arc = if let Some(config_path) = self.find_config_path_for_dir(dir.clone()) {
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
  /// Same as [`ConfigResolver::new`].
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
    assert_eq!(cfg.formatter, None);
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
    assert_eq!(cfg.formatter.as_deref(), Some("json"));
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
    assert_eq!(cfg.formatter.as_deref(), Some("compact"));
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
    assert_eq!(cfg.formatter.as_deref(), Some("text"));
  }

  // -----------------------------------------------------------------------
  // Preset / extends tests
  // -----------------------------------------------------------------------

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
    assert_eq!(cfg.rules.len(), recommended_rule_names().len());
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
  fn unresolved_extends_warnings_name_the_entry() {
    let dir = Path::new("/project");
    assert_eq!(
      unresolved_extends_warning("stylelint-config-missing", dir),
      "warning: could not resolve extends \"stylelint-config-missing\" from /project \
       (is it installed?), so its rules were skipped"
    );
    assert!(
      unresolved_extends_warning(UNEVALUATED_EXTENDS, dir)
        .starts_with("warning: could not resolve an extends entry in /project: it is not a string")
    );
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
  fn overrides_inside_an_overrides_extends_apply_to_files_both_match() {
    // The shape of stylelint-config-standard-vue: an override for `*.vue`
    // extends a config whose own Vue rules sit in an override.
    let tmp = tempfile::tempdir().unwrap();
    let nm = tmp.path().join("node_modules");
    let write = |rel: &str, body: &str| {
      let path = nm.join(rel);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, body).unwrap();
    };
    write(
      "base-config/index.json",
      r#"{ "rules": { "selector-pseudo-class-no-unknown": true, "color-named": "never" } }"#,
    );
    write(
      "vue-config/index.json",
      r#"{ "overrides": [
        { "files": ["*.vue", "**/*.vue", "**/*.html"], "extends": ["base-config"],
          "rules": { "selector-pseudo-class-no-unknown": [true, { "ignorePseudoClasses": ["deep"] }] } }
      ] }"#,
    );
    let raw: ConfigFile = serde_json::from_str(
      r#"{ "overrides": [
        { "files": ["**/*.vue"], "extends": ["base-config", "vue-config"],
          "rules": { "color-named": null } }
      ] }"#,
    )
    .unwrap();
    let cfg = resolve_raw(raw, tmp.path());

    let vue = cfg.rules_for_file("src/App.vue");
    // The nested override beats the outer override's extended rules...
    assert_eq!(
      vue["selector-pseudo-class-no-unknown"].options,
      Some(serde_json::json!({ "ignorePseudoClasses": ["deep"] }))
    );
    // ...and the outer override's own rules beat both.
    assert!(!vue.contains_key("color-named"));

    // The nested override names `*.html`, but only reaches files the outer
    // `*.vue` override matches.
    assert!(cfg.rules_for_file("src/index.html").is_empty());
    assert!(cfg.rules_for_file("src/a.css").is_empty());
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

  #[test]
  fn resolver_memo_agrees_with_find_config_in_any_order() {
    // root/.stylelintrc.json
    // root/a/package.json            (no "stylelint" field)
    // root/a/b/package.json          ("stylelint" field)
    // root/a/b/c/                    (inherits a/b)
    // root/d/e/                      (inherits root)
    // root/x/gale.json + x/y/z/      (inherits x)
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join(".stylelintrc.json"), "{}").unwrap();
    let dirs: Vec<PathBuf> = ["a", "a/b", "a/b/c", "d", "d/e", "x", "x/y", "x/y/z"]
      .iter()
      .map(|d| root.join(d))
      .collect();
    for dir in &dirs {
      std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(root.join("a/package.json"), r#"{"name": "a"}"#).unwrap();
    std::fs::write(
      root.join("a/b/package.json"),
      r#"{"stylelint": {"rules": {}}}"#,
    )
    .unwrap();
    std::fs::write(root.join("x/gale.json"), "{}").unwrap();

    let mut all = vec![root.to_path_buf()];
    all.extend(dirs);
    let expected: Vec<Option<PathBuf>> = all.iter().map(|d| find_config(d)).collect();
    assert_eq!(expected[3], Some(root.join("a/b/package.json")));
    assert_eq!(expected[5], Some(root.join(".stylelintrc.json")));
    assert_eq!(expected[8], Some(root.join("x/gale.json")));

    // Deepest first fills the cache from below; shallowest first from above.
    for order in [all.iter().rev().collect::<Vec<_>>(), all.iter().collect()] {
      let mut resolver = ConfigResolver::new();
      for dir in order {
        let i = all.iter().position(|d| d == dir).unwrap();
        assert_eq!(
          resolver.find_config_path_for_dir(dir.clone()),
          expected[i],
          "{}",
          dir.display()
        );
      }
    }
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
