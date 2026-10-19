//! What a config asks for that gale leaves out, and the warnings saying so.
//!
//! The CLI prints these once per run and the language server logs them, so
//! both word them the same way.

use std::collections::BTreeSet;

use gale_linter::RuleRegistry;
use gale_linter::known_rules::{self, RuleSupport};

use crate::model::{GaleConfig, Severity};
use crate::plugins::is_known_plugin;

/// Configured rules that gale skips rather than runs, by reason.
#[derive(Debug, Default)]
pub struct SkippedRules {
  /// Real Stylelint or plugin rules gale has not implemented yet.
  not_implemented: BTreeSet<String>,
  /// Rules an older Stylelint had that Stylelint 17 has removed.
  removed: BTreeSet<String>,
}

impl SkippedRules {
  /// The rules `config` enables (at the top level or in an override) that
  /// gale skips.  Names that are no rule at all are not among them: the
  /// rule set reports those as `Unknown rule`.
  pub fn in_config(config: &GaleConfig) -> Self {
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
  pub fn extend(&mut self, other: Self) {
    self.not_implemented.extend(other.not_implemented);
    self.removed.extend(other.removed);
  }

  /// One warning line per reason that has any rules, each listing them.
  pub fn warnings(&self) -> Vec<String> {
    let list = |names: &BTreeSet<String>| names.iter().cloned().collect::<Vec<_>>().join(", ");
    let mut warnings = Vec::new();
    if !self.not_implemented.is_empty() {
      warnings.push(format!(
        "warning: gale does not support these rules yet, so they were skipped: {}",
        list(&self.not_implemented)
      ));
    }
    if !self.removed.is_empty() {
      warnings.push(format!(
        "warning: these rules were removed from Stylelint, so they were skipped: {}",
        list(&self.removed)
      ));
    }
    warnings
  }
}

/// A warning for each plugin `config` loads that gale does not know, whose
/// rules therefore do nothing.
pub fn unsupported_plugin_warnings(config: &GaleConfig) -> Vec<String> {
  config
    .plugins
    .iter()
    .filter(|plugin| !is_known_plugin(plugin))
    .map(|plugin| {
      format!(
        "warning: Plugin \"{plugin}\" is not supported by Gale and will be ignored. \
         Rules from this plugin will have no effect."
      )
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

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
    assert_eq!(
      skipped.warnings(),
      vec![
        "warning: gale does not support these rules yet, so they were skipped: \
         acme/no-foo, no-unknown-custom-properties, selector-no-deprecated"
          .to_string(),
        "warning: these rules were removed from Stylelint, so they were skipped: linebreaks"
          .to_string(),
      ]
    );
    assert!(SkippedRules::default().warnings().is_empty());
    assert_eq!(
      unsupported_plugin_warnings(&config),
      [
        "warning: Plugin \"stylelint-acme\" is not supported by Gale and will be ignored. \
        Rules from this plugin will have no effect."
      ]
    );
  }
}
