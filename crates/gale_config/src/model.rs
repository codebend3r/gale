//! The resolved configuration gale lints with, and the error type for
//! loading one.

use std::borrow::Cow;
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

  /// A JavaScript config that could not be evaluated: it threw, failed to
  /// import, or does not export an object.
  #[error("{0}")]
  JavaScript(String),
}

/// Rule severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  Error,
  Warning,
  Off,
}

impl Severity {
  /// The severity problems are reported at, or `None` for a rule that is
  /// off.
  pub fn reported(self) -> Option<gale_diagnostics::Severity> {
    match self {
      Severity::Error => Some(gale_diagnostics::Severity::Error),
      Severity::Warning => Some(gale_diagnostics::Severity::Warning),
      Severity::Off => None,
    }
  }
}

/// A single rule's resolved configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleConfig {
  pub severity: Option<Severity>,
  /// The rule's setting as the config wrote it, less gale's
  /// `["<severity>", setting]` wrapper: `"always"`, `["always", {...}]`,
  /// `[true, {...}]` or a bare `{...}`.  `None` for a rule set to `true` or
  /// to a severity.  The linter's `Setting` splits it into the primary and
  /// secondary options, as Stylelint's `normalizeRuleSettings` does.
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

/// How the cache decides that a file is unchanged: Stylelint's
/// `--cache-strategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheStrategy {
  /// The file's modification time and size are unchanged (the default, as
  /// in Stylelint).  Cheap, but a touch re-lints the file.
  #[default]
  Metadata,
  /// The file's content hash is unchanged.  Survives a touch or a fresh
  /// checkout with identical contents.
  Content,
}

impl CacheStrategy {
  /// Parse a `--cache-strategy` or `cacheStrategy` value.
  pub fn parse(name: &str) -> Option<Self> {
    match name {
      "metadata" => Some(Self::Metadata),
      "content" => Some(Self::Content),
      _ => None,
    }
  }
}

/// How autofix behaves when it is switched on from the config file.
///
/// Mirrors the `--fix` flag.  Stylelint's `strict` skips files with a syntax
/// error and `lax` fixes them anyway; gale's parser recovers from every
/// syntax error, so the two fix alike.
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
  /// Directory containing the config file.  Used by [`GaleConfig::rules_for_file`] to
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
  /// Stylelint's `cacheStrategy`, parsed; `Err` holds a name that is
  /// neither `"metadata"` nor `"content"`.  The CLI, which owns the cache,
  /// reports that only when no `--cache-strategy` overrides it.
  pub cache_strategy: Option<Result<CacheStrategy, String>>,
}

impl GaleConfig {
  /// `file_path` as override globs are matched against it: relative to the
  /// config's directory when the file is inside it, as Stylelint matches,
  /// and as given otherwise.
  fn relative_path<'p>(&self, file_path: &'p str) -> Cow<'p, str> {
    let Some(config_dir) = &self.config_dir else {
      return file_path.into();
    };
    let abs_file = if Path::new(file_path).is_absolute() {
      PathBuf::from(file_path)
    } else {
      std::env::current_dir().unwrap_or_default().join(file_path)
    };
    match abs_file.strip_prefix(config_dir) {
      Ok(rel) => rel.to_string_lossy().into_owned().into(),
      Err(_) => file_path.into(),
    }
  }

  /// What the overrides say about `file_path`, from one pass over them with
  /// its relative path worked out once.
  ///
  /// Each override's globs are tried against the path relative to the
  /// config's directory and against the path as given.
  pub(crate) fn overrides_for_file(&self, file_path: &str) -> OverrideMatches<'_> {
    let mut found = OverrideMatches {
      indices: Vec::new(),
      ignored: false,
      custom_syntax: None,
    };
    if !self.overrides.is_empty() {
      let relative_path = self.relative_path(file_path);
      for (index, ov) in self.overrides.iter().enumerate() {
        let [relative, given] = [relative_path.as_ref(), file_path]
          .map(|path| (ov.matches_files(path), ov.is_excluded(path)));
        // `matches` is `matches_files` and not `is_excluded`, per path.
        let matches = (relative.0 && !relative.1) || (given.0 && !given.1);
        found.ignored |= (relative.0 || given.0) && (relative.1 || given.1);
        if matches {
          found.indices.push(index);
          if found.custom_syntax.is_none() {
            found.custom_syntax = ov.custom_syntax.as_deref();
          }
        }
      }
    }
    found.custom_syntax = found.custom_syntax.or(self.custom_syntax.as_deref());
    found
  }

  /// The positions in [`Self::overrides`] of the overrides that apply to
  /// `file_path`, in order.
  pub fn matching_overrides(&self, file_path: &str) -> Vec<usize> {
    self.overrides_for_file(file_path).indices
  }

  /// The base rules with the overrides at `indices` (from
  /// [`Self::matching_overrides`]) applied in order, later ones winning.
  /// An override that turns a rule off removes it.
  pub fn rules_with_overrides(&self, indices: &[usize]) -> HashMap<String, RuleConfig> {
    let mut rules = self.rules.clone();
    for &index in indices {
      for (name, cfg) in &self.overrides[index].rules {
        if cfg.severity == Some(Severity::Off) {
          rules.remove(name);
        } else {
          rules.insert(name.clone(), cfg.clone());
        }
      }
    }
    rules
  }

  /// Return the effective rules for a given file path: the base rules with
  /// every override that matches it applied.
  pub fn rules_for_file(&self, file_path: &str) -> HashMap<String, RuleConfig> {
    self.rules_with_overrides(&self.matching_overrides(file_path))
  }

  /// The `customSyntax` that applies to `file_path`, if the config names one.
  ///
  /// The first override whose globs match the file wins; otherwise the
  /// top-level `customSyntax` applies.  Whether Gale can actually parse the
  /// named syntax is the caller's decision.
  pub fn custom_syntax_for_file(&self, file_path: &str) -> Option<&str> {
    self.overrides_for_file(file_path).custom_syntax
  }
}

/// What a config's overrides say about one file
/// (see [`GaleConfig::overrides_for_file`]).
pub(crate) struct OverrideMatches<'c> {
  /// The positions in [`GaleConfig::overrides`] of the overrides that
  /// apply, in order.
  pub indices: Vec<usize>,
  /// Whether an override whose `files` match the file lists it in that
  /// same override's `ignoreFiles`.  Stylelint treats such files as fully
  /// ignored (`"ignored": true`): no linting at all.
  pub ignored: bool,
  /// The `customSyntax` of the first applying override that names one,
  /// else the config's top-level one.
  pub custom_syntax: Option<&'c str>,
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
