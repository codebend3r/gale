//! Nearest-config lookup for runs that span several configs, as in a
//! monorepo: each directory maps to its nearest config file, with directory
//! lookups cached.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::discovery::config_in_dir;

/// A directory-to-config-file lookup that caches every directory it visits.
///
/// In monorepo-style projects (like Carbon), different subdirectories may have
/// their own stylelint config that overrides the root config.  This resolver
/// finds each directory's nearest config file (matching Stylelint's
/// cosmiconfig behaviour) while examining each directory at most once.
#[derive(Default)]
pub(crate) struct ConfigResolver {
  /// Cache: directory path -> config file path (or None if no config found).
  dir_cache: HashMap<PathBuf, Option<PathBuf>>,
}

impl ConfigResolver {
  /// The config file that applies to files in `dir`, as [`find_config`](crate::find_config)
  /// would find it.
  ///
  /// A directory's answer is its own config file when it has one and its
  /// parent's answer otherwise, so every directory passed on the way up is
  /// cached too.  Each directory is examined at most once per resolver, no
  /// matter how many of its descendants are asked about.
  pub(crate) fn find_config_path_for_dir(&mut self, dir: PathBuf) -> Option<PathBuf> {
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
    std::fs::write(tmp.path().join(".stylelintrc.json"), "{}").unwrap();
    let sub = tmp.path().join("packages").join("web-components");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join(".stylelintrc.json"), "{}").unwrap();

    // A directory under the sub-package should use the sub-package config.
    let mut resolver = ConfigResolver::default();
    assert_eq!(
      resolver.find_config_path_for_dir(sub.join("src/components")),
      Some(sub.join(".stylelintrc.json"))
    );

    // A directory at the root should use the root config.
    assert_eq!(
      resolver.find_config_path_for_dir(tmp.path().join("src")),
      Some(tmp.path().join(".stylelintrc.json"))
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
      let mut resolver = ConfigResolver::default();
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
