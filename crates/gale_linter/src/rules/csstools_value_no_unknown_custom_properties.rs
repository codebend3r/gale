use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Span};

use crate::custom_properties::{collect_definitions_from_source, var_references};
use crate::lint_env::{LintEnv, find_node_modules};
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "csstools/value-no-unknown-custom-properties";

declare_rule! {
  /// Disallow unknown custom properties referenced via `var()`.
  ///
  /// Equivalent to the csstools plugin `csstools/value-no-unknown-custom-properties`.
  /// Checks that every `var(--name)` reference points to a custom property that is
  /// actually defined — either in the same file or in one of the `importFrom` sources.
  pub struct CsstoolsValueNoUnknownCustomProperties => CsstoolsValueNoUnknownCustomPropertiesCheck {
    name: NAME,
    description: "Disallow unknown custom properties",
    default_severity: Warning,
  }
}

/// `csstools/value-no-unknown-custom-properties` as configured.
pub struct CsstoolsValueNoUnknownCustomPropertiesCheck {
  /// The `importFrom` sources, sorted.
  import_from: Vec<String>,
}

impl CsstoolsValueNoUnknownCustomPropertiesCheck {
  /// The plugin's options: `true` (its `possible` is a function that takes
  /// `true`, `false` or `null`), then `importFrom`, a path or a list of
  /// them.  The plugin validates no secondary option, so gale reads
  /// `importFrom` as written and ignores any other key.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_test(|value| value.is_boolean() || value.is_null());
    options.finish()?;
    let mut import_from: Vec<String> = match setting.secondary().and_then(|s| s.get("importFrom")) {
      Some(serde_json::Value::Array(paths)) => paths
        .iter()
        .filter_map(|path| path.as_str().map(str::to_string))
        .collect(),
      Some(serde_json::Value::String(path)) => vec![path.clone()],
      _ => Vec::new(),
    };
    import_from.sort();
    Ok(Self { import_from })
  }
}

/// Global cache: maps a canonical sorted key of import paths to the set of
/// custom property names defined in those files.
static IMPORT_CACHE: OnceLock<
  std::sync::Mutex<std::collections::HashMap<String, HashSet<String>>>,
> = OnceLock::new();

/// Process-wide cache of parsed `importFrom` sources, keyed by path list.
fn import_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, HashSet<String>>> {
  IMPORT_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

impl Check for CsstoolsValueNoUnknownCustomPropertiesCheck {
  /// Collects custom property definitions from `importFrom` and from this file,
  /// then flags every `var()` reference that resolves to none of them.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let source = context.source;

    // 1. Collect custom property definitions from importFrom files.
    let imported_props = load_imported_custom_properties(&self.import_from, context);

    // 2. Collect custom property definitions from the current file.
    let mut known_props: HashSet<String> = imported_props;
    collect_definitions_from_source(source, &mut known_props);

    // 3. Walk every declaration, find var() references, report unknown ones.
    let mut diagnostics = Vec::new();
    for decl in tree.walk_decls() {
      check_declaration(decl, source, &known_props, &mut diagnostics);
    }

    diagnostics
  }
}

/// Check a single declaration for unknown var() custom property references.
fn check_declaration(
  decl: DeclRef,
  source: &str,
  known_props: &HashSet<String>,
  diagnostics: &mut Vec<Diagnostic>,
) {
  // The plugin reads `decl.value`.
  let value = decl.value();
  if !value.contains("var(") && !value.contains("VAR(") {
    return;
  }

  let decl_source_start = decl.start;

  for vr in var_references(value) {
    if known_props.contains(vr.name) {
      continue;
    }
    if vr.has_fallback {
      continue;
    }

    // Stylelint uses PostCSS's `rangeBy({ word })` which finds the FIRST
    // occurrence of the word in the declaration's source representation.
    // We replicate this by searching for the property name from the
    // declaration start in the source.
    let name_offset = match source[decl_source_start..].find(vr.name) {
      Some(idx) => decl_source_start + idx,
      None => decl.value_start() + vr.offset,
    };

    diagnostics.push(
      Diagnostic::new(
        NAME,
        format!(
          "Unexpected custom property \"{}\" inside declaration \"{}\".",
          vr.name,
          decl.prop()
        ),
      )
      .span(Span::new(name_offset, vr.name.len())),
    );
  }
}

// ---------------------------------------------------------------------------
// Loading imported custom properties
// ---------------------------------------------------------------------------

/// Reads the `importFrom` sources and returns the custom properties they define,
/// memoising the result on the sorted path list.
fn load_imported_custom_properties(paths: &[String], context: &RuleContext) -> HashSet<String> {
  if paths.is_empty() {
    return HashSet::new();
  }
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

  for path_str in paths {
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

/// Resolves an `importFrom` entry: absolute as-is, bare specifiers against
/// `node_modules`, relative against the current directory.
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::{CheckSource, configured, invalid_options};
  use gale_css_parser::Syntax;

  fn lint(source: &str) -> Vec<Diagnostic> {
    let rule = configured(&CsstoolsValueNoUnknownCustomProperties, None);
    let context = RuleContext {
      env: Default::default(),
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      cache: None,
    };
    rule.check_source(&context)
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

  #[test]
  fn names_the_property_as_written_at_the_reference() {
    let source = "@media screen { .a { COLOR: var( --unknown) } }";
    let diags = lint(source);
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected custom property \"--unknown\" inside declaration \"COLOR\"."
    );
    assert_eq!(
      &source[diags[0].span.offset..diags[0].span.end()],
      "--unknown"
    );
  }

  #[test]
  fn options_are_validated_like_the_plugin() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "importFrom": "vars.css", "x": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option \"always\" for rule \"csstools/value-no-unknown-custom-properties\""]
    );
  }
}
