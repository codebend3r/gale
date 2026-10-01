use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

mod js;
mod model;
mod presets;
mod raw;

pub use model::{ConfigError, FixMode, GaleConfig, ResolvedOverride, RuleConfig, Severity};
pub use presets::{recommended_rule_names, resolve_preset};
pub use raw::{ConfigFile, ConfigOverride, RuleConfigValue};

use js::parse_js_config;
use raw::{UNEVALUATED_EXTENDS, has_explicit_severity_in_value};

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
