//! Turning a raw config into the [`GaleConfig`] gale lints with: `extends`,
//! the config's own rules, overrides, and the CLI-equivalent options.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::discovery::{find_config, find_config_for_file, load_config};
use crate::extends::collect_rules_from_extends;
use crate::plugins::extract_plugin_names;
use crate::raw::has_explicit_severity_in_value;
use crate::{
  ConfigFile, ConfigOverride, FixMode, GaleConfig, ResolvedOverride, RuleConfig, Severity,
};

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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::presets::recommended_rule_names;

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
