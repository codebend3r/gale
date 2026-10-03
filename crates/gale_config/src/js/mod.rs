//! Reading JavaScript configs (`stylelint.config.js` and friends) without
//! running them.
//!
//! Gale never executes JavaScript.  It finds the exported object literal,
//! inlines what relative imports and simple constants it can
//! ([`imports`]), and rewrites the literal into JSON ([`to_json`]) for serde
//! to read as a [`ConfigFile`].

mod imports;
mod to_json;

use std::path::Path;

use imports::{
  extract_reexport_require, find_variable_value, resolve_import_path, resolve_js_imports,
  strip_import_lines, substitute_scalar_vars,
};
use to_json::js_object_to_json;

use crate::{ConfigError, ConfigFile};

/// Parse a JavaScript config file that uses `module.exports = { ... }`,
/// `exports = { ... }`, or `export default { ... }` with a static object literal.
///
/// This does NOT execute JavaScript — it extracts the object literal and converts
/// JS object syntax into valid JSON before deserializing.
///
/// When `file_dir` is provided, relative imports (`./` or `../`) are resolved
/// by reading the imported file and inlining the exported value.
pub(crate) fn parse_js_config(
  source: &str,
  file_dir: Option<&Path>,
) -> Result<ConfigFile, ConfigError> {
  // Handle re-export pattern: `module.exports = require("./relative")`
  // This is common in npm config packages where index.js delegates to another file.
  if let Some(file_dir) = file_dir
    && let Some(reexport_path) = extract_reexport_require(source)
    && (reexport_path.starts_with("./") || reexport_path.starts_with("../"))
    && let Some(resolved) = resolve_import_path(file_dir, &reexport_path)
    && let Ok(contents) = std::fs::read_to_string(&resolved)
  {
    return parse_js_config(&contents, resolved.parent());
  }

  // Pre-process: resolve relative imports and inline their exported values.
  let source = resolve_js_imports(source, file_dir);
  // Strip remaining import/require lines that would confuse the JSON converter.
  let source = strip_import_lines(&source);
  // Substitute scalar const/let/var variables (e.g. `const OFF = null;`)
  // so that bare identifiers in the exported object get replaced with their
  // literal values before the JS→JSON conversion.
  let source = substitute_scalar_vars(&source);

  // Find the start of the exported object literal.
  let markers = ["module.exports", "exports", "export default"];

  let mut obj_start: Option<usize> = None;

  for marker in &markers {
    if let Some(pos) = source.find(marker) {
      // Find the `=` after the marker (for module.exports / exports), or
      // no `=` for `export default`.
      let after_marker = pos + marker.len();
      let rest = &source[after_marker..];
      let brace_search = if *marker == "export default" {
        rest
      } else {
        // Skip past the `=`
        if let Some(eq_pos) = rest.find('=') {
          &rest[eq_pos + 1..]
        } else {
          continue;
        }
      };

      let trimmed = brace_search.trim_start();

      // Check if the export is a variable reference (e.g. `export default config`)
      // rather than a direct object literal.
      if !trimmed.starts_with('{')
        && trimmed
          .chars()
          .next()
          .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
      {
        let ident_end = trimmed
          .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
          .unwrap_or(trimmed.len());
        let var_name = &trimmed[..ident_end];
        // Look for `const <var_name> = {` in the source
        if let Some(value) = find_variable_value(&source, var_name)
          && value.starts_with('{')
        {
          // Replace the variable reference with the inlined object
          let source = format!(
            "{}{}{}",
            &source[..pos],
            if *marker == "export default" {
              "export default "
            } else {
              "module.exports = "
            },
            value
          );
          let new_marker_pos = pos;
          let new_after = new_marker_pos
            + if *marker == "export default" {
              "export default ".len()
            } else {
              "module.exports = ".len()
            };
          if let Some(brace_offset) = source[new_after..].find('{') {
            // We need to use the modified source for extraction
            let extracted =
              extract_braced_object(&source[new_after + brace_offset..]).ok_or_else(|| {
                ConfigError::UnsupportedFormat(
                  "failed to extract object literal from JS config".to_string(),
                )
              })?;
            let json = js_object_to_json(&extracted);
            return serde_json::from_str::<ConfigFile>(&json).map_err(ConfigError::from);
          }
        }
      }

      // Find the opening `{`
      if let Some(brace_offset) = brace_search.find('{') {
        let absolute_offset = source.len() - brace_search.len() + brace_offset;
        obj_start = Some(absolute_offset);
        break;
      }
    }
  }

  let start = obj_start.ok_or_else(|| {
    ConfigError::UnsupportedFormat(
      "no module.exports/export default object found in JS config".to_string(),
    )
  })?;

  // Extract from opening `{` to the matching `}`, respecting strings and comments.
  let extracted = extract_braced_object(&source[start..]).ok_or_else(|| {
    ConfigError::UnsupportedFormat("failed to extract object literal from JS config".to_string())
  })?;

  // Convert JS object syntax to JSON.
  let json = js_object_to_json(&extracted);

  serde_json::from_str::<ConfigFile>(&json).map_err(ConfigError::from)
}

/// Extract a brace-balanced substring starting from the opening `{`.
/// Returns the substring including the outer braces.
fn extract_braced_object(s: &str) -> Option<String> {
  let mut depth = 0i32;
  let mut result = String::new();
  let mut chars = s.chars().peekable();
  let mut in_single_quote = false;
  let mut in_double_quote = false;
  let mut in_template = false;
  let mut escape_next = false;

  while let Some(c) = chars.next() {
    if escape_next {
      result.push(c);
      escape_next = false;
      continue;
    }

    if c == '\\' && (in_single_quote || in_double_quote || in_template) {
      result.push(c);
      escape_next = true;
      continue;
    }

    // String handling
    if !in_double_quote && !in_template && c == '\'' {
      in_single_quote = !in_single_quote;
      result.push(c);
      continue;
    }
    if !in_single_quote && !in_template && c == '"' {
      in_double_quote = !in_double_quote;
      result.push(c);
      continue;
    }
    if !in_single_quote && !in_double_quote && c == '`' {
      in_template = !in_template;
      result.push(c);
      continue;
    }

    if in_single_quote || in_double_quote || in_template {
      result.push(c);
      continue;
    }

    // Line comments
    if c == '/' {
      if chars.peek() == Some(&'/') {
        // Skip until end of line
        for nc in chars.by_ref() {
          if nc == '\n' {
            break;
          }
        }
        result.push(' '); // replace comment with space
        continue;
      }
      if chars.peek() == Some(&'*') {
        // Block comment — skip until */
        chars.next(); // consume *
        loop {
          match chars.next() {
            Some('*') if chars.peek() == Some(&'/') => {
              chars.next(); // consume /
              break;
            }
            None => break,
            _ => {}
          }
        }
        result.push(' ');
        continue;
      }
    }

    if c == '{' {
      depth += 1;
    } else if c == '}' {
      depth -= 1;
    }

    result.push(c);

    if depth == 0 {
      return Some(result);
    }
  }

  None // unbalanced braces
}

#[cfg(test)]
mod tests {
  use super::to_json::js_string_body_to_json;
  use super::*;
  use crate::RuleConfigValue;
  use crate::raw::UNEVALUATED_EXTENDS;

  #[test]
  fn js_config_module_exports_single_quoted_keys() {
    let js = r#"
'use strict';

module.exports = {
  'extends': ['stylelint-config-standard'],
  'rules': {
    'alpha-value-notation': null,
    'color-named': 'never',
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("color-named"));
  }

  #[test]
  fn js_config_trailing_commas() {
    let js = r#"
module.exports = {
  "rules": {
    "block-no-empty": true,
    "color-no-invalid-hex": true,
  },
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 2);
    assert!(rules.contains_key("block-no-empty"));
    assert!(rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn js_config_null_values() {
    let js = r#"
module.exports = {
  'rules': {
    'alpha-value-notation': null,
    'declaration-no-important': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 2);
  }

  #[test]
  fn js_config_array_values() {
    let js = r#"
module.exports = {
  'rules': {
    'font-weight-notation': ['numeric', { 'ignore': ['relative'] }],
    'selector-max-id': 0,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("font-weight-notation"));
    assert!(rules.contains_key("selector-max-id"));
  }

  #[test]
  fn js_config_nested_objects() {
    let js = r#"
module.exports = {
  'rules': {
    'font-weight-notation': ['numeric', { 'ignore': ['relative'] }],
    'color-named': 'never',
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 2);
  }

  #[test]
  fn js_config_export_default() {
    let js = r#"
export default {
  "extends": ["stylelint-config-standard"],
  "rules": {
    "block-no-empty": true
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));
  }

  #[test]
  fn js_config_unquoted_keys() {
    let js = r#"
module.exports = {
  extends: ['stylelint-config-standard'],
  rules: {
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
  }

  #[test]
  fn js_config_with_comments() {
    let js = r#"
// Main config
module.exports = {
  /* extends from standard */
  'extends': ['stylelint-config-standard'],
  'rules': {
    // Disable this rule
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert!(raw.extends.is_some());
  }

  #[test]
  fn js_config_with_spread_skipped() {
    let js = r#"
module.exports = {
  ...baseConfig,
  'rules': {
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));
  }

  #[test]
  fn js_config_spreads_of_members_calls_and_expressions_are_skipped() {
    // The shapes stylelint-config-html and stylelint-config-recommended-vue
    // use: spreads gale cannot evaluate statically must not break the rest.
    let js = r#"
const config = {
  overrides: [...html.overrides, ...vue?.overrides],
  rules: {
    'block-no-empty': true,
    ...(semver.gte(version, "16.13.0")
      ? { 'color-named': [true, { ignore: ["inside-function"] }] }
      : {}),
    'color-no-invalid-hex': true,
  },
};
export default config;
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));
    assert!(rules.contains_key("color-no-invalid-hex"));
    assert!(!rules.contains_key("color-named"));
    assert_eq!(raw.overrides.unwrap().len(), 0);
  }

  #[test]
  fn js_config_no_exports_returns_error() {
    let js = r#"
const config = {
  rules: { 'block-no-empty': true }
};
"#;
    assert!(parse_js_config(js, None).is_err());
  }

  #[test]
  fn js_config_full_realistic_example() {
    let js = r#"
'use strict';

module.exports = {
  'extends': ['stylelint-config-standard'],
  'rules': {
    'alpha-value-notation': null,
    'color-named': 'never',
    'declaration-no-important': true,
    'font-weight-notation': ['numeric', { 'ignore': ['relative'] }],
    'selector-max-id': 0,
  }
};
"#;
    let raw = parse_js_config(js, None).unwrap();
    // Verify parsing succeeded and extracted the right structure.
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
    let rules = raw.rules.unwrap();
    assert_eq!(rules.len(), 5);
    assert!(rules.contains_key("alpha-value-notation"));
    assert!(rules.contains_key("color-named"));
    assert!(rules.contains_key("declaration-no-important"));
    assert!(rules.contains_key("font-weight-notation"));
    assert!(rules.contains_key("selector-max-id"));
  }

  #[test]
  fn js_config_with_esm_import_resolution() {
    // Create a temporary directory with a mock file structure
    let tmp = std::env::temp_dir().join("gale_test_esm_import");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // Write the imported file: groups.js
    std::fs::write(
      tmp.join("groups.js"),
      r#"
const propertyGroups = [
    { "properties": ["position", "top", "right"] },
    { "properties": ["display", "flex"] }
]

export default propertyGroups
"#,
    )
    .unwrap();

    // Write the main config file (uses the common pattern:
    // const config = {...}; export default config)
    let config_src = r#"
import propertyGroups from './groups.js'

const config = {
    plugins: ['stylelint-order'],
    rules: {
        'order/properties-order': propertyGroups,
    },
}

export default config
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("order/properties-order"));

    // The value should be an array with the property groups
    let val = rules.get("order/properties-order").unwrap();
    match val {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
        // Verify first group has the right properties
        assert!(arr[0].is_object());
      }
      _ => panic!(
        "Expected array value for order/properties-order, got {:?}",
        val
      ),
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_with_esm_import_with_comments() {
    // Simulates the real stylelint-config-recess-order pattern
    // where groups.js has JS comments inside the array.
    let tmp = std::env::temp_dir().join("gale_test_esm_import_comments");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    std::fs::write(
      tmp.join("groups.js"),
      r#"
/**
 * @typedef {Object} Group
 * @property {Array<string>} properties
 */

/** @type {Group[]} */
const propertyGroups = [
    {
        /**
         * Compose rules from other selectors in CSS Modules.
         * @see https://github.com/css-modules/css-modules#composition
         */
        properties: ['composes'],
    },
    {
        // Must be first (unless using the above).
        properties: ['all'],
    },
    {
        // Position.
        properties: [
            'position',
            'top',
            'right',
            'bottom',
            'left',
        ],
    },
    {
        // Display.
        properties: ['display', 'flex'],
    },
]

export default propertyGroups
"#,
    )
    .unwrap();

    let config_src = r#"
import propertyGroups from './groups.js'

const config = {
    plugins: ['stylelint-order'],
    rules: {
        'order/properties-order': propertyGroups,
    },
}

export default config
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(
      rules.contains_key("order/properties-order"),
      "should have order/properties-order rule"
    );

    let val = rules.get("order/properties-order").unwrap();
    match val {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 4, "should have 4 property groups");
        assert!(arr[0].is_object());
      }
      _ => panic!(
        "Expected array value for order/properties-order, got {:?}",
        val
      ),
    }

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_recess_order_real_files() {
    // Test with the actual recess-order files if available.
    let recess_dir = std::path::Path::new(
      "../../benchmarks/.repos/bootstrap/node_modules/stylelint-config-recess-order",
    );
    if !recess_dir.exists() {
      // Skip if the benchmark repo isn't set up.
      return;
    }

    let index_path = recess_dir.join("index.js");
    let contents = std::fs::read_to_string(&index_path).unwrap();
    let result = parse_js_config(&contents, Some(recess_dir));
    match &result {
      Ok(config) => {
        let rules = config.rules.as_ref().expect("should have rules");
        assert!(
          rules.contains_key("order/properties-order"),
          "recess-order config should have order/properties-order"
        );
        let val = rules.get("order/properties-order").unwrap();
        match val {
          RuleConfigValue::Array(arr) => {
            assert!(
              arr.len() > 5,
              "should have many property groups, got {}",
              arr.len()
            );
          }
          other => panic!(
            "Expected Array for order/properties-order, got {:?}",
            std::mem::discriminant(other)
          ),
        }
      }
      Err(e) => {
        panic!("Failed to parse recess-order config: {e}");
      }
    }
  }

  #[test]
  fn js_config_with_cjs_import_resolution() {
    let tmp = std::env::temp_dir().join("gale_test_cjs_import");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // Write imported file
    std::fs::write(
      tmp.join("groups.js"),
      r#"
module.exports = [
    { "properties": ["color", "background"] }
]
"#,
    )
    .unwrap();

    let config_src = r#"
const groups = require('./groups')

module.exports = {
    rules: {
        'order/properties-order': groups,
    },
}
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("order/properties-order"));

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_import_missing_file_graceful() {
    // When the imported file doesn't exist, parsing should still work
    // (the variable just won't be replaced, and parsing continues)
    let config_src = r#"
import stuff from './nonexistent.js'

export default {
    rules: {
        'block-no-empty': true,
    },
}
"#;
    let tmp = std::env::temp_dir().join("gale_test_missing_import");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_require_resolve_extends_names_the_package() {
    let config_src = r#"
module.exports = {
  extends: require.resolve( '@wordpress/stylelint-tools/config' ),
  plugins: [ require('stylelint-scss') ],
  rules: { 'block-no-empty': true },
};
"#;
    let raw = parse_js_config(config_src, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["@wordpress/stylelint-tools/config".to_string()])
    );
    assert_eq!(raw.plugins, Some(serde_json::json!(["stylelint-scss"])));
  }

  #[test]
  fn extends_entries_that_are_not_strings_are_kept_as_unresolvable() {
    let config_src = r#"
module.exports = {
  extends: [ 'stylelint-config-standard', path.resolve( __dirname, 'base.js' ) ],
};
"#;
    let raw = parse_js_config(config_src, None).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec![
        "stylelint-config-standard".to_string(),
        UNEVALUATED_EXTENDS.to_string()
      ])
    );

    let json: ConfigFile = serde_json::from_str(r#"{ "extends": null }"#).unwrap();
    assert_eq!(json.extends, None);
  }

  #[test]
  fn js_config_with_multibyte_text_before_bare_identifiers_parses() {
    // Bare identifiers are turned into `null` and `true`/`false` kept; the
    // identifier used to be read back as a byte range from character
    // indices, so multibyte text earlier in the config made `true` read as
    // garbage (and the rule silently `null`), or split a character.
    let config_src = r#"
module.exports = {
  rules: {
    'comment-pattern': [ '^[A-Z]', { message: 'Commentaires en français — “s’il vous plaît”' } ],
    'block-no-empty': true,
    'custom-property-pattern': somePattern,
  },
};
"#;
    let raw = parse_js_config(config_src, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("comment-pattern"));
    assert_eq!(
      rules.get("block-no-empty"),
      Some(&RuleConfigValue::Bool(true))
    );
    assert_eq!(
      rules.get("custom-property-pattern"),
      Some(&RuleConfigValue::Null(None))
    );
  }

  #[test]
  fn js_config_npm_import_not_resolved() {
    // npm package imports should not be resolved (no relative path).
    // The import line is stripped, and string plugin names work fine.
    let config_src = r#"
import something from 'stylelint-order'

export default {
    plugins: ['stylelint-order'],
    rules: {
        'block-no-empty': true,
    },
}
"#;
    let raw = parse_js_config(config_src, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));
  }

  #[test]
  fn js_config_import_with_extension_resolution() {
    // Test that we try adding .js extension when path has no extension
    let tmp = std::env::temp_dir().join("gale_test_ext_resolution");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    std::fs::write(tmp.join("data.js"), "export default [\"a\", \"b\"]\n").unwrap();

    let config_src = r#"
import items from './data'

export default {
    rules: {
        'my-rule': items,
    },
}
"#;

    let raw = parse_js_config(config_src, Some(tmp.as_path())).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("my-rule"));
    match rules.get("my-rule").unwrap() {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
      }
      other => panic!("Expected array, got {:?}", other),
    }

    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn js_config_regexp_literals() {
    let js = r#"
module.exports = {
    rules: {
        'selector-class-pattern': [/^[a-z]([a-z0-9-_!])*$/, {
            resolveNestedSelectors: true
        }],
        'selector-id-pattern': /^[a-z]([a-z0-9-])*$/i,
    }
}
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    match rules.get("selector-class-pattern").unwrap() {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], serde_json::json!("^[a-z]([a-z0-9-_!])*$"));
      }
      other => panic!("Expected array, got {:?}", other),
    }
    match rules.get("selector-id-pattern").unwrap() {
      RuleConfigValue::Severity(s) => {
        assert_eq!(s, "^[a-z]([a-z0-9-])*$");
      }
      other => panic!("Expected string, got {:?}", other),
    }
  }

  #[test]
  fn js_config_string_concatenation() {
    let js = r#"
module.exports = {
    rules: {
        'selector-class-pattern': ['pattern', {
            message: 'Class names may only contain [a-z0-9-_!] characters and ' +
                'must start with [a-z]'
        }]
    }
}
"#;
    let raw = parse_js_config(js, None).unwrap();
    let rules = raw.rules.unwrap();
    match rules.get("selector-class-pattern").unwrap() {
      RuleConfigValue::Array(arr) => {
        assert_eq!(arr.len(), 2);
        let opts = arr[1].as_object().unwrap();
        assert_eq!(
          opts.get("message").unwrap().as_str().unwrap(),
          "Class names may only contain [a-z0-9-_!] characters and must start with [a-z]"
        );
      }
      other => panic!("Expected array, got {:?}", other),
    }
  }

  /// The secondary option `key` of rule `rule` in a parsed JS config.
  fn secondary_string(raw: &ConfigFile, rule: &str, key: &str) -> String {
    match raw.rules.as_ref().unwrap().get(rule).unwrap() {
      RuleConfigValue::Array(arr) => arr[1]
        .as_object()
        .unwrap()
        .get(key)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string(),
      other => panic!("Expected array, got {other:?}"),
    }
  }

  #[test]
  fn js_config_multiline_template_literal() {
    // docusaurus's `.stylelintrc.js`: a copyright header spanning lines.
    let js = "module.exports = {\n  rules: {\n    'docusaurus/copyright-header': [\n      true,\n      {\n        header: `*\n * Copyright (c) Facebook, Inc. and its affiliates.\n *\n * This source code is licensed under the MIT license.`,\n      },\n    ],\n  },\n};\n";
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      secondary_string(&raw, "docusaurus/copyright-header", "header"),
      "*\n * Copyright (c) Facebook, Inc. and its affiliates.\n *\n * This source code is licensed under the MIT license."
    );
  }

  #[test]
  fn js_config_message_function_with_escaped_backticks() {
    // govuk-frontend's `stylelint.config.js`.
    let js = r#"
module.exports = {
  rules: {
    'custom-property-pattern': [
      '^_?([a-z][a-z0-9]*)(-[a-z0-9]+)*$',
      {
        message: (name) =>
          `Expected custom property name "${name}" to be kebab-case, possibly starting with an \`_\` if it is private`
      }
    ]
  }
}
"#;
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(
      secondary_string(&raw, "custom-property-pattern", "message"),
      "Expected custom property name \"${name}\" to be kebab-case, possibly starting with an `_` if it is private"
    );
  }

  #[test]
  fn js_string_escapes_become_json_escapes() {
    assert_eq!(js_string_body_to_json(r"it\'s"), "it's");
    assert_eq!(js_string_body_to_json(r#"say "hi""#), r#"say \"hi\""#);
    assert_eq!(
      js_string_body_to_json(r"\x41\u0042\u{43}\v"),
      r"\u0041\u0042C\u000b"
    );
    assert_eq!(js_string_body_to_json("a\\\nb\tc"), r"ab\tc");
    assert_eq!(js_string_body_to_json(r"\d\$\\"), r"d$\\");
    let js = "module.exports = { rules: { 'a': ['x', { message: \"it\\'s \\x41\" }] } }";
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(secondary_string(&raw, "a", "message"), "it's A");
  }
}
