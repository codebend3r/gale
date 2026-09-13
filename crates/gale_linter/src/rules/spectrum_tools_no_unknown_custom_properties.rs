use std::collections::HashSet;
use std::path::Path;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Adobe Spectrum custom plugin: disallow unknown custom properties in `var()`.
///
/// Equivalent to `@spectrum-tools/stylelint-no-unknown-custom-properties`.
/// Only checks `index.css` files. Collects defined custom properties from:
/// - The current file
/// - Sibling `themes/*.css` files in the same component directory
/// - (Optionally) package dependencies
///
/// Reports `var(--name)` references where `--name` is not defined,
/// respecting `ignoreList` regex patterns and skipping vars with fallbacks.
pub struct SpectrumToolsNoUnknownCustomProperties;

impl Rule for SpectrumToolsNoUnknownCustomProperties {
  fn name(&self) -> &'static str {
    "spectrum-tools/no-unknown-custom-properties"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown custom properties (Spectrum)"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let file_path = context.file_path;

    // The plugin only checks index.css files.
    if !file_path.ends_with("index.css") {
      return vec![];
    }

    let source = context.source;

    // Parse options: ignoreList, skipDependencies
    let ignore_patterns = parse_ignore_list(context);

    // Find the component root directory.
    // The plugin looks for a "components" directory in the path and uses the next
    // segment as the component name.
    let component_root = find_component_root(file_path);

    // Collect custom property definitions from theme files.
    let mut known_props: HashSet<String> = HashSet::new();

    if let Some(ref root) = component_root {
      collect_theme_definitions(root, &mut known_props);
    }

    // Collect custom property definitions from the current file.
    collect_definitions_from_source(source, &mut known_props);

    // Walk all declarations, find var() references, report unknown ones.
    let mut diagnostics = Vec::new();
    walk_nodes_for_vars(
      nodes,
      source,
      &known_props,
      &ignore_patterns,
      self,
      &mut diagnostics,
    );

    diagnostics
  }
}

// ---------------------------------------------------------------------------
// Option parsing
// ---------------------------------------------------------------------------

/// Parse the `ignoreList` option from the rule config.
/// Returns a list of regex patterns as strings.
fn parse_ignore_list(context: &RuleContext) -> Vec<String> {
  let secondary = match context.secondary_options() {
    Some(opts) => opts,
    None => return vec![],
  };

  let ignore_list = match secondary.get("ignoreList") {
    Some(serde_json::Value::Array(arr)) => arr,
    _ => return vec![],
  };

  ignore_list
    .iter()
    .filter_map(|v| {
      // Patterns can be strings or regex objects serialized as strings.
      // In Gale's config, JS regexps are serialized as strings like "^--mod-".
      v.as_str().map(|s| s.to_string())
    })
    .collect()
}

/// Check if a property name matches any of the ignore patterns.
fn is_ignored(name: &str, patterns: &[String]) -> bool {
  for pattern in patterns {
    // Simple prefix matching for common patterns like "^--mod-"
    if let Some(prefix) = pattern.strip_prefix('^') {
      // Check for exact match pattern like "^--spectrum-picked-color$"
      if let Some(exact) = prefix.strip_suffix('$') {
        if name == exact {
          return true;
        }
      } else if name.starts_with(prefix) {
        return true;
      }
    } else if name.contains(pattern) {
      return true;
    }
  }
  false
}

// ---------------------------------------------------------------------------
// Component root discovery
// ---------------------------------------------------------------------------

/// Find the component root directory from the file path.
/// Looks for `components/<name>/` in the path.
fn find_component_root(file_path: &str) -> Option<String> {
  // Try to find "components/X/" in the file path.
  // Handle both absolute paths (.../components/X/index.css) and
  // relative paths (components/X/index.css).

  // First try "/components/" for absolute paths
  if let Some(result) = extract_component_root(file_path, "/components/") {
    return Some(result);
  }

  // Try at the start for relative paths like "components/X/..."
  if let Some(after) = file_path.strip_prefix("components/")
    && let Some(slash) = after.find('/')
  {
    return Some(file_path[.."components/".len() + slash].to_string());
  }

  None
}

fn extract_component_root(file_path: &str, pattern: &str) -> Option<String> {
  let idx = file_path.find(pattern)?;
  let after = &file_path[idx + pattern.len()..];
  let slash = after.find('/')?;
  Some(file_path[..idx + pattern.len() + slash].to_string())
}

// ---------------------------------------------------------------------------
// Theme file definitions
// ---------------------------------------------------------------------------

/// Read all CSS files in the `themes/` subdirectory and collect custom property
/// definitions.
fn collect_theme_definitions(component_root: &str, props: &mut HashSet<String>) {
  let themes_dir = Path::new(component_root).join("themes");
  if !themes_dir.is_dir() {
    return;
  }

  let entries = match std::fs::read_dir(&themes_dir) {
    Ok(entries) => entries,
    Err(_) => return,
  };

  for entry in entries.flatten() {
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) == Some("css")
      && let Ok(contents) = std::fs::read_to_string(&path)
    {
      collect_definitions_from_source(&contents, props);
    }
  }
}

// ---------------------------------------------------------------------------
// Collecting custom property definitions
// ---------------------------------------------------------------------------

/// Extract all custom property definitions (`--name: value`) from a CSS source.
fn collect_definitions_from_source(source: &str, props: &mut HashSet<String>) {
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i + 2 < len {
    if bytes[i] == b'-' && bytes[i + 1] == b'-' {
      let is_prop_start = if i == 0 {
        true
      } else {
        let prev = bytes[i - 1];
        prev == b' '
          || prev == b'\t'
          || prev == b'\n'
          || prev == b'\r'
          || prev == b'{'
          || prev == b';'
          || prev == b'('
      };

      if is_prop_start {
        let name_start = i;
        let mut j = i + 2;
        while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b'_')
        {
          j += 1;
        }
        let name = &source[name_start..j];

        let mut k = j;
        while k < len && (bytes[k] == b' ' || bytes[k] == b'\t') {
          k += 1;
        }

        if k < len && bytes[k] == b':' && (k + 1 >= len || bytes[k + 1] != b':') {
          props.insert(name.to_string());
        }

        i = j;
        continue;
      }
    }
    i += 1;
  }
}

// ---------------------------------------------------------------------------
// Walking AST for var() references
// ---------------------------------------------------------------------------

/// Recursively walk AST nodes, checking declarations for unknown var() references.
fn walk_nodes_for_vars(
  nodes: &[CssNode],
  source: &str,
  known_props: &HashSet<String>,
  ignore_patterns: &[String],
  rule: &SpectrumToolsNoUnknownCustomProperties,
  diagnostics: &mut Vec<Diagnostic>,
) {
  for node in nodes {
    match node {
      CssNode::Style(style_rule) => {
        for decl in &style_rule.declarations {
          check_declaration(
            decl,
            source,
            known_props,
            ignore_patterns,
            rule,
            diagnostics,
          );
        }
        // Recurse into nested style rule children.
        walk_nodes_for_vars(
          &style_rule
            .children
            .iter()
            .map(|c| CssNode::Style(c.clone()))
            .collect::<Vec<_>>(),
          source,
          known_props,
          ignore_patterns,
          rule,
          diagnostics,
        );
        // Recurse into nested at-rules.
        walk_nodes_for_vars(
          &style_rule.nested_at_rules,
          source,
          known_props,
          ignore_patterns,
          rule,
          diagnostics,
        );
      }
      CssNode::AtRule(at_rule) => {
        walk_nodes_for_vars(
          &at_rule.children,
          source,
          known_props,
          ignore_patterns,
          rule,
          diagnostics,
        );
      }
      CssNode::Declaration(decl) => {
        check_declaration(
          decl,
          source,
          known_props,
          ignore_patterns,
          rule,
          diagnostics,
        );
      }
      CssNode::Comment(_) => {}
    }
  }
}

/// Check a single declaration for unknown var() custom property references.
fn check_declaration(
  decl: &gale_css_parser::Declaration,
  source: &str,
  known_props: &HashSet<String>,
  ignore_patterns: &[String],
  rule: &SpectrumToolsNoUnknownCustomProperties,
  diagnostics: &mut Vec<Diagnostic>,
) {
  if !decl.value.contains("var(") && !decl.value.contains("VAR(") {
    return;
  }

  // Find all var() references in the value.
  let var_refs = find_var_refs_in_value(&decl.value);

  for vr in &var_refs {
    // Check ignoreList
    if is_ignored(&vr.name, ignore_patterns) {
      continue;
    }

    // Check if defined locally or in themes
    if known_props.contains(&vr.name) {
      continue;
    }

    // NOTE: The original spectrum plugin checks `secondNode.type === "div"`
    // to skip vars with fallbacks, but postcss-values-parser v6 uses "comma"
    // nodes (not "div"), so the check always fails — the plugin effectively
    // never skips vars with fallbacks. We match this behavior.

    // Report at the declaration position (matching Stylelint's behavior
    // which uses `node: decl` in the report call).
    diagnostics.push(
      Diagnostic::new(
        rule.name(),
        format!("Custom property {} not defined", vr.name),
      )
      .severity(rule.default_severity())
      .span(Span::new(decl.span.offset, decl.span.length)),
    );
  }
}

// ---------------------------------------------------------------------------
// var() reference finding
// ---------------------------------------------------------------------------

struct VarRef {
  name: String,
  has_fallback: bool,
}

/// Find all `var(--name)` references in a value string, at all nesting levels.
fn find_var_refs_in_value(value: &str) -> Vec<VarRef> {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut refs = Vec::new();
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
      while i < len && is_whitespace(bytes[i]) {
        i += 1;
      }

      if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
        let name_start = i;
        while i < len && is_name_char(bytes[i]) {
          i += 1;
        }
        let prop_name = &value[name_start..i];

        // Skip whitespace
        while i < len && is_whitespace(bytes[i]) {
          i += 1;
        }

        let has_fallback = i < len && bytes[i] == b',';

        refs.push(VarRef {
          name: prop_name.to_string(),
          has_fallback,
        });

        continue;
      }
    }

    i += 1;
  }

  refs
}

#[inline]
fn is_whitespace(b: u8) -> bool {
  b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
}

#[inline]
fn is_name_char(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn find_component_root_absolute() {
    let root = find_component_root("/path/to/spectrum-css/components/accordion/index.css");
    assert_eq!(
      root,
      Some("/path/to/spectrum-css/components/accordion".to_string())
    );
  }

  #[test]
  fn find_component_root_relative() {
    let root = find_component_root("components/accordion/index.css");
    assert_eq!(root, Some("components/accordion".to_string()));
  }

  #[test]
  fn is_ignored_prefix_pattern() {
    assert!(is_ignored("--mod-something", &["^--mod-".to_string()]));
    assert!(!is_ignored(
      "--spectrum-something",
      &["^--mod-".to_string()]
    ));
  }

  #[test]
  fn is_ignored_exact_pattern() {
    assert!(is_ignored(
      "--spectrum-picked-color",
      &["^--spectrum-picked-color$".to_string()]
    ));
    assert!(!is_ignored(
      "--spectrum-picked-color-extra",
      &["^--spectrum-picked-color$".to_string()]
    ));
  }

  #[test]
  fn collect_definitions() {
    let source = ":root { --my-color: red; --spacing: 8px; }";
    let mut props = HashSet::new();
    collect_definitions_from_source(source, &mut props);
    assert!(props.contains("--my-color"));
    assert!(props.contains("--spacing"));
  }

  #[test]
  fn find_vars_with_fallback() {
    let refs = find_var_refs_in_value("var(--a, var(--b))");
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].name, "--a");
    assert!(refs[0].has_fallback);
    assert_eq!(refs[1].name, "--b");
    assert!(!refs[1].has_fallback);
  }

  #[test]
  fn skips_non_index_css() {
    let rule = SpectrumToolsNoUnknownCustomProperties;
    let ctx = RuleContext {
      file_path: "components/foo/themes/spectrum.css",
      source: ".a { color: var(--unknown); }",
      syntax: gale_css_parser::Syntax::Css,
      options: None,
    };
    let parsed = gale_css_parser::parse(ctx.source, gale_css_parser::Syntax::Css).unwrap();
    let diags = rule.check_root(&parsed.nodes, &ctx);
    assert!(diags.is_empty());
  }
}
