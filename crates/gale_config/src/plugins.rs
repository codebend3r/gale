//! The Stylelint plugins gale implements as built-in rules, so configs that
//! load them are accepted.

/// Known Stylelint plugins whose rules Gale implements as built-in rules.
/// These plugins are silently accepted when found in the `plugins` config field.
const KNOWN_PLUGINS: &[&str] = &[
  "stylelint-scss",
  "stylelint-order",
  "@stylistic/stylelint-plugin",
  "stylelint-no-unsupported-browser-features",
  "stylelint-declaration-block-no-ignored-properties",
  "stylelint-value-no-unknown-custom-properties",
];

/// Rule prefixes associated with known plugins.  Used to determine whether an
/// unrecognised rule belongs to a known plugin (and thus should produce a
/// "not yet supported" warning rather than being silently dropped).
const KNOWN_PLUGIN_RULE_PREFIXES: &[&str] =
  &["scss/", "order/", "@stylistic/", "stylistic/", "csstools/"];

/// Standalone rule names from known plugins (no prefix).
const KNOWN_PLUGIN_STANDALONE_RULES: &[&str] = &[
  "plugin/no-unsupported-browser-features",
  "plugin/declaration-block-no-ignored-properties",
];

/// Extract plugin name strings from the raw `plugins` JSON value.
///
/// Stylelint's `plugins` field can be a single string or an array of strings.
/// Some entries may be paths (e.g. `./my-plugin.js`) — we normalise by
/// extracting the last path component without the extension when it looks
/// like a file path.
pub(crate) fn extract_plugin_names(value: &serde_json::Value) -> Vec<String> {
  let mut names = Vec::new();
  match value {
    serde_json::Value::String(s) => {
      names.push(s.clone());
    }
    serde_json::Value::Array(arr) => {
      for item in arr {
        if let serde_json::Value::String(s) = item {
          names.push(s.clone());
        }
      }
    }
    _ => {}
  }
  names
}

/// Check whether a plugin name matches one of the known plugins.
///
/// Uses substring matching so that `./node_modules/stylelint-scss` or
/// `stylelint-scss/lib/index.js` still matches `stylelint-scss`.
pub fn is_known_plugin(plugin: &str) -> bool {
  KNOWN_PLUGINS.iter().any(|known| plugin.contains(known))
}

/// Check whether a rule name looks like it belongs to a known plugin.
pub fn is_known_plugin_rule(rule_name: &str) -> bool {
  KNOWN_PLUGIN_RULE_PREFIXES
    .iter()
    .any(|prefix| rule_name.starts_with(prefix))
    || KNOWN_PLUGIN_STANDALONE_RULES.contains(&rule_name)
}
