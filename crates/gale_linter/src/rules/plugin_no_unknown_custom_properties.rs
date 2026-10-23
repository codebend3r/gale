use std::collections::HashSet;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::custom_properties::var_references;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "plugin/no-unknown-custom-properties";

declare_rule! {
  /// Report usage of CSS custom properties (`var(--foo)`) that are not defined
  /// in the current file or a configurable allowlist.
  ///
  /// It reads the whole file (`check`) to collect
  /// definitions before checking usages.
  ///
  /// Options (secondary): an object with an `allowedProperties` key containing
  /// an array of allowed property patterns (exact strings or `/regex/`).
  ///
  /// Example config:
  /// ```json
  /// ["plugin/no-unknown-custom-properties", [true, {
  ///   "allowedProperties": ["--theme-color", "/^--global-/"]
  /// }]]
  /// ```
  pub struct PluginNoUnknownCustomProperties => PluginNoUnknownCustomPropertiesCheck {
    name: NAME,
    description: "Disallow usage of unknown custom properties",
    default_severity: Warning,
  }
}

/// `plugin/no-unknown-custom-properties` as configured.
pub struct PluginNoUnknownCustomPropertiesCheck {
  /// `allowedProperties`: custom properties that may be used without a definition.
  allowed_properties: Matchers,
}

impl PluginNoUnknownCustomPropertiesCheck {
  /// Gale's own rule, standing in for projects' custom plugins, so it is
  /// lenient: it reads only `allowedProperties`, a name or `/regex/` entry or a list
  /// of them, and ignores its primary option and any other key.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let allowed_properties = options.lenient_strings("allowedProperties");
    options.finish()?;
    Ok(Self {
      allowed_properties: Matchers::new(&allowed_properties),
    })
  }
}

impl Check for PluginNoUnknownCustomPropertiesCheck {
  /// Collects the file's custom property definitions, then flags every `var()`
  /// reference that is neither defined nor allowlisted.
  fn check(&self, tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
    // Collect all definitions in the file
    let defined: HashSet<&str> = tree
      .walk_decls()
      .filter(|decl| decl.is_custom_property())
      .map(|decl| decl.prop())
      .collect();

    let mut diagnostics = Vec::new();
    for decl in tree.walk_decls() {
      for usage in var_references(decl.value()) {
        // Skip if defined in the file
        if defined.contains(usage.name) {
          continue;
        }
        // Skip if in allowlist
        if self.allowed_properties.matches(usage.name) {
          continue;
        }
        diagnostics.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected unknown custom property \"{}\"", usage.name),
          )
          .span(decl.span()),
        );
      }
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
  fn no_diagnostic_when_defined() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let nodes = make_nodes(vec![("--my-color", "red"), ("color", "var(--my-color)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn reports_unknown_custom_property() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let nodes = make_nodes(vec![("color", "var(--unknown)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unknown"));
  }

  #[test]
  fn allowed_property_skipped() {
    let opts = json!([true, {
        "allowedProperties": ["--theme-color"]
    }]);
    let rule = configured(&PluginNoUnknownCustomProperties, opts.clone());
    let nodes = make_nodes(vec![("color", "var(--theme-color)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn allowed_regex_pattern() {
    let opts = json!([true, {
        "allowedProperties": ["/^--global-/"]
    }]);
    let rule = configured(&PluginNoUnknownCustomProperties, opts.clone());
    let nodes = make_nodes(vec![("color", "var(--global-text)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn regex_does_not_match_reports() {
    let opts = json!([true, {
        "allowedProperties": ["/^--global-/"]
    }]);
    let rule = configured(&PluginNoUnknownCustomProperties, opts.clone());
    let nodes = make_nodes(vec![("color", "var(--local-text)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn multiple_vars_in_one_value() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let nodes = make_nodes(vec![
      ("--known", "red"),
      ("color", "var(--known) var(--unknown)"),
    ]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unknown"));
  }

  #[test]
  fn nested_var_checked() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let nodes = make_nodes(vec![("color", "var(--a, var(--b))")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 2);
  }

  #[test]
  fn definitions_in_nested_rules() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let nodes = ":root { --my-var: blue; .child { color: var(--my-var); } }".to_string();
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert!(diags.is_empty());
  }

  #[test]
  fn no_options_works() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let nodes = make_nodes(vec![("color", "var(--undefined)")]);
    let diags = rule.check_source(&ctx_with_source(&nodes));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn options_are_validated_leniently() {
    use crate::testing::invalid_options;
    // Gale's own rule ignores its primary option and keys it does not read.
    let valid =
      serde_json::json!(["anything", { "allowedProperties": ["--a", "/^--global-/"], "other": 1 }]);
    assert!(invalid_options(NAME, valid).is_empty());
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "allowedProperties": [1] }])
      ),
      [
        "Invalid value \"1\" for option \"allowedProperties\" of rule \"plugin/no-unknown-custom-properties\""
      ]
    );
  }
}
