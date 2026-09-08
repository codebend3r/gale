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
const CACHE_FORMAT_VERSION: u32 = 1;

/// On-disk representation of the lint cache.
#[derive(Debug, Serialize, Deserialize)]
pub struct LintCache {
    /// Format version of this cache file.
    #[serde(default)]
    pub version: u32,
    /// Rust compiler version that produced the hashes in this file.
    #[serde(default)]
    pub hasher_id: String,
    /// Map from canonical file path to its cache entry.
    pub entries: HashMap<String, CacheEntry>,
}

impl Default for LintCache {
    fn default() -> Self {
        Self {
            version: CACHE_FORMAT_VERSION,
            hasher_id: hasher_id(),
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
    /// Load the cache from disk. Returns an empty cache on any error.
    pub fn load(path: &Path) -> Self {
        let Ok(data) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let cache: Self = match serde_json::from_str(&data) {
            Ok(cache) => cache,
            Err(err) => {
                debug!("Failed to parse cache file: {err}");
                return Self::default();
            }
        };
        if cache.version != CACHE_FORMAT_VERSION || cache.hasher_id != hasher_id() {
            debug!("Discarding cache written by a different Gale build");
            return Self::default();
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
        self.entries
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
        assert!(LintCache::load(&path).is_clean("a.css", 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
