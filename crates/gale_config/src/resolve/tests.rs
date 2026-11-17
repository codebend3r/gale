use super::*;
use crate::presets::recommended_rule_names;

#[test]
fn defaults_preserve_inherited_explicit_severities_and_cover_overrides() {
  let dir = tempfile::tempdir().unwrap();
  std::fs::write(
    dir.path().join("base.json"),
    r#"{
      "rules": {"block-no-empty": [true, {"severity":"error"}], "color-no-invalid-hex": true},
      "overrides": [{"files":"*.scss", "rules":{"color-named":"never", "block-no-empty":"error"}}]
    }"#,
  )
  .unwrap();
  let raw =
    serde_json::from_str(r#"{"extends":"./base.json","defaultSeverity":"warning"}"#).unwrap();
  let config = resolve_raw(raw, dir.path());
  assert_eq!(
    config.rules["block-no-empty"].severity,
    Some(Severity::Error)
  );
  assert_eq!(
    config.rules["color-no-invalid-hex"].severity,
    Some(Severity::Warning)
  );
  assert_eq!(
    config.overrides[0].rules["block-no-empty"].severity,
    Some(Severity::Error)
  );
  assert_eq!(
    config.overrides[0].rules["color-named"].severity,
    Some(Severity::Warning)
  );
}

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
    Some(serde_json::json!([true, { "ignorePseudoClasses": ["deep"] }]))
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
