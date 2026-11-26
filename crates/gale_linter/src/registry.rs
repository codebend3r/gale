use std::collections::HashMap;

use crate::rule::Rule;
use crate::rules;

/// A collection of registered lint rules.
pub struct RuleRegistry {
  rules: Vec<Box<dyn Rule>>,
  /// Position in `rules` of the first rule registered under each name, so
  /// lookups by name are constant time rather than a scan of every rule.
  by_name: HashMap<&'static str, usize>,
}

impl RuleRegistry {
  /// Create an empty registry.
  pub fn new() -> Self {
    Self {
      rules: Vec::new(),
      by_name: HashMap::new(),
    }
  }

  /// Register a rule in the registry.
  pub fn register(&mut self, rule: Box<dyn Rule>) {
    // The first rule registered under a name wins, as it did when lookups
    // scanned the list in order.
    self.by_name.entry(rule.name()).or_insert(self.rules.len());
    self.rules.push(rule);
  }

  /// Look up a rule by name, including deprecated Stylelint rule name aliases.
  pub fn get(&self, name: &str) -> Option<&dyn Rule> {
    self
      .by_name
      .get(name)
      .or_else(|| {
        // Try deprecated rule name → @stylistic/ alias.
        // Stylelint moved these rules to the @stylistic plugin but
        // still accepts the old names for backward compatibility.
        self.by_name.get(resolve_deprecated_alias(name)?)
      })
      .map(|&i| &*self.rules[i])
  }

  /// Return a slice of all registered rules.
  pub fn all(&self) -> &[Box<dyn Rule>] {
    &self.rules
  }

  /// The built-in rules, registered once per process.  Rule sets borrow
  /// their rules from here, so a rule set can be built and shared without
  /// tying it to a registry's lifetime.
  pub fn global() -> &'static RuleRegistry {
    static GLOBAL: std::sync::LazyLock<RuleRegistry> =
      std::sync::LazyLock::new(RuleRegistry::default);
    &GLOBAL
  }
}

impl Default for RuleRegistry {
  /// Create a registry with all built-in rules registered.
  fn default() -> Self {
    let mut registry = Self::new();
    rules::register_all(&mut registry);
    registry
  }
}

/// The canonical name for `name`: its `@stylistic/` replacement when it is a
/// deprecated alias, otherwise `name` itself.
pub(crate) fn canonical_name(name: &str) -> &str {
  resolve_deprecated_alias(name).unwrap_or(name)
}

/// The core rules Stylelint 15 deprecated and moved to
/// `@stylistic/stylelint-plugin`: each old name resolves to the same name
/// under `@stylistic/`.
const STYLISTIC_MOVES: &[(&str, &str)] = &[
  ("at-rule-name-case", "@stylistic/at-rule-name-case"),
  (
    "at-rule-name-space-after",
    "@stylistic/at-rule-name-space-after",
  ),
  (
    "at-rule-semicolon-newline-after",
    "@stylistic/at-rule-semicolon-newline-after",
  ),
  (
    "at-rule-semicolon-space-before",
    "@stylistic/at-rule-semicolon-space-before",
  ),
  (
    "block-closing-brace-empty-line-before",
    "@stylistic/block-closing-brace-empty-line-before",
  ),
  (
    "block-closing-brace-newline-after",
    "@stylistic/block-closing-brace-newline-after",
  ),
  (
    "block-closing-brace-newline-before",
    "@stylistic/block-closing-brace-newline-before",
  ),
  (
    "block-closing-brace-space-before",
    "@stylistic/block-closing-brace-space-before",
  ),
  (
    "block-opening-brace-newline-after",
    "@stylistic/block-opening-brace-newline-after",
  ),
  (
    "block-opening-brace-space-before",
    "@stylistic/block-opening-brace-space-before",
  ),
  ("color-hex-case", "@stylistic/color-hex-case"),
  (
    "declaration-bang-space-after",
    "@stylistic/declaration-bang-space-after",
  ),
  (
    "declaration-bang-space-before",
    "@stylistic/declaration-bang-space-before",
  ),
  (
    "declaration-block-semicolon-newline-after",
    "@stylistic/declaration-block-semicolon-newline-after",
  ),
  (
    "declaration-block-semicolon-newline-before",
    "@stylistic/declaration-block-semicolon-newline-before",
  ),
  (
    "declaration-block-semicolon-space-before",
    "@stylistic/declaration-block-semicolon-space-before",
  ),
  (
    "declaration-block-trailing-semicolon",
    "@stylistic/declaration-block-trailing-semicolon",
  ),
  (
    "declaration-colon-newline-after",
    "@stylistic/declaration-colon-newline-after",
  ),
  (
    "declaration-colon-space-after",
    "@stylistic/declaration-colon-space-after",
  ),
  (
    "declaration-colon-space-before",
    "@stylistic/declaration-colon-space-before",
  ),
  (
    "function-comma-newline-after",
    "@stylistic/function-comma-newline-after",
  ),
  (
    "function-comma-space-after",
    "@stylistic/function-comma-space-after",
  ),
  (
    "function-comma-space-before",
    "@stylistic/function-comma-space-before",
  ),
  (
    "function-max-empty-lines",
    "@stylistic/function-max-empty-lines",
  ),
  (
    "function-parentheses-space-inside",
    "@stylistic/function-parentheses-space-inside",
  ),
  (
    "function-whitespace-after",
    "@stylistic/function-whitespace-after",
  ),
  ("indentation", "@stylistic/indentation"),
  ("max-empty-lines", "@stylistic/max-empty-lines"),
  (
    "media-feature-colon-space-after",
    "@stylistic/media-feature-colon-space-after",
  ),
  (
    "media-feature-colon-space-before",
    "@stylistic/media-feature-colon-space-before",
  ),
  (
    "media-feature-name-case",
    "@stylistic/media-feature-name-case",
  ),
  (
    "media-feature-parentheses-space-inside",
    "@stylistic/media-feature-parentheses-space-inside",
  ),
  (
    "media-feature-range-operator-space-after",
    "@stylistic/media-feature-range-operator-space-after",
  ),
  (
    "media-feature-range-operator-space-before",
    "@stylistic/media-feature-range-operator-space-before",
  ),
  (
    "media-query-list-comma-newline-after",
    "@stylistic/media-query-list-comma-newline-after",
  ),
  (
    "media-query-list-comma-space-after",
    "@stylistic/media-query-list-comma-space-after",
  ),
  (
    "media-query-list-comma-space-before",
    "@stylistic/media-query-list-comma-space-before",
  ),
  ("no-empty-first-line", "@stylistic/no-empty-first-line"),
  ("no-eol-whitespace", "@stylistic/no-eol-whitespace"),
  ("no-extra-semicolons", "@stylistic/no-extra-semicolons"),
  (
    "no-missing-end-of-source-newline",
    "@stylistic/no-missing-end-of-source-newline",
  ),
  ("number-leading-zero", "@stylistic/number-leading-zero"),
  (
    "number-no-trailing-zeros",
    "@stylistic/number-no-trailing-zeros",
  ),
  ("property-case", "@stylistic/property-case"),
  (
    "selector-attribute-brackets-space-inside",
    "@stylistic/selector-attribute-brackets-space-inside",
  ),
  (
    "selector-attribute-operator-space-after",
    "@stylistic/selector-attribute-operator-space-after",
  ),
  (
    "selector-attribute-operator-space-before",
    "@stylistic/selector-attribute-operator-space-before",
  ),
  (
    "selector-combinator-space-after",
    "@stylistic/selector-combinator-space-after",
  ),
  (
    "selector-combinator-space-before",
    "@stylistic/selector-combinator-space-before",
  ),
  (
    "selector-descendant-combinator-no-non-space",
    "@stylistic/selector-descendant-combinator-no-non-space",
  ),
  (
    "selector-list-comma-newline-after",
    "@stylistic/selector-list-comma-newline-after",
  ),
  (
    "selector-list-comma-newline-before",
    "@stylistic/selector-list-comma-newline-before",
  ),
  (
    "selector-list-comma-space-after",
    "@stylistic/selector-list-comma-space-after",
  ),
  (
    "selector-list-comma-space-before",
    "@stylistic/selector-list-comma-space-before",
  ),
  (
    "selector-max-empty-lines",
    "@stylistic/selector-max-empty-lines",
  ),
  (
    "selector-pseudo-class-case",
    "@stylistic/selector-pseudo-class-case",
  ),
  (
    "selector-pseudo-class-parentheses-space-inside",
    "@stylistic/selector-pseudo-class-parentheses-space-inside",
  ),
  (
    "selector-pseudo-element-case",
    "@stylistic/selector-pseudo-element-case",
  ),
  ("string-quotes", "@stylistic/string-quotes"),
  ("unicode-bom", "@stylistic/unicode-bom"),
  ("unit-case", "@stylistic/unit-case"),
  (
    "value-list-comma-newline-after",
    "@stylistic/value-list-comma-newline-after",
  ),
  (
    "value-list-comma-newline-before",
    "@stylistic/value-list-comma-newline-before",
  ),
  (
    "value-list-comma-space-after",
    "@stylistic/value-list-comma-space-after",
  ),
  (
    "value-list-comma-space-before",
    "@stylistic/value-list-comma-space-before",
  ),
  (
    "value-list-max-empty-lines",
    "@stylistic/value-list-max-empty-lines",
  ),
];

/// Other deprecated names, as `(old, new)`.
const RENAMES: &[(&str, &str)] = &[
  // Spectrum Tools alias, functionally identical to the csstools rule.
  (
    "spectrum-tools/no-unknown-custom-properties",
    "csstools/value-no-unknown-custom-properties",
  ),
  // SCSS deprecated aliases.
  (
    "scss/at-import-partial-extension-blacklist",
    "scss/at-import-partial-extension-disallowed-list",
  ),
  (
    "scss/at-import-no-partial-leading-underscore",
    "scss/load-no-partial-leading-underscore",
  ),
];

/// Map deprecated Stylelint rule names to their `@stylistic/` equivalents
/// (and the few other renames in [`RENAMES`]).
///
/// Stylelint 15+ moved stylistic rules to the `@stylistic/stylelint-plugin`
/// package, but old configs still use the unprefixed names.
pub(crate) fn resolve_deprecated_alias(name: &str) -> Option<&'static str> {
  STYLISTIC_MOVES
    .iter()
    .chain(RENAMES)
    .find(|(old, _)| *old == name)
    .map(|&(_, new)| new)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::lint_env::LintEnv;
  use crate::options::{InvalidOption, Setting};
  use crate::rule::{Check, RuleMeta};

  #[test]
  fn default_registry_has_rules() {
    let registry = RuleRegistry::default();
    assert!(!registry.all().is_empty());
  }

  #[test]
  fn lookup_by_name() {
    let registry = RuleRegistry::default();
    assert!(registry.get("block-no-empty").is_some());
    assert!(registry.get("nonexistent-rule").is_none());
  }

  #[test]
  fn lookup_resolves_deprecated_aliases() {
    let registry = RuleRegistry::default();
    assert_eq!(
      registry.get("indentation").map(|r| r.name()),
      Some("@stylistic/indentation")
    );
    assert_eq!(
      registry.get("@stylistic/indentation").map(|r| r.name()),
      Some("@stylistic/indentation")
    );
  }

  #[test]
  fn each_stylistic_move_keeps_its_name_under_the_plugin() {
    for (old, new) in STYLISTIC_MOVES {
      assert_eq!(new.strip_prefix("@stylistic/"), Some(*old));
    }
    assert_eq!(
      resolve_deprecated_alias("scss/at-import-partial-extension-blacklist"),
      Some("scss/at-import-partial-extension-disallowed-list")
    );
    assert_eq!(resolve_deprecated_alias("max-line-length"), None);
    assert_eq!(canonical_name("max-line-length"), "max-line-length");
  }

  #[test]
  fn every_registered_rule_is_found_under_its_own_name() {
    let registry = RuleRegistry::default();
    for rule in registry.all() {
      let found = registry.get(rule.name()).expect("registered rule");
      assert_eq!(found.name(), rule.name());
    }
  }

  /// A rule that only differs from another by its description.
  struct Named(&'static str);

  /// What [`Named`] checks: nothing.
  struct ChecksNothing;

  impl Check for ChecksNothing {}

  impl Rule for Named {
    fn meta(&self) -> RuleMeta {
      RuleMeta {
        name: "duplicate-name",
        description: self.0,
        default_severity: gale_diagnostics::Severity::Error,
        primary_option_array: false,
        primary_option_object: false,
      }
    }

    fn configure(
      &self,
      _setting: &Setting,
      _env: &LintEnv,
    ) -> Result<Box<dyn Check + '_>, Vec<InvalidOption>> {
      Ok(Box::new(ChecksNothing))
    }
  }

  #[test]
  fn the_first_rule_registered_under_a_name_wins() {
    let mut registry = RuleRegistry::new();
    registry.register(Box::new(Named("first")));
    registry.register(Box::new(Named("second")));
    assert_eq!(
      registry.get("duplicate-name").map(|r| r.description()),
      Some("first")
    );
    assert_eq!(registry.all().len(), 2);
  }
}
