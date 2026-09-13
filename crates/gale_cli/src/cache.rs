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
const CACHE_FORMAT_VERSION: u32 = 2;

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

/// The fingerprint that identifies an unchanged file under `strategy`.
///
/// `Metadata` hashes the modification time and size, as Stylelint's default
/// does; `Content` hashes the source.  When metadata cannot be read the
/// content hash is used, so a file is never wrongly skipped.
pub fn compute_fingerprint(
  path: &Path,
  contents: &str,
  strategy: CacheStrategy,
  config_hash: u64,
) -> u64 {
  match strategy {
    CacheStrategy::Content => compute_hash(contents, config_hash),
    CacheStrategy::Metadata => {
      let Ok(meta) = std::fs::metadata(path) else {
        return compute_hash(contents, config_hash);
      };
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
      hasher.finish()
    }
  }
}

/// Compute a stable hash for the current linter configuration so that cache
/// entries are invalidated when the config changes (including rule options).
pub fn compute_config_hash(rules: &HashMap<String, gale_config::RuleConfig>) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  // Sort rule names for deterministic ordering.
  let mut sorted: Vec<&String> = rules.keys().collect();
  sorted.sort();
  for name in sorted {
    name.hash(&mut hasher);
    let rc = &rules[name];
    // Hash severity (use debug repr for determinism).
    format!("{:?}", rc.severity).hash(&mut hasher);
    // Hash options via canonical JSON serialization.
    if let Some(ref opts) = rc.options {
      serde_json::to_string(opts)
        .unwrap_or_default()
        .hash(&mut hasher);
    }
  }
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

    let content_before = compute_fingerprint(&file, "a {}", CacheStrategy::Content, 7);
    let meta_before = compute_fingerprint(&file, "a {}", CacheStrategy::Metadata, 7);

    // A touch that keeps the content changes only the metadata fingerprint.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
      .write(true)
      .open(&file)
      .unwrap()
      .set_modified(later)
      .unwrap();
    assert_eq!(
      compute_fingerprint(&file, "a {}", CacheStrategy::Content, 7),
      content_before
    );
    assert_ne!(
      compute_fingerprint(&file, "a {}", CacheStrategy::Metadata, 7),
      meta_before
    );
    let _ = std::fs::remove_dir_all(&dir);
  }
}
