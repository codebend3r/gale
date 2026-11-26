//! Resolving `extends`: built-in presets, npm packages and relative files,
//! merged in order with later entries winning.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::discovery::load_config_file;
use crate::module_resolution::{
  CandidateKind, PackageCandidates, PathCandidates, package_candidates, path_candidates,
};

use crate::raw::UNEVALUATED_EXTENDS;
use crate::{
  ConfigError, ConfigFile, ConfigOverride, RuleConfig, RuleConfigValue, Severity, resolve_preset,
};

/// A loaded config retains its identity and the directory its imports use.
struct LoadedConfig {
  config: ConfigFile,
  path: PathBuf,
}

impl LoadedConfig {
  fn directory(&self) -> &Path {
    self.path.parent().expect("a config file has a parent")
  }
}

/// An inherited override resolves imports relative to the config declaring it.
pub(crate) struct ExtendedOverride {
  pub entry: ConfigOverride,
  pub directory: PathBuf,
}

/// What a relative `extends` entry is tried as: the path as written, with
/// a config extension, or a directory's index file.
const RELATIVE_CANDIDATES: PathCandidates = PathCandidates {
  extensions: &[".js", ".json", ".cjs", ".mjs"],
  index_files: &["index.js", "index.json"],
};

/// What an npm `extends` entry is tried as after its manifest's `exports`
/// and `main`: a subpath with a config extension (no index file), or for
/// the package itself an index or a config file at its root.
const PACKAGE_CANDIDATES: PackageCandidates = PackageCandidates {
  subpath: PathCandidates {
    extensions: &[".js", ".json", ".cjs", ".mjs"],
    index_files: &[],
  },
  index_files: &[
    "index.json",
    "index.js",
    "stylelint.config.js",
    "stylelint.config.cjs",
    "stylelint.config.mjs",
    ".stylelintrc.json",
    ".stylelintrc",
  ],
};

/// Resolve a relative path (starting with `./` or `../`) to a [`ConfigFile`]:
/// the first of [`RELATIVE_CANDIDATES`] that loads.
///
/// The path as written and the index files are loaded without first
/// checking that they are files, as they always have been; a missing
/// JavaScript candidate then warns that it could not be loaded.
fn resolve_relative_config(rel_path: &str, base_dir: &Path) -> Option<LoadedConfig> {
  path_candidates(base_dir, rel_path, &RELATIVE_CANDIDATES).find_map(|candidate| {
    let must_be_file = candidate.kind == CandidateKind::WithExtension;
    load_if(&candidate.path, must_be_file)
  })
}

/// Try to load a [`ConfigFile`] from an npm package installed in
/// `node_modules`: the first of its [`PACKAGE_CANDIDATES`] that loads.
///
/// Supports subpath imports like `@scope/package/subpath` which resolves to
/// `node_modules/@scope/package/subpath.js` (or `.json`).  `main` and the
/// package's index files are loaded without first checking that they are
/// files, as they always have been.
fn resolve_npm_config(package_name: &str, base_dir: &Path) -> Option<LoadedConfig> {
  package_candidates(base_dir, package_name, &PACKAGE_CANDIDATES).find_map(|candidate| {
    let must_be_file = !matches!(candidate.kind, CandidateKind::Main | CandidateKind::Index);
    load_if(&candidate.path, must_be_file)
  })
}

/// [`load_candidate`], skipping a path that is not a file when
/// `must_be_file`.
fn load_if(path: &Path, must_be_file: bool) -> Option<LoadedConfig> {
  if must_be_file && !path.is_file() {
    return None;
  }
  load_candidate(path)
}

/// Load the config at `path`, one of the files an `extends` entry may name.
///
/// Most candidates simply do not exist, so failing to load one is not worth
/// mentioning; a JavaScript config that exists but throws is, since its
/// rules are about to be skipped for a reason the user can fix.
fn load_candidate(path: &Path) -> Option<LoadedConfig> {
  match load_config_file(path) {
    Ok(config) => Some(LoadedConfig {
      config,
      path: path.canonicalize().ok()?,
    }),
    Err(ConfigError::JavaScript(message)) => {
      warn_once(format!(
        "warning: could not load {}: {message}",
        path.display()
      ));
      None
    }
    Err(_) => None,
  }
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
      "warning: could not resolve an extends entry in {}: it is not a string, \
       so its rules were skipped",
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
pub(crate) fn collect_rules_from_extends(
  extends: &[String],
  base_dir: &Path,
  active: &mut HashSet<PathBuf>,
) -> (HashMap<String, RuleConfig>, Vec<ExtendedOverride>) {
  let mut rules = HashMap::new();
  let mut overrides = Vec::new();

  for preset_name in extends {
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
      // A path: relative, or absolute as `require.resolve` and
      // `path.resolve` give it when the config runs under Node.js.
      let is_path = preset_name.starts_with("./")
        || preset_name.starts_with("../")
        || Path::new(preset_name).is_absolute();
      let config = if is_path {
        resolve_relative_config(preset_name, base_dir)
      } else {
        resolve_npm_config(preset_name, base_dir)
      };

      if let Some(loaded) = config {
        if !active.insert(loaded.path.clone()) {
          continue;
        }
        let sub_base = loaded.directory().to_path_buf();
        let config = loaded.config;

        // Recursively resolve this config's extends first
        if let Some(ref sub_extends) = config.extends {
          let (sub_rules, sub_overrides) =
            collect_rules_from_extends(sub_extends, &sub_base, active);
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
          overrides.extend(config_overrides.into_iter().map(|entry| ExtendedOverride {
            entry,
            directory: sub_base.clone(),
          }));
        }
        active.remove(&loaded.path);
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

#[cfg(test)]
mod tests {
  use super::*;

  fn write(root: &Path, name: &str, json: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, json).unwrap();
  }

  fn collect(root: &Path, entries: &[&str]) -> HashMap<String, RuleConfig> {
    collect_rules_from_extends(
      &entries
        .iter()
        .map(|entry| entry.to_string())
        .collect::<Vec<_>>(),
      root,
      &mut HashSet::new(),
    )
    .0
  }

  #[test]
  fn relative_entries_are_identified_by_the_resolved_file() {
    let dir = tempfile::tempdir().unwrap();
    for (folder, rule) in [("one", "block-no-empty"), ("two", "color-no-invalid-hex")] {
      write(
        dir.path(),
        &format!("{folder}/config.json"),
        r#"{"extends":"./base.json"}"#,
      );
      write(
        dir.path(),
        &format!("{folder}/base.json"),
        &format!(r#"{{"rules":{{"{rule}":true}}}}"#),
      );
    }
    let rules = collect(dir.path(), &["./one/config.json", "./two/config.json"]);
    assert!(rules.contains_key("block-no-empty"));
    assert!(rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn cycles_stop_but_shared_ancestors_apply_in_order() {
    let dir = tempfile::tempdir().unwrap();
    write(
      dir.path(),
      "a.json",
      r#"{"extends":"./b.json","rules":{"block-no-empty":true}}"#,
    );
    write(
      dir.path(),
      "b.json",
      r#"{"extends":"./a.json","rules":{"color-no-invalid-hex":true}}"#,
    );
    assert_eq!(collect(dir.path(), &["./a.json"]).len(), 2);
    write(
      dir.path(),
      "base.json",
      r#"{"rules":{"block-no-empty":true}}"#,
    );
    write(
      dir.path(),
      "left.json",
      r#"{"extends":"./base.json","rules":{"block-no-empty":false}}"#,
    );
    write(dir.path(), "right.json", r#"{"extends":"./base.json"}"#);
    assert_ne!(
      collect(dir.path(), &["./left.json", "./right.json"])["block-no-empty"].severity,
      Some(Severity::Off)
    );
  }

  #[test]
  fn npm_imports_use_the_actual_entry_directory() {
    let dir = tempfile::tempdir().unwrap();
    // A closer node_modules exists, but the package is found above it.
    std::fs::create_dir_all(dir.path().join("project/node_modules")).unwrap();
    write(
      dir.path(),
      "node_modules/shared/package.json",
      r#"{"main":"configs/index.json"}"#,
    );
    write(
      dir.path(),
      "node_modules/shared/configs/index.json",
      r#"{"extends":"./base.json","overrides":[{"files":"*.scss","extends":"./base.json"}]}"#,
    );
    write(
      dir.path(),
      "node_modules/shared/configs/base.json",
      r#"{"rules":{"block-no-empty":true}}"#,
    );
    let (rules, overrides) = collect_rules_from_extends(
      &["shared".into()],
      &dir.path().join("project"),
      &mut HashSet::new(),
    );
    assert!(rules.contains_key("block-no-empty"));
    assert_eq!(
      overrides[0].directory,
      dir
        .path()
        .join("node_modules/shared/configs")
        .canonicalize()
        .unwrap()
    );
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
}
