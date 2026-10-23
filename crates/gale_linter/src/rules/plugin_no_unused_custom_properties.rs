use std::collections::HashSet;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::custom_properties::var_references;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "plugin/no-unused-custom-properties";

declare_rule! {
  /// Report custom properties that are defined but never referenced via `var()`.
  ///
  /// It reads the whole file (`check`) to collect
  /// definitions and usages before comparing.
  ///
  /// Options (secondary): an object with an `ignoreProperties` key containing
  /// an array of property patterns to ignore (exact strings or `/regex/`).
  ///
  /// Example config:
  /// ```json
  /// ["plugin/no-unused-custom-properties", [true, {
  ///   "ignoreProperties": ["/^--external-/"]
  /// }]]
  /// ```
  pub struct PluginNoUnusedCustomProperties => PluginNoUnusedCustomPropertiesCheck {
    name: NAME,
    description: "Disallow custom properties that are defined but never used",
    default_severity: Warning,
  }
}

/// `plugin/no-unused-custom-properties` as configured.
pub struct PluginNoUnusedCustomPropertiesCheck {
  /// `ignoreProperties`: custom properties that may go unused.
  ignore_properties: Matchers,
}

impl PluginNoUnusedCustomPropertiesCheck {
  /// Gale's own rule, standing in for projects' custom plugins, so it is
  /// lenient: it reads only `ignoreProperties`, a name or `/regex/` entry or a list
  /// of them, and ignores its primary option and any other key.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let ignore_properties = options.lenient_strings("ignoreProperties");
    options.finish()?;
    Ok(Self {
      ignore_properties: Matchers::new(&ignore_properties),
    })
  }
}

impl Check for PluginNoUnusedCustomPropertiesCheck {
  /// Flags custom properties that no `var()` in the file references, reporting
  /// each name once at its first definition.
  fn check(&self, tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
    // Collect all var() references
    let references: HashSet<&str> = tree
      .walk_decls()
      .flat_map(|decl| var_references(decl.value()))
      .map(|reference| reference.name)
      .collect();

    // Deduplicate definitions by name — only report each unused name once
    // (at the first definition site).
    let mut seen: HashSet<&str> = HashSet::new();
    let mut diagnostics = Vec::new();

    for decl in tree.walk_decls().filter(|decl| decl.is_custom_property()) {
      let name = decl.prop();
      // Skip if already seen (report only first definition)
      if !seen.insert(name) {
        continue;
      }

      // Skip if referenced
      if references.contains(name) {
        continue;
      }

      // Skip if ignored
      if self.ignore_properties.matches(name) {
        continue;
      }

      diagnostics.push(
        Diagnostic::new(
          NAME,
          format!("Unexpected unused custom property \"{name}\""),
        )
        .span(decl.span()),
      );
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  use crate::testing::{CheckSource, configured, ctx_with_source};

  /// A `:root` rule holding `decls`.
  fn make_nodes(decls: Vec<(&str, &str)>) -> String {
    let body: Vec<String> = decls.iter().map(|(p, v)| format!("{p}: {v};")).collect();
    format!(":root {{ {} }}", body.join(" "))
  }

  #[test]
  fn no_diagnostic_when_used() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let nodes = make_nodes(vec![("--my-color", "red"), ("color", "var(--my-color)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn reports_unused_custom_property() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let nodes = make_nodes(vec![("--unused", "red"), ("color", "blue")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unused"));
  }

  #[test]
  fn ignored_property_not_reported() {
    let opts = json!([true, {
        "ignoreProperties": ["/^--external-/"]
    }]);
    let rule = configured(&PluginNoUnusedCustomProperties, opts.clone());
    let nodes = make_nodes(vec![("--external-api", "red")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn ignored_exact_property() {
    let opts = json!([true, {
        "ignoreProperties": ["--my-var"]
    }]);
    let rule = configured(&PluginNoUnusedCustomProperties, opts.clone());
    let nodes = make_nodes(vec![("--my-var", "red")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn multiple_unused() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let nodes = make_nodes(vec![("--a", "1"), ("--b", "2"), ("color", "var(--a)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--b"));
  }

  #[test]
  fn used_in_fallback() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let nodes = make_nodes(vec![
      ("--fallback", "blue"),
      ("color", "var(--primary, var(--fallback))"),
    ]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    // --primary is not defined so it would be caught by no-unknown,
    // but --fallback is used so it should not appear here
    assert!(
      !diags.iter().any(|d| d.message.contains("--fallback")),
      "should not report --fallback as unused"
    );
  }

  #[test]
  fn no_definitions_no_diagnostics() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let nodes = make_nodes(vec![("color", "red")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn no_options_works() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let nodes = make_nodes(vec![("--unused", "red")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn duplicate_definition_reported_once() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let nodes = make_nodes(vec![("--dup", "a"), ("--dup", "b")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn options_are_validated_leniently() {
    use crate::testing::invalid_options;
    // Gale's own rule ignores its primary option and keys it does not read.
    let valid = serde_json::json!(["anything", { "ignoreProperties": "--a", "other": 1 }]);
    assert!(invalid_options(NAME, valid).is_empty());
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreProperties": [true] }])
      ),
      [
        "Invalid value \"true\" for option \"ignoreProperties\" of rule \"plugin/no-unused-custom-properties\""
      ]
    );
  }
}
