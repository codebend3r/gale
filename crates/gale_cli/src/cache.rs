use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::debug;

/// Bumped whenever the on-disk shape or the hashing scheme changes.
///
/// `DefaultHasher`'s algorithm is explicitly unspecified across Rust releases,
/// so hashes are only meaningful to the binary that wrote them. A mismatch
/// here discards the cache rather than trusting stale hashes.
const CACHE_FORMAT_VERSION: u32 = 3;

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
  /// Parse a `--cache-strategy` value.
  pub fn parse(name: &str) -> Option<Self> {
    match name {
      "metadata" => Some(Self::Metadata),
      "content" => Some(Self::Content),
      _ => None,
    }
  }
}

/// On-disk representation of the lint cache.
#[derive(Debug, Serialize, Deserialize)]
pub struct LintCache {
  /// Format version of this cache file.
  #[serde(default)]
  pub version: u32,
  /// Rust compiler version that produced the hashes in this file.
  #[serde(default)]
  pub hasher_id: String,
  /// The strategy the fingerprints were computed with.  Switching strategy
  /// discards the cache rather than comparing unlike fingerprints.
  #[serde(default)]
  pub strategy: CacheStrategy,
  /// Map from canonical file path to its cache entry.
  pub entries: HashMap<String, CacheEntry>,
}

impl Default for LintCache {
  /// An empty cache using the default fingerprint strategy.
  fn default() -> Self {
    Self::new(CacheStrategy::default())
  }
}

impl LintCache {
  /// An empty cache that will fingerprint files with `strategy`.
  pub fn new(strategy: CacheStrategy) -> Self {
    Self {
      version: CACHE_FORMAT_VERSION,
      hasher_id: hasher_id(),
      strategy,
      entries: HashMap::new(),
    }
  }
}

/// Identifies the hashing environment. `DefaultHasher` is only stable within a
/// single Rust release, so the toolchain version is part of the cache identity.
fn hasher_id() -> String {
  // A cheap, stable probe: hash a fixed string and record the result. If the
  // std hasher ever changes, this value changes with it.
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  "gale-cache-probe".hash(&mut hasher);
  format!("{:x}", hasher.finish())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry {
  /// Hash of the file contents (combined with config hash).
  pub hash: u64,
  /// Number of diagnostics the linter reported for this file.
  pub diagnostics_count: usize,
}

impl LintCache {
  /// Load the cache from disk for `strategy`.  Returns an empty cache on
  /// any error, or when the file was written by a different build or with
  /// a different strategy.
  pub fn load(path: &Path, strategy: CacheStrategy) -> Self {
    let Ok(data) = std::fs::read_to_string(path) else {
      return Self::new(strategy);
    };
    let cache: Self = match serde_json::from_str(&data) {
      Ok(cache) => cache,
      Err(err) => {
        debug!("Failed to parse cache file: {err}");
        return Self::new(strategy);
      }
    };
    if cache.version != CACHE_FORMAT_VERSION || cache.hasher_id != hasher_id() {
      debug!("Discarding cache written by a different Gale build");
      return Self::new(strategy);
    }
    if cache.strategy != strategy {
      debug!("Discarding cache written with a different cache strategy");
      return Self::new(strategy);
    }
    cache
  }

  /// Save the cache to disk, creating the parent directory if needed so a
  /// `cacheLocation` such as `tmp/lint.cache` works on a fresh checkout.
  pub fn save(&self, path: &Path) {
    match serde_json::to_string(self) {
      Ok(data) => {
        if let Some(parent) = path.parent()
          && !parent.as_os_str().is_empty()
          && let Err(err) = std::fs::create_dir_all(parent)
        {
          debug!("Failed to create cache directory: {err}");
        }
        if let Err(err) = std::fs::write(path, data) {
          debug!("Failed to write cache file: {err}");
        }
      }
      Err(err) => {
        debug!("Failed to serialize cache: {err}");
      }
    }
  }

  /// Check whether a file can be skipped (hash matches and had 0 diagnostics).
  pub fn is_clean(&self, file_key: &str, content_hash: u64) -> bool {
    self
      .entries
      .get(file_key)
      .is_some_and(|e| e.hash == content_hash && e.diagnostics_count == 0)
  }

  /// Record a lint result in the cache.
  pub fn record(&mut self, file_key: String, content_hash: u64, diagnostics_count: usize) {
    self.entries.insert(
      file_key,
      CacheEntry {
        hash: content_hash,
        diagnostics_count,
      },
    );
  }
}

/// Compute a fast hash of the file contents combined with the config hash.
///
/// The crate version is folded in so that upgrading Gale invalidates the whole
/// cache. Without it, files that were clean under the old binary stay skipped
/// and rules added or fixed in the new version never run on them.
pub fn compute_hash(contents: &str, config_hash: u64) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  contents.hash(&mut hasher);
  config_hash.hash(&mut hasher);
  env!("CARGO_PKG_VERSION").hash(&mut hasher);
  hasher.finish()
}

/// The fingerprint that identifies an unchanged file without reading it:
/// under the `Metadata` strategy (Stylelint's default), a hash of the
/// modification time and size.
///
/// `None` for the `Content` strategy, or when the metadata cannot be read;
/// the caller then fingerprints the contents with [`compute_hash`], so a
/// file is never wrongly skipped.
pub fn metadata_fingerprint(path: &Path, strategy: CacheStrategy, config_hash: u64) -> Option<u64> {
  if strategy != CacheStrategy::Metadata {
    return None;
  }
  let meta = std::fs::metadata(path).ok()?;
  let modified = meta
    .modified()
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|d| d.as_nanos())
    .unwrap_or_default();
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  modified.hash(&mut hasher);
  meta.len().hash(&mut hasher);
  config_hash.hash(&mut hasher);
  env!("CARGO_PKG_VERSION").hash(&mut hasher);
  Some(hasher.finish())
}

/// Feed rules into `hasher` in name order, so the hash does not depend on
/// `HashMap` iteration order.
fn hash_rules(rules: &HashMap<String, gale_config::RuleConfig>, hasher: &mut impl Hasher) {
  let mut sorted: Vec<&String> = rules.keys().collect();
  sorted.sort();
  sorted.len().hash(hasher);
  for name in sorted {
    name.hash(hasher);
    let rc = &rules[name];
    // Hash severity (use debug repr for determinism).
    format!("{:?}", rc.severity).hash(hasher);
    // Hash options via JSON serialization.
    rc.options
      .as_ref()
      .map(|opts| serde_json::to_string(opts).unwrap_or_default())
      .hash(hasher);
  }
}

/// Feed per-rule options into `hasher` in name order.
fn hash_rule_options(options: &HashMap<String, serde_json::Value>, hasher: &mut impl Hasher) {
  let mut sorted: Vec<&String> = options.keys().collect();
  sorted.sort();
  sorted.len().hash(hasher);
  for name in sorted {
    name.hash(hasher);
    serde_json::to_string(&options[name])
      .unwrap_or_default()
      .hash(hasher);
  }
}

/// Compute a stable hash of everything in a resolved config that can change
/// a file's lint result, so cache entries are invalidated when any of it
/// changes: the rules and their options, the overrides, the syntax, and the
/// disable-comment switches.
///
/// File discovery, output and caching settings are left out because they
/// do not change what a linted file reports.
pub fn compute_config_hash(config: &gale_config::GaleConfig) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  hash_rules(&config.rules, &mut hasher);
  config.overrides.len().hash(&mut hasher);
  for ov in &config.overrides {
    ov.file_patterns.hash(&mut hasher);
    ov.ignore_patterns().collect::<Vec<_>>().hash(&mut hasher);
    hash_rules(&ov.rules, &mut hasher);
    ov.custom_syntax.hash(&mut hasher);
  }
  // Override globs are matched relative to the config's directory.
  config.config_dir.hash(&mut hasher);
  config.custom_syntax.hash(&mut hasher);
  format!("{:?}", config.default_severity).hash(&mut hasher);
  [
    config.report_needless_disables,
    config.report_invalid_scope_disables,
    config.report_descriptionless_disables,
    config.report_unscoped_disables,
    config.ignore_disables,
  ]
  .hash(&mut hasher);
  hasher.finish()
}

/// The cache key part for files linted with `config`, given the options
/// the runner receives for it (which may carry injected compatibility
/// settings).
pub fn compute_params_hash(
  config: &gale_config::GaleConfig,
  rule_options: &HashMap<String, serde_json::Value>,
) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  compute_config_hash(config).hash(&mut hasher);
  hash_rule_options(rule_options, &mut hasher);
  hasher.finish()
}

/// The cache key part shared by every file in a run: the root config and
/// its runner options (the runner falls back on both), the forced
/// `--custom-syntax`, and the command-line switches that change results
/// (the four disable reports and `--ignore-disables`).
pub fn compute_run_hash(
  root: &gale_config::GaleConfig,
  root_rule_options: &HashMap<String, serde_json::Value>,
  custom_syntax: Option<&str>,
  cli_switches: [bool; 5],
) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  compute_params_hash(root, root_rule_options).hash(&mut hasher);
  custom_syntax.hash(&mut hasher);
  cli_switches.hash(&mut hasher);
  hasher.finish()
}

/// Combine cache key parts into one hash.
pub fn combine_hashes(parts: &[u64]) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  parts.hash(&mut hasher);
  hasher.finish()
}

/// Resolve the cache file path from CLI options.
pub fn resolve_cache_path(custom: Option<&Path>) -> PathBuf {
  if let Some(p) = custom {
    p.to_path_buf()
  } else {
    std::env::current_dir()
      .unwrap_or_else(|_| PathBuf::from("."))
      .join(".gale_cache")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn save_creates_missing_parent_directories() {
    let dir = std::env::temp_dir().join(format!("gale-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("nested").join("lint.cache");

    let mut cache = LintCache::default();
    cache.record("a.css".to_string(), 1, 0);
    cache.save(&path);

    assert!(path.is_file(), "cache file was not written");
    assert!(LintCache::load(&path, CacheStrategy::Metadata).is_clean("a.css", 1));
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn switching_strategy_discards_the_cache() {
    let dir = std::env::temp_dir().join(format!("gale-cache-strategy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("lint.cache");

    let mut cache = LintCache::new(CacheStrategy::Content);
    cache.record("a.css".to_string(), 1, 0);
    cache.save(&path);

    assert!(LintCache::load(&path, CacheStrategy::Content).is_clean("a.css", 1));
    assert!(!LintCache::load(&path, CacheStrategy::Metadata).is_clean("a.css", 1));
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn strategies_parse_and_fingerprint_differently() {
    assert_eq!(
      CacheStrategy::parse("metadata"),
      Some(CacheStrategy::Metadata)
    );
    assert_eq!(
      CacheStrategy::parse("content"),
      Some(CacheStrategy::Content)
    );
    assert_eq!(CacheStrategy::parse("bogus"), None);
    assert_eq!(CacheStrategy::default(), CacheStrategy::Metadata);

    let dir = std::env::temp_dir().join(format!("gale-fingerprint-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.css");
    std::fs::write(&file, "a {}").unwrap();

    let content_before = compute_hash("a {}", 7);
    let meta_before = metadata_fingerprint(&file, CacheStrategy::Metadata, 7).unwrap();
    assert_eq!(metadata_fingerprint(&file, CacheStrategy::Content, 7), None);

    // A touch that keeps the content changes only the metadata fingerprint.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
      .write(true)
      .open(&file)
      .unwrap()
      .set_modified(later)
      .unwrap();
    assert_eq!(compute_hash("a {}", 7), content_before);
    assert_ne!(
      metadata_fingerprint(&file, CacheStrategy::Metadata, 7),
      Some(meta_before)
    );
    let _ = std::fs::remove_dir_all(&dir);
  }

  /// A rule config with an optional severity and options.
  fn rule(options: Option<serde_json::Value>) -> gale_config::RuleConfig {
    gale_config::RuleConfig {
      severity: Some(gale_config::Severity::Error),
      options,
    }
  }

  #[test]
  fn config_hash_ignores_rule_order_but_not_rule_content() {
    let names = [
      "block-no-empty",
      "color-named",
      "max-nesting-depth",
      "unit-no-unknown",
    ];
    let mut forward = gale_config::GaleConfig::default();
    for name in names {
      forward.rules.insert(name.to_string(), rule(None));
    }
    let mut backward = gale_config::GaleConfig::default();
    for name in names.iter().rev() {
      backward.rules.insert(name.to_string(), rule(None));
    }
    assert_eq!(
      compute_config_hash(&forward),
      compute_config_hash(&backward)
    );

    let mut with_options = forward.clone();
    with_options.rules.insert(
      "max-nesting-depth".to_string(),
      rule(Some(serde_json::json!(3))),
    );
    assert_ne!(
      compute_config_hash(&forward),
      compute_config_hash(&with_options)
    );
  }

  #[test]
  fn config_hash_covers_overrides_syntax_and_disable_switches() {
    let base = gale_config::GaleConfig::default();
    let hash = compute_config_hash(&base);
    let changed: Vec<gale_config::GaleConfig> = vec![
      gale_config::GaleConfig {
        overrides: vec![gale_config::ResolvedOverride::new(
          vec!["**/*.css".to_string()],
          vec![],
          HashMap::from([("color-named".to_string(), rule(None))]),
          None,
        )],
        ..base.clone()
      },
      gale_config::GaleConfig {
        custom_syntax: Some("postcss-scss".to_string()),
        ..base.clone()
      },
      gale_config::GaleConfig {
        default_severity: Some(gale_config::Severity::Warning),
        ..base.clone()
      },
      gale_config::GaleConfig {
        report_needless_disables: true,
        ..base.clone()
      },
      gale_config::GaleConfig {
        report_invalid_scope_disables: true,
        ..base.clone()
      },
      gale_config::GaleConfig {
        report_descriptionless_disables: true,
        ..base.clone()
      },
      gale_config::GaleConfig {
        report_unscoped_disables: true,
        ..base.clone()
      },
      gale_config::GaleConfig {
        ignore_disables: true,
        ..base.clone()
      },
    ];
    for config in &changed {
      assert_ne!(compute_config_hash(config), hash, "{config:?}");
    }

    // Overrides differing only in their ignoreFiles patterns differ too.
    let with_ignore = |ignore: Vec<String>| gale_config::GaleConfig {
      overrides: vec![gale_config::ResolvedOverride::new(
        vec!["**/*.css".to_string()],
        ignore,
        HashMap::new(),
        None,
      )],
      ..base.clone()
    };
    assert_ne!(
      compute_config_hash(&with_ignore(vec![])),
      compute_config_hash(&with_ignore(vec!["vendor/**".to_string()]))
    );

    // Output and discovery settings do not change a file's result.
    let quiet = gale_config::GaleConfig {
      quiet: true,
      ignore_patterns: vec!["dist/**".to_string()],
      ..base.clone()
    };
    assert_eq!(compute_config_hash(&quiet), hash);
  }

  #[test]
  fn run_hash_covers_command_line_switches() {
    let config = gale_config::GaleConfig::default();
    let options = HashMap::new();
    let base = compute_run_hash(&config, &options, None, [false; 5]);
    assert_ne!(
      compute_run_hash(&config, &options, Some("postcss-less"), [false; 5]),
      base
    );
    for i in 0..5 {
      let mut switches = [false; 5];
      switches[i] = true;
      assert_ne!(compute_run_hash(&config, &options, None, switches), base);
    }
    let injected = HashMap::from([(
      "unit-no-unknown".to_string(),
      serde_json::json!({"__additionalUnknownUnits": ["dvh"]}),
    )]);
    assert_ne!(compute_run_hash(&config, &injected, None, [false; 5]), base);
  }
}
