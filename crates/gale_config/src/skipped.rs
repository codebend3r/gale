//! What a config asks for that gale leaves out, and the warnings saying so.
//!
//! The CLI prints these once per run and the language server logs them, so
//! both word them the same way; each frontend adds its own prefix (the
//! CLI's `warning: `, the language server's message type).

use std::collections::BTreeSet;
use std::fmt;

use gale_linter::RuleRegistry;
use gale_linter::known_rules::{self, RuleSupport};

use crate::file_settings::RuleSetResolver;
use crate::model::{GaleConfig, Severity};
use crate::plugins::is_known_plugin;

/// Something a config asks for that gale leaves out.  It displays as the
/// message alone, without a `warning: ` prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigWarning {
  /// Real Stylelint or plugin rules gale has not implemented yet, sorted.
  NotImplemented(Vec<String>),
  /// Rules an older Stylelint had that Stylelint 17 has removed, sorted.
  Removed(Vec<String>),
  /// A plugin gale does not know, whose rules therefore do nothing.
  UnsupportedPlugin(String),
}

impl fmt::Display for ConfigWarning {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::NotImplemented(rules) => write!(
        f,
        "gale does not support these rules yet, so they were skipped: {}",
        rules.join(", ")
      ),
      Self::Removed(rules) => write!(
        f,
        "these rules were removed from Stylelint, so they were skipped: {}",
        rules.join(", ")
      ),
      Self::UnsupportedPlugin(plugin) => write!(
        f,
        "Plugin \"{plugin}\" is not supported by Gale and will be ignored. \
         Rules from this plugin will have no effect."
      ),
    }
  }
}

/// Configured rules that gale skips rather than runs, by reason.
#[derive(Debug, Default)]
struct SkippedRules {
  /// Real Stylelint or plugin rules gale has not implemented yet.
  not_implemented: BTreeSet<String>,
  /// Rules an older Stylelint had that Stylelint 17 has removed.
  removed: BTreeSet<String>,
}

impl SkippedRules {
  /// The rules `config` enables (at the top level or in an override) that
  /// gale skips.  Names that are no rule at all are not among them: the
  /// rule set reports those as `Unknown rule`.
  fn in_config(config: &GaleConfig) -> Self {
    let registry = RuleRegistry::global();
    let mut skipped = Self::default();
    let enabled = config
      .rules
      .iter()
      .chain(config.overrides.iter().flat_map(|o| o.rules.iter()))
      .filter(|(_, rule)| rule.severity != Some(Severity::Off));
    for (name, _) in enabled {
      match known_rules::classify(registry, name) {
        RuleSupport::NotImplemented => skipped.not_implemented.insert(name.clone()),
        RuleSupport::Removed => skipped.removed.insert(name.clone()),
        RuleSupport::Implemented | RuleSupport::Unknown => false,
      };
    }
    skipped
  }

  /// Add the rules another config skips.
  fn extend(&mut self, other: Self) {
    self.not_implemented.extend(other.not_implemented);
    self.removed.extend(other.removed);
  }

  /// The rules skipped here that `reported` does not hold.
  fn not_in(&self, reported: &Self) -> Self {
    Self {
      not_implemented: &self.not_implemented - &reported.not_implemented,
      removed: &self.removed - &reported.removed,
    }
  }

  /// One warning per reason that has any rules, each listing them.
  fn warnings(&self) -> Vec<ConfigWarning> {
    let list = |names: &BTreeSet<String>| names.iter().cloned().collect::<Vec<_>>();
    let not_implemented = (!self.not_implemented.is_empty())
      .then(|| ConfigWarning::NotImplemented(list(&self.not_implemented)));
    let removed = (!self.removed.is_empty()).then(|| ConfigWarning::Removed(list(&self.removed)));
    not_implemented.into_iter().chain(removed).collect()
  }
}

/// The skipped rules already warned about, so each is warned about once
/// however many configs ask for it and whenever they are loaded.
///
/// The CLI asks once, after every file's config is loaded; the language
/// server asks again whenever a document loads a config it had not seen.
/// Either way every config the resolver loaded is covered, the root config
/// and nested ones alike.
#[derive(Debug, Default)]
pub struct SkippedRuleWarnings {
  /// How many of the resolver's configs (which it only ever adds to, in
  /// order) have been looked at.
  configs_seen: usize,
  reported: SkippedRules,
}

impl SkippedRuleWarnings {
  /// One warning per reason for the rules skipped by the configs
  /// `resolver` loaded since the last call, leaving out rules already
  /// warned about.  Names that are no rule at all are reported in each
  /// file as `Unknown rule`, as Stylelint does, not here.
  pub fn new_warnings(&mut self, resolver: &RuleSetResolver) -> Vec<ConfigWarning> {
    let mut skipped = SkippedRules::default();
    resolver
      .configs()
      .skip(self.configs_seen)
      .for_each(|config| {
        skipped.extend(SkippedRules::in_config(config));
        self.configs_seen += 1;
      });
    let new = skipped.not_in(&self.reported);
    let warnings = new.warnings();
    self.reported.extend(new);
    warnings
  }
}

/// A warning for each plugin `config` loads that gale does not know, whose
/// rules therefore do nothing.
pub fn unsupported_plugin_warnings(config: &GaleConfig) -> Vec<ConfigWarning> {
  config
    .plugins
    .iter()
    .filter(|plugin| !is_known_plugin(plugin))
    .map(|plugin| ConfigWarning::UnsupportedPlugin(plugin.clone()))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{ConfigSource, ResolverOptions, RunSettings, WithoutConfig};

  #[test]
  fn skipped_rules_lists_rules_gale_lacks_and_rules_stylelint_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".stylelintrc.json");
    std::fs::write(
      &path,
      r#"{
        "plugins": ["stylelint-acme", "stylelint-scss"],
        "rules": {
          "block-no-empty": true,
          "no-unknown-custom-properties": true,
          "acme/no-foo": true,
          "not-a-rule": true,
          "linebreaks": "unix",
          "no-unknown-custom-media": null
        },
        "overrides": [
          { "files": ["*.scss"], "rules": { "no-unknown-custom-properties": true, "selector-no-deprecated": true } }
        ]
      }"#,
    )
    .unwrap();
    let config = crate::load_config(&path).unwrap();
    let skipped = SkippedRules::in_config(&config);
    let messages: Vec<String> = skipped.warnings().iter().map(ToString::to_string).collect();
    assert_eq!(
      messages,
      vec![
        "gale does not support these rules yet, so they were skipped: \
         acme/no-foo, no-unknown-custom-properties, selector-no-deprecated"
          .to_string(),
        "these rules were removed from Stylelint, so they were skipped: linebreaks".to_string(),
      ]
    );
    assert!(SkippedRules::default().warnings().is_empty());
    let plugins: Vec<String> = unsupported_plugin_warnings(&config)
      .iter()
      .map(ToString::to_string)
      .collect();
    assert_eq!(
      plugins,
      [
        "Plugin \"stylelint-acme\" is not supported by Gale and will be ignored. \
        Rules from this plugin will have no effect."
      ]
    );
  }

  #[test]
  fn nested_configs_loaded_later_warn_only_about_rules_not_yet_warned_about() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join(".stylelintrc.json");
    std::fs::write(&root, r#"{ "rules": { "acme/no-foo": true } }"#).unwrap();
    std::fs::create_dir(tmp.path().join("nested")).unwrap();
    std::fs::write(
      tmp.path().join("nested/.stylelintrc.json"),
      r#"{ "rules": { "acme/no-foo": true, "acme/no-bar": true, "linebreaks": "unix" } }"#,
    )
    .unwrap();
    let config = crate::load_config(&root).unwrap();
    let mut resolver = RuleSetResolver::new(ResolverOptions {
      source: ConfigSource::Closest {
        root: Some((root, config)),
      },
      without_config: WithoutConfig::NoRules,
      forced_syntax: None,
      run: RunSettings::default(),
      cwd: tmp.path().to_path_buf(),
    });
    let mut warnings = SkippedRuleWarnings::default();
    assert_eq!(
      warnings.new_warnings(&resolver),
      [ConfigWarning::NotImplemented(vec![
        "acme/no-foo".to_string()
      ])]
    );
    assert!(warnings.new_warnings(&resolver).is_empty());

    resolver.resolve(&tmp.path().join("nested/a.css").display().to_string());
    assert_eq!(
      warnings.new_warnings(&resolver),
      [
        ConfigWarning::NotImplemented(vec!["acme/no-bar".to_string()]),
        ConfigWarning::Removed(vec!["linebreaks".to_string()]),
      ]
    );
  }
}
