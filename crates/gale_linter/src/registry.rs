use crate::rule::Rule;
use crate::rules;

/// A collection of registered lint rules.
pub struct RuleRegistry {
  rules: Vec<Box<dyn Rule>>,
}

impl RuleRegistry {
  /// Create an empty registry.
  pub fn new() -> Self {
    Self { rules: Vec::new() }
  }

  /// Register a rule in the registry.
  pub fn register(&mut self, rule: Box<dyn Rule>) {
    self.rules.push(rule);
  }

  /// Look up a rule by name, including deprecated Stylelint rule name aliases.
  pub fn get(&self, name: &str) -> Option<&dyn Rule> {
    self
      .rules
      .iter()
      .find(|r| r.name() == name)
      .map(|r| &**r)
      .or_else(|| {
        // Try deprecated rule name → @stylistic/ alias.
        // Stylelint moved these rules to the @stylistic plugin but
        // still accepts the old names for backward compatibility.
        let aliased = resolve_deprecated_alias(name)?;
        self
          .rules
          .iter()
          .find(|r| r.name() == aliased)
          .map(|r| &**r)
      })
  }

  /// Return a slice of all registered rules.
  pub fn all(&self) -> &[Box<dyn Rule>] {
    &self.rules
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

/// Map deprecated Stylelint rule names to their `@stylistic/` equivalents.
///
/// Stylelint 15+ moved stylistic rules to the `@stylistic/stylelint-plugin`
/// package, but old configs still use the unprefixed names.
pub fn resolve_deprecated_alias(name: &str) -> Option<&'static str> {
  match name {
    "at-rule-name-case" => Some("@stylistic/at-rule-name-case"),
    "at-rule-name-space-after" => Some("@stylistic/at-rule-name-space-after"),
    "at-rule-semicolon-newline-after" => Some("@stylistic/at-rule-semicolon-newline-after"),
    "at-rule-semicolon-space-before" => Some("@stylistic/at-rule-semicolon-space-before"),
    "block-closing-brace-empty-line-before" => {
      Some("@stylistic/block-closing-brace-empty-line-before")
    }
    "block-closing-brace-newline-after" => Some("@stylistic/block-closing-brace-newline-after"),
    "block-closing-brace-newline-before" => Some("@stylistic/block-closing-brace-newline-before"),
    "block-closing-brace-space-before" => Some("@stylistic/block-closing-brace-space-before"),
    "block-opening-brace-newline-after" => Some("@stylistic/block-opening-brace-newline-after"),
    "block-opening-brace-space-before" => Some("@stylistic/block-opening-brace-space-before"),
    "color-hex-case" => Some("@stylistic/color-hex-case"),
    "declaration-bang-space-after" => Some("@stylistic/declaration-bang-space-after"),
    "declaration-bang-space-before" => Some("@stylistic/declaration-bang-space-before"),
    "declaration-block-semicolon-newline-after" => {
      Some("@stylistic/declaration-block-semicolon-newline-after")
    }
    "declaration-block-semicolon-newline-before" => {
      Some("@stylistic/declaration-block-semicolon-newline-before")
    }
    "declaration-block-semicolon-space-before" => {
      Some("@stylistic/declaration-block-semicolon-space-before")
    }
    "declaration-block-trailing-semicolon" => {
      Some("@stylistic/declaration-block-trailing-semicolon")
    }
    "declaration-colon-newline-after" => Some("@stylistic/declaration-colon-newline-after"),
    "declaration-colon-space-after" => Some("@stylistic/declaration-colon-space-after"),
    "declaration-colon-space-before" => Some("@stylistic/declaration-colon-space-before"),
    "function-comma-newline-after" => Some("@stylistic/function-comma-newline-after"),
    "function-comma-space-after" => Some("@stylistic/function-comma-space-after"),
    "function-comma-space-before" => Some("@stylistic/function-comma-space-before"),
    "function-max-empty-lines" => Some("@stylistic/function-max-empty-lines"),
    "function-parentheses-space-inside" => Some("@stylistic/function-parentheses-space-inside"),
    "function-whitespace-after" => Some("@stylistic/function-whitespace-after"),
    "indentation" => Some("@stylistic/indentation"),
    "max-empty-lines" => Some("@stylistic/max-empty-lines"),
    "max-line-length" => Some("max-line-length"), // Gale has this as its own rule
    "media-feature-colon-space-after" => Some("@stylistic/media-feature-colon-space-after"),
    "media-feature-colon-space-before" => Some("@stylistic/media-feature-colon-space-before"),
    "media-feature-name-case" => Some("@stylistic/media-feature-name-case"),
    "media-feature-parentheses-space-inside" => {
      Some("@stylistic/media-feature-parentheses-space-inside")
    }
    "media-feature-range-operator-space-after" => {
      Some("@stylistic/media-feature-range-operator-space-after")
    }
    "media-feature-range-operator-space-before" => {
      Some("@stylistic/media-feature-range-operator-space-before")
    }
    "media-query-list-comma-newline-after" => {
      Some("@stylistic/media-query-list-comma-newline-after")
    }
    "media-query-list-comma-space-after" => Some("@stylistic/media-query-list-comma-space-after"),
    "media-query-list-comma-space-before" => Some("@stylistic/media-query-list-comma-space-before"),
    "no-empty-first-line" => Some("@stylistic/no-empty-first-line"),
    "no-eol-whitespace" => Some("@stylistic/no-eol-whitespace"),
    "no-extra-semicolons" => Some("@stylistic/no-extra-semicolons"),
    "no-missing-end-of-source-newline" => Some("@stylistic/no-missing-end-of-source-newline"),
    "number-leading-zero" => Some("@stylistic/number-leading-zero"),
    "number-no-trailing-zeros" => Some("@stylistic/number-no-trailing-zeros"),
    "property-case" => Some("@stylistic/property-case"),
    "selector-attribute-brackets-space-inside" => {
      Some("@stylistic/selector-attribute-brackets-space-inside")
    }
    "selector-attribute-operator-space-after" => {
      Some("@stylistic/selector-attribute-operator-space-after")
    }
    "selector-attribute-operator-space-before" => {
      Some("@stylistic/selector-attribute-operator-space-before")
    }
    "selector-combinator-space-after" => Some("@stylistic/selector-combinator-space-after"),
    "selector-combinator-space-before" => Some("@stylistic/selector-combinator-space-before"),
    "selector-descendant-combinator-no-non-space" => {
      Some("@stylistic/selector-descendant-combinator-no-non-space")
    }
    "selector-list-comma-newline-after" => Some("@stylistic/selector-list-comma-newline-after"),
    "selector-list-comma-newline-before" => Some("@stylistic/selector-list-comma-newline-before"),
    "selector-list-comma-space-after" => Some("@stylistic/selector-list-comma-space-after"),
    "selector-list-comma-space-before" => Some("@stylistic/selector-list-comma-space-before"),
    "selector-max-empty-lines" => Some("@stylistic/selector-max-empty-lines"),
    "selector-pseudo-class-case" => Some("@stylistic/selector-pseudo-class-case"),
    "selector-pseudo-class-parentheses-space-inside" => {
      Some("@stylistic/selector-pseudo-class-parentheses-space-inside")
    }
    "selector-pseudo-element-case" => Some("@stylistic/selector-pseudo-element-case"),
    "string-quotes" => Some("@stylistic/string-quotes"),
    "unicode-bom" => Some("@stylistic/unicode-bom"),
    "unit-case" => Some("@stylistic/unit-case"),
    "value-list-comma-newline-after" => Some("@stylistic/value-list-comma-newline-after"),
    "value-list-comma-newline-before" => Some("@stylistic/value-list-comma-newline-before"),
    "value-list-comma-space-after" => Some("@stylistic/value-list-comma-space-after"),
    "value-list-comma-space-before" => Some("@stylistic/value-list-comma-space-before"),
    "value-list-max-empty-lines" => Some("@stylistic/value-list-max-empty-lines"),
    // Spectrum Tools aliases (functionally identical to csstools equivalents)
    "spectrum-tools/no-unknown-custom-properties" => {
      Some("csstools/value-no-unknown-custom-properties")
    }
    // SCSS deprecated aliases
    "scss/at-import-partial-extension-blacklist" => {
      Some("scss/at-import-partial-extension-disallowed-list")
    }
    "scss/at-import-no-partial-leading-underscore" => {
      Some("scss/load-no-partial-leading-underscore")
    }
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
}
