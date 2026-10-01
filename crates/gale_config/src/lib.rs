use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod discovery;
mod extends;
mod js;
mod model;
mod plugins;
mod presets;
mod raw;

pub use discovery::{find_config, find_config_for_file, load_config};
pub use model::{ConfigError, FixMode, GaleConfig, ResolvedOverride, RuleConfig, Severity};
pub use plugins::{is_known_plugin, is_known_plugin_rule};
pub use presets::{recommended_rule_names, resolve_preset};
pub use raw::{ConfigFile, ConfigOverride, RuleConfigValue};

use discovery::config_in_dir;
use extends::collect_rules_from_extends;
use plugins::extract_plugin_names;
use raw::has_explicit_severity_in_value;

/// Convert a raw [`ConfigFile`] into the resolved [`GaleConfig`].
///
/// `base_dir` is used when resolving `extends` that reference npm packages or
/// relative paths.
pub(crate) fn resolve_raw(raw: ConfigFile, base_dir: &Path) -> GaleConfig {
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
