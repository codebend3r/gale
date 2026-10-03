//! The resolved configuration gale lints with, and the error type for
//! loading one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

/// Rule severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  Error,
  Warning,
  Off,
}

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
  pub(crate) fn within(mut self, outer: &ResolvedOverride) -> Self {
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
}
