//! Resolving `extends`: built-in presets, npm packages and relative files,
//! merged in order with later entries winning.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::discovery::load_config_file;
use crate::raw::UNEVALUATED_EXTENDS;
use crate::{ConfigFile, ConfigOverride, RuleConfig, RuleConfigValue, Severity, resolve_preset};

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
pub(crate) fn collect_rules_from_extends(
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

#[cfg(test)]
mod tests {
  use super::*;

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
