use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Options, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "plugin/no-unknown-custom-properties";

declare_rule! {
  /// Report usage of CSS custom properties (`var(--foo)`) that are not defined
  /// in the current file or a configurable allowlist.
  ///
  /// This is a `check_root` rule that needs full file context to collect
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
    let allowed_properties = lenient_strings(setting, &mut options, "allowedProperties");
    options.finish()?;
    Ok(Self {
      allowed_properties: Matchers::new(&allowed_properties),
    })
  }
}

/// The secondary option `key`, a name or `/regex/` entry or a list of them,
/// read without making other keys unknown.  An entry that is not a string
/// is reported as Stylelint words it.
fn lenient_strings(setting: &Setting, options: &mut Options<'_>, key: &str) -> Vec<String> {
  let Some(value) = setting.secondary().and_then(|secondary| secondary.get(key)) else {
    return Vec::new();
  };
  let entries: Vec<&serde_json::Value> = match value {
    serde_json::Value::Array(items) => items.iter().collect(),
    other => vec![other],
  };
  let mut strings = Vec::new();
  for entry in entries {
    match entry.as_str() {
      Some(text) => strings.push(text.to_string()),
      None => options.report(InvalidOption::option_value(NAME, key, entry)),
    }
  }
  strings
}

/// Collect all custom property definitions (`--name: value`) from nodes.
fn collect_definitions(nodes: &[CssNode], defs: &mut HashSet<String>) {
  for node in nodes {
    match node {
      CssNode::Declaration(decl) => {
        if decl.property.starts_with("--") {
          defs.insert(decl.property.clone());
        }
      }
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          if decl.property.starts_with("--") {
            defs.insert(decl.property.clone());
          }
        }
        // Recurse into nested children
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
      CssNode::Comment(_) => {}
    }
  }
}

/// A var() usage found in the source.
struct VarUsage {
  name: String,
  /// Byte offset of the declaration containing this var() reference.
  decl_offset: usize,
  /// Byte length of the declaration containing this var() reference.
  decl_length: usize,
}

/// Find all `var(--name)` usages in declarations within nodes.
fn collect_usages(nodes: &[CssNode], usages: &mut Vec<VarUsage>) {
  for node in nodes {
    match node {
      CssNode::Declaration(decl) => {
        collect_var_refs_from_value(&decl.value, decl.span.offset, decl.span.length, usages);
      }
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          collect_var_refs_from_value(&decl.value, decl.span.offset, decl.span.length, usages);
        }
        let child_nodes: Vec<CssNode> = rule
          .children
          .iter()
          .map(|c| CssNode::Style(c.clone()))
          .collect();
        collect_usages(&child_nodes, usages);
        collect_usages(&rule.nested_at_rules, usages);
      }
      CssNode::AtRule(at_rule) => {
        collect_usages(&at_rule.children, usages);
      }
      CssNode::Comment(_) => {}
    }
  }
}

/// Extract var(--name) references from a value string.
fn collect_var_refs_from_value(
  value: &str,
  decl_offset: usize,
  decl_length: usize,
  usages: &mut Vec<VarUsage>,
) {
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
      // Make sure this is not part of another identifier
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
        usages.push(VarUsage {
          name: prop_name.to_string(),
          decl_offset,
          decl_length,
        });
        continue;
      }
    }
    i += 1;
  }
}

impl Check for PluginNoUnknownCustomPropertiesCheck {
  /// Collects the file's custom property definitions, then flags every `var()`
  /// reference that is neither defined nor allowlisted.
  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    // Collect all definitions in the file
    let mut defined: HashSet<String> = HashSet::new();
    collect_definitions(nodes, &mut defined);

    // Collect all var() usages
    let mut usages: Vec<VarUsage> = Vec::new();
    collect_usages(nodes, &mut usages);

    let mut diagnostics = Vec::new();
    for usage in &usages {
      // Skip if defined in the file
      if defined.contains(&usage.name) {
        continue;
      }
      // Skip if in allowlist
      if self.allowed_properties.matches(&usage.name) {
        continue;
      }
      diagnostics.push(
        Diagnostic::new(
          NAME,
          format!("Unexpected unknown custom property \"{}\"", usage.name),
        )
        .span(Span::new(usage.decl_offset, usage.decl_length)),
      );
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};
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
          span: ParserSpan::new(0, 10),
          important: false,
        })
        .collect(),
      span: ParserSpan::new(0, 0),
      children: vec![],
      nested_at_rules: vec![],
    })]
  }

  #[test]
  fn no_diagnostic_when_defined() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("--my-color", "red"), ("color", "var(--my-color)")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn reports_unknown_custom_property() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("color", "var(--unknown)")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unknown"));
  }

  #[test]
  fn allowed_property_skipped() {
    let opts = json!([true, {
        "allowedProperties": ["--theme-color"]
    }]);
    let rule = configured(&PluginNoUnknownCustomProperties, opts.clone());
    let ctx = ctx();
    let nodes = make_nodes(vec![("color", "var(--theme-color)")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn allowed_regex_pattern() {
    let opts = json!([true, {
        "allowedProperties": ["/^--global-/"]
    }]);
    let rule = configured(&PluginNoUnknownCustomProperties, opts.clone());
    let ctx = ctx();
    let nodes = make_nodes(vec![("color", "var(--global-text)")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn regex_does_not_match_reports() {
    let opts = json!([true, {
        "allowedProperties": ["/^--global-/"]
    }]);
    let rule = configured(&PluginNoUnknownCustomProperties, opts.clone());
    let ctx = ctx();
    let nodes = make_nodes(vec![("color", "var(--local-text)")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn multiple_vars_in_one_value() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![
      ("--known", "red"),
      ("color", "var(--known) var(--unknown)"),
    ]);
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unknown"));
  }

  #[test]
  fn nested_var_checked() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("color", "var(--a, var(--b))")]);
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 2);
  }

  #[test]
  fn definitions_in_nested_rules() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let ctx = ctx();
    let nodes = vec![CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![Declaration {
        property: "--my-var".to_string(),
        value: "blue".to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      children: vec![StyleRule {
        selector: ".child".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "var(--my-var)".to_string(),
          span: ParserSpan::new(20, 15),
          important: false,
        }],
        span: ParserSpan::new(20, 15),
        children: vec![],
        nested_at_rules: vec![],
      }],
      nested_at_rules: vec![],
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn no_options_works() {
    let rule = configured(&PluginNoUnknownCustomProperties, None);
    let ctx = ctx();
    let nodes = make_nodes(vec![("color", "var(--undefined)")]);
    let diags = rule.check_root(&nodes, &ctx);
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
