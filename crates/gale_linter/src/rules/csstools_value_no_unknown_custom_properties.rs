use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow unknown custom properties referenced via `var()`.
///
/// Equivalent to the csstools plugin `csstools/value-no-unknown-custom-properties`.
/// Checks that every `var(--name)` reference points to a custom property that is
/// actually defined — either in the same file or in one of the `importFrom` sources.
pub struct CsstoolsValueNoUnknownCustomProperties;

/// Global cache: maps a canonical sorted key of import paths to the set of
/// custom property names defined in those files.
static IMPORT_CACHE: OnceLock<
  std::sync::Mutex<std::collections::HashMap<String, HashSet<String>>>,
> = OnceLock::new();

fn import_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, HashSet<String>>> {
  IMPORT_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

impl Rule for CsstoolsValueNoUnknownCustomProperties {
  fn name(&self) -> &'static str {
    "csstools/value-no-unknown-custom-properties"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown custom properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let source = context.source;

    // 1. Collect custom property definitions from importFrom files.
    let imported_props = load_imported_custom_properties(context);

    // 2. Collect custom property definitions from the current file.
    let mut known_props: HashSet<String> = imported_props;
    collect_definitions_from_source(source, &mut known_props);

    // 3. Walk all AST declarations, find var() references, report unknown ones.
    let mut diagnostics = Vec::new();
    walk_nodes_for_vars(nodes, source, &known_props, self, &mut diagnostics);

    diagnostics
  }
}

// ---------------------------------------------------------------------------
// Walking AST for declarations with var() references
// ---------------------------------------------------------------------------

/// Recursively walk AST nodes, checking declarations for unknown var() references.
fn walk_nodes_for_vars(
  nodes: &[CssNode],
  source: &str,
  known_props: &HashSet<String>,
  rule: &CsstoolsValueNoUnknownCustomProperties,
  diagnostics: &mut Vec<Diagnostic>,
) {
  for node in nodes {
    match node {
      CssNode::Style(style_rule) => {
        // Check declarations in this style rule.
        for decl in &style_rule.declarations {
          check_declaration(decl, source, known_props, rule, diagnostics);
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
          rule,
          diagnostics,
        );
        // Recurse into nested at-rules.
        walk_nodes_for_vars(
          &style_rule.nested_at_rules,
          source,
          known_props,
          rule,
          diagnostics,
        );
      }
      CssNode::AtRule(at_rule) => {
        // Recurse into child rules (e.g., @media, @keyframes blocks).
        walk_nodes_for_vars(&at_rule.children, source, known_props, rule, diagnostics);
      }
      CssNode::Declaration(decl) => {
        check_declaration(decl, source, known_props, rule, diagnostics);
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
  rule: &CsstoolsValueNoUnknownCustomProperties,
  diagnostics: &mut Vec<Diagnostic>,
) {
  // Only process if the value contains `var(`.
  if !decl.value.contains("var(") && !decl.value.contains("VAR(") {
    return;
  }

  // Find the byte offset of the value in the source.
  let value_offset = find_value_offset(source, decl);

  // Find all var() references in the value.
  let var_refs = find_var_refs_in_value(&decl.value, value_offset);

  // The declaration source representation (from property start to value end).
  // Used for word-based position lookup matching Stylelint's behavior.
  let decl_source_start = decl.span.offset;

  for vr in &var_refs {
    if known_props.contains(&vr.name) {
      continue;
    }
    if vr.has_fallback {
      continue;
    }

    // Stylelint uses PostCSS's `rangeBy({ word })` which finds the FIRST
    // occurrence of the word in the declaration's source representation.
    // We replicate this by searching for the property name from the
    // declaration start in the source.
    let name_offset = if let Some(idx) = source[decl_source_start..].find(&vr.name) {
      decl_source_start + idx
    } else {
      vr.name_offset
    };

    diagnostics.push(
      Diagnostic::new(
        rule.name(),
        format!(
          "Unexpected custom property \"{}\" inside declaration \"{}\".",
          vr.name, decl.property
        ),
      )
      .severity(rule.default_severity())
      .span(Span::new(name_offset, vr.name.len())),
    );
  }
}

/// Find the byte offset of the declaration value in the source.
/// Uses the declaration's span (pointing to property start) and searches
/// for the colon to find where the value starts.
fn find_value_offset(source: &str, decl: &gale_css_parser::Declaration) -> usize {
  let start = decl.span.offset;
  let bytes = source.as_bytes();
  let len = bytes.len();

  // Find the colon after the property name.
  let mut i = start;
  while i < len && bytes[i] != b':' {
    i += 1;
  }
  if i < len {
    i += 1; // skip colon
  }

  // Skip whitespace after colon.
  while i < len && is_whitespace(bytes[i]) {
    i += 1;
  }

  i
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
// Loading imported custom properties
// ---------------------------------------------------------------------------

fn load_imported_custom_properties(context: &RuleContext) -> HashSet<String> {
  let secondary = match context.secondary_options() {
    Some(opts) => opts,
    None => return HashSet::new(),
  };

  let import_from = match secondary.get("importFrom") {
    Some(serde_json::Value::Array(arr)) => arr,
    _ => return HashSet::new(),
  };

  if import_from.is_empty() {
    return HashSet::new();
  }

  let mut paths: Vec<&str> = import_from.iter().filter_map(|v| v.as_str()).collect();
  paths.sort();
  let cache_key = paths.join("\n");

  {
    let cache = import_cache().lock().unwrap();
    if let Some(cached) = cache.get(&cache_key) {
      return cached.clone();
    }
  }

  let file_path = Path::new(context.file_path);
  let node_modules = find_node_modules(file_path);

  let mut props = HashSet::new();

  for path_str in &paths {
    let resolved = resolve_import_path(path_str, node_modules.as_deref());
    if let Some(resolved_path) = resolved
      && let Ok(contents) = std::fs::read_to_string(&resolved_path)
    {
      collect_definitions_from_source(&contents, &mut props);
    }
  }

  {
    let mut cache = import_cache().lock().unwrap();
    cache.insert(cache_key, props.clone());
  }

  props
}

fn find_node_modules(file_path: &Path) -> Option<std::path::PathBuf> {
  let mut dir = if file_path.is_file() {
    file_path.parent()?
  } else {
    file_path
  };

  loop {
    let nm = dir.join("node_modules");
    if nm.is_dir() {
      return Some(nm);
    }
    dir = dir.parent()?;
  }
}

fn resolve_import_path(path_str: &str, node_modules: Option<&Path>) -> Option<std::path::PathBuf> {
  let p = Path::new(path_str);

  if p.is_absolute() && p.exists() {
    return Some(p.to_path_buf());
  }

  if !path_str.starts_with('.')
    && !path_str.starts_with('/')
    && let Some(nm) = node_modules
  {
    let resolved = nm.join(path_str);
    if resolved.exists() {
      return Some(resolved);
    }
  }

  if path_str.starts_with('.') {
    let cwd = std::env::current_dir().ok()?;
    let resolved = cwd.join(path_str);
    if resolved.exists() {
      return Some(resolved);
    }
  }

  None
}

// ---------------------------------------------------------------------------
// var() reference finding
// ---------------------------------------------------------------------------

struct VarRef {
  name: String,
  name_offset: usize,
  has_fallback: bool,
}

/// Find all `var(--name)` references in a value string, at all nesting levels.
fn find_var_refs_in_value(value: &str, base_offset: usize) -> Vec<VarRef> {
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

      i += 4;

      while i < len && is_whitespace(bytes[i]) {
        i += 1;
      }

      if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
        let name_start = i;
        while i < len && is_name_char(bytes[i]) {
          i += 1;
        }
        let prop_name = &value[name_start..i];

        while i < len && is_whitespace(bytes[i]) {
          i += 1;
        }

        let has_fallback = i < len && bytes[i] == b',';

        refs.push(VarRef {
          name: prop_name.to_string(),
          name_offset: base_offset + name_start,
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
  use gale_css_parser::Syntax;

  fn lint(source: &str) -> Vec<Diagnostic> {
    let rule = CsstoolsValueNoUnknownCustomProperties;
    let parsed = gale_css_parser::parse(source, Syntax::Css).unwrap();
    let context = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    rule.check_root(&parsed.nodes, &context)
  }

  #[test]
  fn collects_definitions() {
    let source = ":root { --my-color: red; --spacing: 8px; }";
    let mut props = HashSet::new();
    collect_definitions_from_source(source, &mut props);
    assert!(props.contains("--my-color"));
    assert!(props.contains("--spacing"));
  }

  #[test]
  fn reports_unknown_custom_property() {
    let diags = lint(".a { color: var(--unknown); }");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unknown"));
    assert!(diags[0].message.contains("color"));
  }

  #[test]
  fn allows_defined_custom_property() {
    let diags = lint(":root { --my-color: red; } .a { color: var(--my-color); }");
    assert!(diags.is_empty());
  }

  #[test]
  fn skips_unknown_with_fallback_reports_inner() {
    let diags = lint(".a { background: var(--overlay-bg, var(--unknown-fallback)); }");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unknown-fallback"));
  }

  #[test]
  fn known_with_fallback_visits_inner() {
    let diags = lint(":root { --known: red; } .a { color: var(--known, var(--unknown)); }");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--unknown"));
  }

  #[test]
  fn checks_custom_property_definitions() {
    let diags = lint(":root { --my-color: var(--base-color); }");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("--base-color"));
  }
}
