use std::collections::{HashMap, HashSet};

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "plugin/no-unused-custom-properties";

declare_rule! {
  /// Report custom properties that are defined but never referenced via `var()`.
  ///
  /// This is a `check_root` rule that needs full file context to collect
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

/// A custom property definition with its location.
struct Definition {
  name: String,
  offset: usize,
  length: usize,
}

/// Collect all custom property definitions (`--name: value`) from nodes.
fn collect_definitions(nodes: &[CssNode], defs: &mut Vec<Definition>) {
  for node in nodes {
    for decl in node.declarations() {
      if decl.property.starts_with("--") {
        defs.push(Definition {
          name: decl.property.clone(),
          offset: decl.span.offset,
          length: decl.span.length,
        });
      }
    }
    match node {
      CssNode::Style(rule) => {
        let child_nodes: Vec<CssNode> = rule
          .children
          .iter()
          .map(|c| CssNode::Style(c.clone()))
          .collect();
        collect_definitions(&child_nodes, defs);
        collect_definitions(&rule.nested_at_rules, defs);
      }
      CssNode::AtRule(at_rule) => {
        collect_definitions(&at_rule.children, defs);
      }
      CssNode::Declaration(_) | CssNode::Comment(_) => {}
    }
  }
}

/// Collect all var(--name) referenced property names.
fn collect_var_references(nodes: &[CssNode], refs: &mut HashSet<String>) {
  for node in nodes {
    for decl in node.declarations() {
      collect_var_refs_from_value(&decl.value, refs);
    }
    match node {
      CssNode::Style(rule) => {
        let child_nodes: Vec<CssNode> = rule
          .children
          .iter()
          .map(|c| CssNode::Style(c.clone()))
          .collect();
        collect_var_references(&child_nodes, refs);
        collect_var_references(&rule.nested_at_rules, refs);
      }
      CssNode::AtRule(at_rule) => {
        collect_var_references(&at_rule.children, refs);
      }
      CssNode::Declaration(_) | CssNode::Comment(_) => {}
    }
  }
}

/// Extract var(--name) references from a value string.
fn collect_var_refs_from_value(value: &str, refs: &mut HashSet<String>) {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if i + 4 <= len
      && (bytes[i] == b'v' || bytes[i] == b'V')
      && (bytes[i + 1] == b'a' || bytes[i + 1] == b'A')
      && (bytes[i + 2] == b'r' || bytes[i + 2] == b'R')
      && bytes[i + 3] == b'('
    {
      if i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        i += 1;
        continue;
      }

      i += 4; // skip "var("

      // Skip whitespace
      while i < len && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
      }

      if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
        {
          i += 1;
        }
        let prop_name = &value[name_start..i];
        refs.insert(prop_name.to_string());
        continue;
      }
    }
    i += 1;
  }
}

impl Check for PluginNoUnusedCustomPropertiesCheck {
  /// Flags custom properties that no `var()` in the file references, reporting
  /// each name once at its first definition.
  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    // Collect all definitions
    let mut definitions: Vec<Definition> = Vec::new();
    collect_definitions(nodes, &mut definitions);

    // Collect all var() references
    let mut references: HashSet<String> = HashSet::new();
    collect_var_references(nodes, &mut references);

    // Deduplicate definitions by name — only report each unused name once
    // (at the first definition site).
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut diagnostics = Vec::new();

    for def in &definitions {
      // Skip if already seen (report only first definition)
      if seen.contains_key(&def.name) {
        continue;
      }
      seen.insert(def.name.clone(), def.offset);

      // Skip if referenced
      if references.contains(&def.name) {
        continue;
      }

      // Skip if ignored
      if self.ignore_properties.matches(&def.name) {
        continue;
      }

      diagnostics.push(
        Diagnostic::new(
          NAME,
          format!("Unexpected unused custom property \"{}\"", def.name),
        )
        .span(Span::new(def.offset, def.length)),
      );
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule};
  use serde_json::json;

  use crate::testing::{configured, ctx};

  fn make_nodes(decls: Vec<(&str, &str)>) -> Vec<CssNode> {
    vec![CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: decls
        .into_iter()
        .map(|(p, v)| Declaration {
          property: p.to_string(),
          value: v.to_string(),
          span: Span::new(0, 10),
          important: false,
        })
        .collect(),
      span: Span::new(0, 0),
      children: vec![],
      nested_at_rules: vec![],
    })]
  }

  #[test]
  fn no_diagnostic_when_used() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("--my-color", "red"), ("color", "var(--my-color)")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn reports_unused_custom_property() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("--unused", "red"), ("color", "blue")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unused"));
  }

  #[test]
  fn ignored_property_not_reported() {
    let opts = json!([true, {
        "ignoreProperties": ["/^--external-/"]
    }]);
    let rule = configured(&PluginNoUnusedCustomProperties, opts.clone());
    let ctx = ctx();
    let nodes = make_nodes(vec![("--external-api", "red")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignored_exact_property() {
    let opts = json!([true, {
        "ignoreProperties": ["--my-var"]
    }]);
    let rule = configured(&PluginNoUnusedCustomProperties, opts.clone());
    let ctx = ctx();
    let nodes = make_nodes(vec![("--my-var", "red")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn multiple_unused() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("--a", "1"), ("--b", "2"), ("color", "var(--a)")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--b"));
  }

  #[test]
  fn used_in_fallback() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![
      ("--fallback", "blue"),
      ("color", "var(--primary, var(--fallback))"),
    ]);
    let diags = rule.check_root(&nodes, &ctx);
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
    let ctx = ctx();
    let nodes = make_nodes(vec![("color", "red")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn no_options_works() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("--unused", "red")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn duplicate_definition_reported_once() {
    let rule = configured(&PluginNoUnusedCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("--dup", "a"), ("--dup", "b")]);
    let diags = rule.check_root(&nodes, &ctx);
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
