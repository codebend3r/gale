//! Turning a raw config into the [`GaleConfig`] gale lints with: `extends`,
//! the config's own rules, overrides, and the CLI-equivalent options.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::extends::{ExtendedOverride, collect_rules_from_extends};
use crate::plugins::extract_plugin_names;
use crate::{
  ConfigFile, ConfigOverride, FixMode, GaleConfig, ResolvedOverride, RuleConfig, Severity,
};

/// Convert a raw [`ConfigFile`] into the resolved [`GaleConfig`].
///
/// `base_dir` is used when resolving `extends` that reference npm packages or
/// relative paths.
pub(crate) fn resolve_raw(raw: ConfigFile, base_dir: &Path) -> GaleConfig {
  // 1. Start with rules from extended presets / npm configs (in order).
  //    Also collect any overrides defined in extended configs.
  let mut rules: HashMap<String, RuleConfig> = HashMap::new();
  let mut extended_overrides = Vec::new();
  if let Some(ref extends) = raw.extends {
    let mut visited = HashSet::new();
    let (ext_rules, ext_overrides) = collect_rules_from_extends(extends, base_dir, &mut visited);
    rules = ext_rules;
    extended_overrides = ext_overrides;
  }

  // 1b. Strip rules that extended configs disabled (severity == Off).
  //     These Off entries were kept in the map during recursive extends
  //     resolution so they could propagate upward and override rules
  //     enabled by earlier extends.
  rules.retain(|_, rc| rc.severity != Some(Severity::Off));

  // 2. Overlay user rules on top — user always wins.
  let raw_rules = raw.rules.unwrap_or_default();
  for (name, value) in &raw_rules {
    let resolved = value.resolve();
    // If the user set a rule to Off, remove it from the map entirely
    // so the linter doesn't run it at all.
    if resolved.severity == Some(Severity::Off) {
      rules.remove(name);
    } else {
      rules.insert(name.clone(), resolved);
    }
  }

  // Merge both `ignorePatterns` and `ignoreFiles` (Stylelint's field name).
  let mut ignore_patterns = raw.ignore_patterns.unwrap_or_default();
  if let Some(ignore_files) = raw.ignore_files {
    ignore_patterns.extend(ignore_files);
  }

  let formatter = raw.formatter;

  // 3. Resolve overrides.
  //    Extended configs' overrides come first, then the user's own overrides.
  // Combine: extended overrides first, then user overrides.
  let mut all_raw_overrides = extended_overrides;
  if let Some(user_overrides) = raw.overrides {
    all_raw_overrides.extend(user_overrides.into_iter().map(|entry| ExtendedOverride {
      entry,
      directory: base_dir.to_path_buf(),
    }));
  }

  let mut overrides: Vec<ResolvedOverride> = all_raw_overrides
    .into_iter()
    .flat_map(|ov| resolve_override(ov.entry, &ov.directory, 0))
    .collect();

  // 4. Extract plugin names from the config.
  let plugins = raw
    .plugins
    .as_ref()
    .map(extract_plugin_names)
    .unwrap_or_default();

  // 5. Parse the disable-report switches.  Each accepts `true`, `false`, or
  //    Stylelint's `[bool, { except, severity }]` array form, of which only
  //    the leading boolean is honoured.
  let report_needless_disables = disable_report_enabled(raw.report_needless_disables.as_ref());
  let report_invalid_scope_disables =
    disable_report_enabled(raw.report_invalid_scope_disables.as_ref());
  let report_descriptionless_disables =
    disable_report_enabled(raw.report_descriptionless_disables.as_ref());
  let report_unscoped_disables = disable_report_enabled(raw.report_unscoped_disables.as_ref());

  // 6. Parse defaultSeverity.
  let default_severity = raw.default_severity.as_deref().and_then(|s| match s {
    "warning" => Some(Severity::Warning),
    "error" => Some(Severity::Error),
    _ => None,
  });

  // Materialize the default once, after every inherited rule has retained
  // whether its severity was explicit. Overrides follow the same policy.
  for rule in rules
    .values_mut()
    .chain(overrides.iter_mut().flat_map(|ov| ov.rules.values_mut()))
  {
    rule
      .severity
      .get_or_insert(default_severity.unwrap_or(Severity::Error));
  }

  // 7. Extract top-level customSyntax as a string (if present).
  let custom_syntax = raw
    .custom_syntax
    .as_ref()
    .and_then(|v| v.as_str().map(String::from));

  // 8. The CLI-equivalent switches.
  let fix = raw.fix.as_ref().and_then(fix_mode_from_value);
  let cache_location = raw.cache_location.as_deref().map(PathBuf::from);

  GaleConfig {
    rules,
    ignore_patterns,
    formatter,
    overrides,
    config_dir: Some(base_dir.to_path_buf()),
    plugins,
    report_needless_disables,
    default_severity,
    custom_syntax,
    ignore_disables: raw.ignore_disables.unwrap_or(false),
    report_invalid_scope_disables,
    report_descriptionless_disables,
    report_unscoped_disables,
    allow_empty_input: raw.allow_empty_input.unwrap_or(false),
    quiet: raw.quiet.unwrap_or(false),
    fix,
    cache: raw.cache.unwrap_or(false),
    cache_location,
    cache_strategy: raw.cache_strategy,
  }
}

/// How deep overrides may nest through `extends` before gale stops
/// following them, as a guard against configs that extend each other.
const MAX_OVERRIDE_NESTING: usize = 8;

/// Resolve one `overrides` entry into the overrides gale applies, in order.
///
/// The override's `extends` supply rules under its own.  When an extended
/// config has `overrides` of its own (`stylelint-config-standard-vue`
/// extends `stylelint-config-recommended-vue`, whose Vue rules sit in an
/// override), Stylelint applies those too, to the files both match.  Such
/// an entry becomes three steps with Stylelint's precedence: the extended
/// rules, then the extended configs' overrides, then the entry's own rules.
fn resolve_override(ov: ConfigOverride, base_dir: &Path, depth: usize) -> Vec<ResolvedOverride> {
  let file_patterns = ov.files.unwrap_or_default();
  if file_patterns.is_empty() {
    return Vec::new();
  }
  let ignore_patterns = ov.ignore_files.unwrap_or_default();
  let custom_syntax = ov
    .custom_syntax
    .as_ref()
    .and_then(|v| v.as_str().map(String::from));

  // Rules from the override's extends, and the overrides they bring.
  let (mut ov_rules, nested) = match ov.extends {
    Some(ref extends) => collect_rules_from_extends(extends, base_dir, &mut HashSet::new()),
    None => (HashMap::new(), Vec::new()),
  };
  // The override's own rules.  Off entries are kept so they can remove
  // base rules when applied per-file.
  let own_rules: HashMap<String, RuleConfig> = ov
    .rules
    .unwrap_or_default()
    .into_iter()
    .map(|(name, value)| (name, value.resolve()))
    .collect();

  if nested.is_empty() || depth >= MAX_OVERRIDE_NESTING {
    ov_rules.extend(own_rules);
    return vec![ResolvedOverride::new(
      file_patterns,
      ignore_patterns,
      ov_rules,
      custom_syntax,
    )];
  }

  let outer = ResolvedOverride::new(
    file_patterns.clone(),
    ignore_patterns.clone(),
    ov_rules,
    custom_syntax,
  );
  let mut resolved = vec![outer.clone()];
  for inner in nested {
    resolved.extend(
      resolve_override(inner.entry, &inner.directory, depth + 1)
        .into_iter()
        .map(|r| r.within(&outer)),
    );
  }
  resolved.push(ResolvedOverride::new(
    file_patterns,
    ignore_patterns,
    own_rules,
    None,
  ));
  resolved
}

/// Interpret one of Stylelint's `report*Disables` settings.
///
/// `true` / `false` are the common forms.  The array form
/// `[bool, { except, severity }]` is read for its leading boolean; any other
/// truthy value enables the report.
fn disable_report_enabled(value: Option<&serde_json::Value>) -> bool {
  match value {
    None => false,
    Some(serde_json::Value::Bool(b)) => *b,
    Some(serde_json::Value::Array(items)) => {
      items.first().and_then(|v| v.as_bool()).unwrap_or(true)
    }
    Some(serde_json::Value::Null) => false,
    Some(_) => true,
  }
}

/// Interpret Stylelint's `fix` config value.
///
/// `true` and `"strict"` enable strict fixing, `"lax"` enables lax fixing,
/// and anything else leaves fixing off.
fn fix_mode_from_value(value: &serde_json::Value) -> Option<FixMode> {
  match value {
    serde_json::Value::Bool(true) => Some(FixMode::Strict),
    serde_json::Value::String(s) => match s.to_lowercase().as_str() {
      "strict" => Some(FixMode::Strict),
      "lax" => Some(FixMode::Lax),
      _ => None,
    },
    _ => None,
  }
}

#[cfg(test)]
mod tests;
