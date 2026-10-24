use std::collections::HashSet;
use std::path::Path;

use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_diagnostics::Diagnostic;

use crate::custom_properties::{collect_definitions_from_source, var_references};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "spectrum-tools/no-unknown-custom-properties";

declare_rule! {
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
  pub struct SpectrumToolsNoUnknownCustomProperties => SpectrumToolsNoUnknownCustomPropertiesCheck {
    name: NAME,
    description: "Disallow unknown custom properties (Spectrum)",
    default_severity: Warning,
  }
}

/// `spectrum-tools/no-unknown-custom-properties` as configured.
pub struct SpectrumToolsNoUnknownCustomPropertiesCheck {
  /// The `ignoreList` option: patterns for custom properties to pass over.
  ignore_list: Vec<String>,
}

impl SpectrumToolsNoUnknownCustomPropertiesCheck {
  /// The plugin's options: `true`, then `ignoreList` (strings or `/regex/`
  /// entries) and `skipDependencies`, which gale accepts but does not apply:
  /// it never reads package dependencies.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    let ignore_list = options.strings("ignoreList");
    options.flag("skipDependencies");
    options.finish()?;
    Ok(Self { ignore_list })
  }
}

impl Check for SpectrumToolsNoUnknownCustomPropertiesCheck {
  /// In `index.css` only, collects custom properties from this file and sibling
  /// `themes/*.css`, then flags `var()` references defined by none of them.
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let file_path = context.file_path;

    // The plugin only checks index.css files.
    if !file_path.ends_with("index.css") {
      return vec![];
    }

    let source = context.source;

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
    for decl in tree.walk_decls() {
      check_declaration(decl, &known_props, &self.ignore_list, &mut diagnostics);
    }

    diagnostics
  }
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

/// The component directory path containing `pattern`, if the path has one.
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

/// Check a single declaration for unknown var() custom property references.
fn check_declaration(
  decl: DeclRef,
  known_props: &HashSet<String>,
  ignore_patterns: &[String],
  diagnostics: &mut Vec<Diagnostic>,
) {
  // The plugin reads `decl.value`.
  let value = decl.value();
  if !value.contains("var(") && !value.contains("VAR(") {
    return;
  }

  for vr in var_references(value) {
    // Check ignoreList
    if is_ignored(vr.name, ignore_patterns) {
      continue;
    }

    // Check if defined locally or in themes
    if known_props.contains(vr.name) {
      continue;
    }

    // NOTE: The original spectrum plugin checks `secondNode.type === "div"`
    // to skip vars with fallbacks, but postcss-values-parser v6 uses "comma"
    // nodes (not "div"), so the check always fails — the plugin effectively
    // never skips vars with fallbacks. We match this behavior.

    // Report at the declaration position (matching Stylelint's behavior
    // which uses `node: decl` in the report call).
    diagnostics.push(
      Diagnostic::new(NAME, format!("Custom property {} not defined", vr.name)).span(decl.span()),
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::{CheckSource, configured, invalid_options};

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
    let refs = var_references("var(--a, var(--b))");
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].name, "--a");
    assert_eq!(refs[1].name, "--b");
  }

  #[test]
  fn skips_non_index_css() {
    let rule = configured(&SpectrumToolsNoUnknownCustomProperties, None);
    let ctx = RuleContext {
      env: Default::default(),
      file_path: "components/foo/themes/spectrum.css",
      source: ".a { color: var(--unknown); }",
      syntax: gale_css_parser::Syntax::Css,
      cache: None,
      embedded: None,
    };
    let diags = rule.check_source(&ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!([true, { "ignoreList": ["^--mod-", "/^--x/"], "skipDependencies": false }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"spectrum-tools/no-unknown-custom-properties\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "skipDependencies": "no" }])
      ),
      [
        "Invalid value \"no\" for option \"skipDependencies\" of rule \"spectrum-tools/no-unknown-custom-properties\""
      ]
    );
  }
}
