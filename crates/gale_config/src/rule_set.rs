//! Turning a resolved config into the [`RuleSet`] a file is linted with.

use gale_linter::{DisableOptions, LintEnv, RuleRegistry, RuleSet, RuleSetSpec, RuleSetting};

use crate::model::{GaleConfig, RuleConfig};

/// What a run adds to every config's rule sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RunSettings {
  /// Disable-comment switches turned on for the whole run (the command
  /// line's `--ignore-disables` and `--report-*-disables`), on top of each
  /// config's own.
  pub disables: DisableOptions,
  /// What the run knows about the project beyond its configs.
  pub env: LintEnv,
}

/// The rules a run uses for a file that no config file applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithoutConfig {
  /// No rules at all.
  NoRules,
  /// Every rule gale has, each with its default options and severity.
  AllRules,
  /// The rules of the `gale:recommended` preset, with default options.
  Recommended,
}

impl WithoutConfig {
  /// The config standing in for a missing one.
  pub fn config(self) -> GaleConfig {
    let names: Vec<&str> = match self {
      WithoutConfig::NoRules => Vec::new(),
      WithoutConfig::AllRules => RuleRegistry::global()
        .all()
        .iter()
        .map(|rule| rule.name())
        .collect(),
      WithoutConfig::Recommended => crate::recommended_rule_names(),
    };
    GaleConfig {
      rules: names
        .into_iter()
        .map(|name| {
          let rule = RuleConfig {
            severity: None,
            options: None,
          };
          (name.to_string(), rule)
        })
        .collect(),
      ..GaleConfig::default()
    }
  }
}

impl GaleConfig {
  /// The disable-comment switches this config turns on.
  pub fn disable_options(&self) -> DisableOptions {
    DisableOptions {
      ignore: self.ignore_disables,
      report_needless: self.report_needless_disables,
      report_invalid_scope: self.report_invalid_scope_disables,
      report_descriptionless: self.report_descriptionless_disables,
      report_unscoped: self.report_unscoped_disables,
    }
  }

  /// The rule set for files that the overrides at `overrides` (from
  /// [`GaleConfig::matching_overrides`]) apply to.
  ///
  /// Every enabled rule keeps the name, options and severity the config
  /// gives it; rules turned off are left out.  The config's own rule names
  /// all count as configured for `reportInvalidScopeDisables`, so a
  /// disable naming a plugin rule gale lacks is not reported.
  pub fn rule_set(&self, overrides: &[usize], run: &RunSettings) -> RuleSet {
    let rules = self
      .rules_with_overrides(overrides)
      .into_iter()
      .filter_map(|(name, rule)| {
        let severity = match rule.severity {
          Some(severity) => Some(severity.reported()?),
          None => None,
        };
        Some(RuleSetting {
          name,
          options: rule.options,
          severity,
        })
      })
      .collect();
    RuleSet::new(RuleSetSpec {
      rules,
      configured_rules: self.rules.keys().cloned().collect(),
      disables: self.disable_options().union(run.disables),
      default_severity: self.default_severity.and_then(|s| s.reported()),
      env: run.env,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use gale_diagnostics::Severity as Reported;

  fn config(json: &str) -> GaleConfig {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".stylelintrc.json");
    std::fs::write(&path, json).unwrap();
    crate::load_config(&path).unwrap()
  }

  fn rules_reported(set: &RuleSet, source: &str, path: &str) -> Vec<(String, Reported)> {
    set
      .lint(source, path, Syntax::Css)
      .diagnostics
      .into_iter()
      .map(|d| (d.rule_name, d.severity))
      .collect()
  }

  #[test]
  fn a_rule_set_runs_the_rules_the_config_and_its_overrides_enable() {
    let config = config(
      r#"{
        "rules": { "block-no-empty": true, "color-named": ["never", { "severity": "warning" }] },
        "overrides": [{ "files": ["*.module.css"], "rules": { "block-no-empty": null } }]
      }"#,
    );
    let source = "a {}\nb { color: red; }\n";
    let base = config.rule_set(&[], &RunSettings::default());
    assert_eq!(
      rules_reported(&base, source, "a.css"),
      [
        ("block-no-empty".to_string(), Reported::Error),
        ("color-named".to_string(), Reported::Warning)
      ]
    );
    let overrides = config.matching_overrides("a.module.css");
    assert_eq!(overrides, [0]);
    let overridden = config.rule_set(&overrides, &RunSettings::default());
    assert_eq!(
      rules_reported(&overridden, source, "a.module.css"),
      [("color-named".to_string(), Reported::Warning)]
    );
  }

  #[test]
  fn the_run_adds_its_disable_switches_to_the_configs() {
    let config = config(r#"{ "rules": { "block-no-empty": true } }"#);
    let run = RunSettings {
      disables: DisableOptions {
        report_descriptionless: true,
        ..DisableOptions::default()
      },
      ..RunSettings::default()
    };
    let set = config.rule_set(&[], &run);
    assert!(set.disables().report_descriptionless);
    assert!(!set.disables().ignore);
  }

  #[test]
  fn without_a_config_the_stand_in_names_its_rules() {
    assert!(WithoutConfig::NoRules.config().rules.is_empty());
    assert_eq!(
      WithoutConfig::AllRules.config().rules.len(),
      RuleRegistry::global().all().len()
    );
    let recommended = WithoutConfig::Recommended.config();
    assert!(recommended.rules.contains_key("block-no-empty"));
    assert!(recommended.rules.values().all(|r| r.severity.is_none()));
  }
}
