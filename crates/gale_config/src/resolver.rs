//! Per-file config resolution for runs that span several configs, as in a
//! monorepo: each file gets its nearest config, with config files and
//! directory lookups cached.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::GaleConfig;
use crate::discovery::{config_in_dir, load_config};

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

  /// The config file that applies to files in `dir`, as [`find_config`](crate::find_config)
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::discovery::find_config;

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
}
