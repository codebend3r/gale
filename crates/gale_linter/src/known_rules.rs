//! Which rule names gale can tell apart from typos.
//!
//! A config can name four kinds of rule that gale does not run:
//!
//! - a real Stylelint core rule gale has not implemented yet,
//! - a plugin rule (any `namespace/name`), which gale cannot run unless it
//!   has a built-in port,
//! - a rule an older Stylelint had and a later one removed (`linebreaks`,
//!   `function-whitelist`), which configs written for those versions still
//!   carry,
//! - a name that is no rule at all, usually a typo.
//!
//! Stylelint reports the last kind as `Unknown rule <name>.`, an error on every
//! file.  Gale does the same, and skips the others with a warning so that a
//! migration does not fail on rules gale has yet to port, or on a config
//! written for the Stylelint version the project is moving off.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::registry::RuleRegistry;

/// Every core rule in Stylelint 17, in the order Stylelint lists them (which
/// is also the order its "Did you mean" suggestions come in).
pub const STYLELINT_RULES: &[&str] = &[
  "alpha-value-notation",
  "annotation-no-unknown",
  "at-rule-allowed-list",
  "at-rule-descriptor-no-unknown",
  "at-rule-descriptor-value-no-unknown",
  "at-rule-disallowed-list",
  "at-rule-empty-line-before",
  "at-rule-no-deprecated",
  "at-rule-no-unknown",
  "at-rule-no-vendor-prefix",
  "at-rule-prelude-no-invalid",
  "at-rule-property-required-list",
  "block-no-empty",
  "block-no-redundant-nested-style-rules",
  "color-function-alias-notation",
  "color-function-notation",
  "color-hex-alpha",
  "color-hex-length",
  "color-named",
  "color-no-hex",
  "color-no-invalid-hex",
  "comment-empty-line-before",
  "comment-no-empty",
  "comment-pattern",
  "comment-whitespace-inside",
  "comment-word-disallowed-list",
  "container-name-pattern",
  "custom-media-pattern",
  "custom-property-empty-line-before",
  "custom-property-no-missing-var-function",
  "custom-property-pattern",
  "declaration-block-no-duplicate-custom-properties",
  "declaration-block-no-duplicate-properties",
  "declaration-block-no-redundant-longhand-properties",
  "declaration-block-no-shorthand-property-overrides",
  "declaration-block-single-line-max-declarations",
  "declaration-empty-line-before",
  "declaration-no-important",
  "declaration-property-max-values",
  "declaration-property-unit-allowed-list",
  "declaration-property-unit-disallowed-list",
  "declaration-property-value-allowed-list",
  "declaration-property-value-disallowed-list",
  "declaration-property-value-keyword-no-deprecated",
  "declaration-property-value-no-unknown",
  "display-notation",
  "font-family-name-quotes",
  "font-family-no-duplicate-names",
  "font-family-no-missing-generic-family-keyword",
  "font-weight-notation",
  "function-allowed-list",
  "function-calc-no-unspaced-operator",
  "function-disallowed-list",
  "function-linear-gradient-no-nonstandard-direction",
  "function-name-case",
  "function-no-unknown",
  "function-url-no-scheme-relative",
  "function-url-quotes",
  "function-url-scheme-allowed-list",
  "function-url-scheme-disallowed-list",
  "hue-degree-notation",
  "import-notation",
  "keyframe-block-no-duplicate-selectors",
  "keyframe-declaration-no-important",
  "keyframe-selector-notation",
  "keyframes-name-pattern",
  "layer-name-pattern",
  "length-zero-no-unit",
  "lightness-notation",
  "max-nesting-depth",
  "media-feature-name-allowed-list",
  "media-feature-name-disallowed-list",
  "media-feature-name-no-unknown",
  "media-feature-name-no-vendor-prefix",
  "media-feature-name-unit-allowed-list",
  "media-feature-name-value-allowed-list",
  "media-feature-name-value-no-unknown",
  "media-feature-range-notation",
  "media-query-no-invalid",
  "media-type-no-deprecated",
  "named-grid-areas-no-invalid",
  "nesting-selector-no-missing-scoping-root",
  "no-descending-specificity",
  "no-duplicate-at-import-rules",
  "no-duplicate-selectors",
  "no-empty-source",
  "no-invalid-double-slash-comments",
  "no-invalid-position-at-import-rule",
  "no-invalid-position-declaration",
  "no-irregular-whitespace",
  "no-unknown-animations",
  "no-unknown-custom-media",
  "no-unknown-custom-properties",
  "number-max-precision",
  "property-allowed-list",
  "property-disallowed-list",
  "property-layout-mappings",
  "property-no-deprecated",
  "property-no-unknown",
  "property-no-vendor-prefix",
  "relative-selector-nesting-notation",
  "rule-empty-line-before",
  "rule-nesting-at-rule-required-list",
  "rule-selector-property-disallowed-list",
  "selector-anb-no-unmatchable",
  "selector-attribute-name-disallowed-list",
  "selector-attribute-operator-allowed-list",
  "selector-attribute-operator-disallowed-list",
  "selector-attribute-quotes",
  "selector-class-pattern",
  "selector-combinator-allowed-list",
  "selector-combinator-disallowed-list",
  "selector-disallowed-list",
  "selector-id-pattern",
  "selector-max-attribute",
  "selector-max-class",
  "selector-max-combinators",
  "selector-max-compound-selectors",
  "selector-max-id",
  "selector-max-pseudo-class",
  "selector-max-specificity",
  "selector-max-type",
  "selector-max-universal",
  "selector-nested-pattern",
  "selector-no-deprecated",
  "selector-no-invalid",
  "selector-no-qualifying-type",
  "selector-no-unmatchable",
  "selector-no-vendor-prefix",
  "selector-not-notation",
  "selector-pseudo-class-allowed-list",
  "selector-pseudo-class-disallowed-list",
  "selector-pseudo-class-no-unknown",
  "selector-pseudo-element-allowed-list",
  "selector-pseudo-element-colon-notation",
  "selector-pseudo-element-disallowed-list",
  "selector-pseudo-element-no-unknown",
  "selector-type-case",
  "selector-type-no-unknown",
  "shorthand-property-no-redundant-values",
  "string-no-newline",
  "syntax-string-no-invalid",
  "time-min-milliseconds",
  "unit-allowed-list",
  "unit-disallowed-list",
  "unit-layout-mappings",
  "unit-no-unknown",
  "value-keyword-case",
  "value-keyword-layout-mappings",
  "value-no-vendor-prefix",
];

/// Every core rule Stylelint 13 to 16 had that Stylelint 17 does not: the
/// stylistic rules deprecated in 15 and removed in 16, and the
/// `*-blacklist` / `*-whitelist` names and `function-calc-no-invalid`
/// removed in 14.  Gale implements many of the stylistic ones under their
/// `@stylistic/` names (see [`crate::registry::resolve_deprecated_alias`]).
pub const REMOVED_STYLELINT_RULES: &[&str] = &[
  "at-rule-blacklist",
  "at-rule-name-case",
  "at-rule-name-newline-after",
  "at-rule-name-space-after",
  "at-rule-property-requirelist",
  "at-rule-semicolon-newline-after",
  "at-rule-semicolon-space-before",
  "at-rule-whitelist",
  "block-closing-brace-empty-line-before",
  "block-closing-brace-newline-after",
  "block-closing-brace-newline-before",
  "block-closing-brace-space-after",
  "block-closing-brace-space-before",
  "block-opening-brace-newline-after",
  "block-opening-brace-newline-before",
  "block-opening-brace-space-after",
  "block-opening-brace-space-before",
  "color-hex-case",
  "comment-word-blacklist",
  "declaration-bang-space-after",
  "declaration-bang-space-before",
  "declaration-block-semicolon-newline-after",
  "declaration-block-semicolon-newline-before",
  "declaration-block-semicolon-space-after",
  "declaration-block-semicolon-space-before",
  "declaration-block-trailing-semicolon",
  "declaration-colon-newline-after",
  "declaration-colon-space-after",
  "declaration-colon-space-before",
  "declaration-property-unit-blacklist",
  "declaration-property-unit-whitelist",
  "declaration-property-value-blacklist",
  "declaration-property-value-whitelist",
  "function-blacklist",
  "function-calc-no-invalid",
  "function-comma-newline-after",
  "function-comma-newline-before",
  "function-comma-space-after",
  "function-comma-space-before",
  "function-max-empty-lines",
  "function-parentheses-newline-inside",
  "function-parentheses-space-inside",
  "function-url-scheme-blacklist",
  "function-url-scheme-whitelist",
  "function-whitelist",
  "function-whitespace-after",
  "indentation",
  "linebreaks",
  "max-empty-lines",
  "max-line-length",
  "media-feature-colon-space-after",
  "media-feature-colon-space-before",
  "media-feature-name-blacklist",
  "media-feature-name-case",
  "media-feature-name-value-whitelist",
  "media-feature-name-whitelist",
  "media-feature-parentheses-space-inside",
  "media-feature-range-operator-space-after",
  "media-feature-range-operator-space-before",
  "media-query-list-comma-newline-after",
  "media-query-list-comma-newline-before",
  "media-query-list-comma-space-after",
  "media-query-list-comma-space-before",
  "no-empty-first-line",
  "no-eol-whitespace",
  "no-extra-semicolons",
  "no-missing-end-of-source-newline",
  "number-leading-zero",
  "number-no-trailing-zeros",
  "property-blacklist",
  "property-case",
  "property-whitelist",
  "selector-attribute-brackets-space-inside",
  "selector-attribute-operator-blacklist",
  "selector-attribute-operator-space-after",
  "selector-attribute-operator-space-before",
  "selector-attribute-operator-whitelist",
  "selector-combinator-blacklist",
  "selector-combinator-space-after",
  "selector-combinator-space-before",
  "selector-combinator-whitelist",
  "selector-descendant-combinator-no-non-space",
  "selector-list-comma-newline-after",
  "selector-list-comma-newline-before",
  "selector-list-comma-space-after",
  "selector-list-comma-space-before",
  "selector-max-empty-lines",
  "selector-pseudo-class-blacklist",
  "selector-pseudo-class-case",
  "selector-pseudo-class-parentheses-space-inside",
  "selector-pseudo-class-whitelist",
  "selector-pseudo-element-blacklist",
  "selector-pseudo-element-case",
  "selector-pseudo-element-whitelist",
  "string-quotes",
  "unicode-bom",
  "unit-blacklist",
  "unit-case",
  "unit-whitelist",
  "value-list-comma-newline-after",
  "value-list-comma-newline-before",
  "value-list-comma-space-after",
  "value-list-comma-space-before",
  "value-list-max-empty-lines",
];

/// How gale treats a rule name from a config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSupport {
  /// Gale implements the rule, directly or through a deprecated alias.
  Implemented,
  /// A Stylelint core rule or a plugin rule that gale does not implement
  /// yet.  Skipped with a warning.
  NotImplemented,
  /// A rule an older Stylelint had that Stylelint 17 has removed, and that
  /// gale does not implement under an alias.  Skipped with a warning.
  Removed,
  /// Neither: Stylelint itself would reject the name as an unknown rule.
  Unknown,
}

/// Classify a configured rule name.
///
/// Any namespaced name (`scss/...`, `@stylistic/...`, a third-party plugin)
/// that gale lacks counts as not implemented rather than unknown: gale has no
/// way to tell which rules a given plugin version provides, and calling a real
/// plugin rule unknown would fail a run Stylelint passes.
pub fn classify(registry: &RuleRegistry, name: &str) -> RuleSupport {
  if registry.get(name).is_some() {
    RuleSupport::Implemented
  } else if name.contains('/') || STYLELINT_RULES.contains(&name) {
    RuleSupport::NotImplemented
  } else if REMOVED_STYLELINT_RULES.contains(&name) {
    RuleSupport::Removed
  } else {
    RuleSupport::Unknown
  }
}

/// The most edit distance at which a core rule is still suggested.
const MAX_SUGGESTION_DISTANCE: usize = 6;
/// How many suggestions an unknown-rule message lists at most.
const MAX_SUGGESTIONS: usize = 3;

/// Messages already built, keyed by rule name: every file reports the same
/// unknown rule, and the suggestion search is not free.
static MESSAGES: LazyLock<Mutex<HashMap<String, String>>> =
  LazyLock::new(|| Mutex::new(HashMap::new()));

/// Stylelint's text for an unknown rule: `Unknown rule <name>.`, followed by
/// `Did you mean a, b, c?` when core rules with similar names exist.
pub fn unknown_rule_message(name: &str) -> String {
  let mut cache = MESSAGES.lock().unwrap_or_else(|e| e.into_inner());
  cache
    .entry(name.to_string())
    .or_insert_with(|| {
      let suggestions = suggestions_for(name);
      if suggestions.is_empty() {
        format!("Unknown rule {name}.")
      } else {
        format!(
          "Unknown rule {name}. Did you mean {}?",
          suggestions.join(", ")
        )
      }
    })
    .clone()
}

/// Core rules close to `name`, as Stylelint picks them: the nearest of the
/// first three distances that has any match, otherwise the closest few from
/// distances four to six.
fn suggestions_for(name: &str) -> Vec<&'static str> {
  let mut by_distance: Vec<Vec<&'static str>> = vec![Vec::new(); MAX_SUGGESTION_DISTANCE];
  for rule in STYLELINT_RULES {
    let distance = levenshtein(rule, name);
    if (1..=MAX_SUGGESTION_DISTANCE).contains(&distance) {
      by_distance[distance - 1].push(rule);
    }
  }

  let mut found = Vec::new();
  for (index, bucket) in by_distance.into_iter().enumerate() {
    if bucket.is_empty() {
      continue;
    }
    if index < 3 {
      return bucket.into_iter().take(MAX_SUGGESTIONS).collect();
    }
    found.extend(bucket);
  }
  found.truncate(MAX_SUGGESTIONS);
  found
}

/// Edit distance between two strings, counted in characters.
fn levenshtein(a: &str, b: &str) -> usize {
  let b: Vec<char> = b.chars().collect();
  let mut previous: Vec<usize> = (0..=b.len()).collect();
  let mut current = vec![0; b.len() + 1];
  for (i, ca) in a.chars().enumerate() {
    current[0] = i + 1;
    for (j, cb) in b.iter().enumerate() {
      let substitution = previous[j] + usize::from(ca != *cb);
      current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
    }
    std::mem::swap(&mut previous, &mut current);
  }
  previous[b.len()]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn classifies_implemented_missing_and_unknown_names() {
    let registry = RuleRegistry::default();
    assert_eq!(
      classify(&registry, "block-no-empty"),
      RuleSupport::Implemented
    );
    // A deprecated alias of an implemented rule.
    assert_eq!(classify(&registry, "indentation"), RuleSupport::Implemented);
    assert_eq!(
      classify(&registry, "no-unknown-custom-properties"),
      RuleSupport::NotImplemented
    );
    assert_eq!(
      classify(&registry, "some-plugin/some-rule"),
      RuleSupport::NotImplemented
    );
    assert_eq!(classify(&registry, "linebreaks"), RuleSupport::Removed);
    assert_eq!(
      classify(&registry, "function-whitelist"),
      RuleSupport::Removed
    );
    assert_eq!(classify(&registry, "block-no-emty"), RuleSupport::Unknown);
    assert_eq!(
      classify(&registry, "not-a-rule-at-all"),
      RuleSupport::Unknown
    );
  }

  #[test]
  fn unknown_rule_messages_match_stylelint() {
    assert_eq!(
      unknown_rule_message("block-no-emty"),
      "Unknown rule block-no-emty. Did you mean block-no-empty?"
    );
    assert_eq!(
      unknown_rule_message("zzzzzzzzzzzzzzzzzzzzzzzzzz"),
      "Unknown rule zzzzzzzzzzzzzzzzzzzzzzzzzz."
    );
    // Several rules at the same distance are listed in Stylelint's order,
    // at most three of them.
    assert_eq!(
      unknown_rule_message("color-hex-lengths"),
      "Unknown rule color-hex-lengths. Did you mean color-hex-length?"
    );
  }

  #[test]
  fn levenshtein_counts_edits() {
    assert_eq!(levenshtein("kitten", "sitting"), 3);
    assert_eq!(levenshtein("", "abc"), 3);
    assert_eq!(levenshtein("same", "same"), 0);
  }

  #[test]
  fn the_stylelint_lists_are_sorted_unique_and_disjoint() {
    assert!(
      STYLELINT_RULES.windows(2).all(|w| w[0] < w[1]),
      "keep STYLELINT_RULES in Stylelint's (alphabetical) order"
    );
    assert!(REMOVED_STYLELINT_RULES.windows(2).all(|w| w[0] < w[1]));
    for name in REMOVED_STYLELINT_RULES {
      assert!(!STYLELINT_RULES.contains(name), "{name} was not removed");
    }
  }
}
