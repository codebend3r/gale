//! Reading JavaScript configs: `stylelint.config.js` and friends, and the
//! JavaScript files they extend.
//!
//! Stylelint runs these as modules, and so does gale: [`node`] imports the
//! file under Node.js and reads back the JSON it prints.  Without Node.js,
//! [`fallback`] reads the file without running it, which covers a plain
//! exported object, and gale warns about each part it had to skip.

mod fallback;
mod node;
mod scan;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use serde_json::Value;

use crate::{ConfigError, ConfigFile};

/// What each JavaScript file evaluated to this run, keyed by its path and
/// modification time.  The same config is loaded more than once (the CLI's
/// own lookup, then per directory, and as a shared `extends`), and each
/// load would otherwise start Node again and repeat its warnings; the time
/// keeps a long-running language server from holding on to an old version.
type Loaded = HashMap<(PathBuf, Option<SystemTime>), Result<Value, String>>;
static LOADED: LazyLock<Mutex<Loaded>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Load the JavaScript config at `path`.
///
/// Every JavaScript file the config machinery reads, whether the config
/// itself or a preset it extends, comes through here.
pub(crate) fn load_js_config(path: &Path) -> Result<ConfigFile, ConfigError> {
  let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
  let key = (
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
    modified,
  );
  let cached = LOADED
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .get(&key)
    .cloned();
  let value = match cached {
    Some(value) => value,
    None => {
      let value = evaluate(path).map_err(|err| err.to_string());
      LOADED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, value.clone());
      value
    }
  };
  Ok(serde_json::from_value(
    value.map_err(ConfigError::JavaScript)?,
  )?)
}

/// Evaluate the file at `path` under Node.js, or read it statically when
/// Node.js is not available, warning about whatever had to be left out.
fn evaluate(path: &Path) -> Result<Value, ConfigError> {
  let cwd = std::env::current_dir().unwrap_or_default();
  let shown = path.strip_prefix(&cwd).unwrap_or(path).display();
  let value = match node::evaluate(path) {
    Some(Ok(evaluated)) => {
      for dropped in &evaluated.dropped {
        eprintln!(
          "warning: {shown}: dropped {dropped}, since gale reads the config as JSON, \
           which cannot hold it"
        );
      }
      Some(evaluated.config)
    }
    Some(Err(message)) => {
      return Err(ConfigError::JavaScript(format!(
        "Node.js could not load it: {message}"
      )));
    }
    None => {
      let reading = fallback::read(path)?;
      for skipped in &reading.skipped {
        eprintln!(
          "warning: {shown}: Node.js was not found, so gale read this config without \
           running it and skipped: {skipped}"
        );
      }
      reading.value
    }
  };
  match value {
    Some(value) if value.is_object() => Ok(value),
    Some(_) => Err(ConfigError::JavaScript(
      "it does not export an object".to_string(),
    )),
    None => Err(ConfigError::JavaScript(
      "Node.js was not found, and gale could not read the export without running it".to_string(),
    )),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::RuleConfigValue;
  use crate::raw::UNEVALUATED_EXTENDS;

  /// `source` read by the static fallback, as a config.
  fn parse_js_config(source: &str, dir: Option<&Path>) -> Result<ConfigFile, ConfigError> {
    let reading = fallback::read_source(source, dir, 0)?;
    Ok(serde_json::from_value(reading.value.unwrap_or_default())?)
  }

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
    // Token offsets are byte offsets, so multibyte text earlier in the
    // config cannot shift what `true` reads as, or split a character.  A
    // name the reader cannot resolve leaves its key out.
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
    assert!(!rules.contains_key("custom-property-pattern"));
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
        assert_eq!(arr[0], serde_json::json!("/^[a-z]([a-z0-9-_!])*$/"));
      }
      other => panic!("Expected array, got {:?}", other),
    }
    match rules.get("selector-id-pattern").unwrap() {
      RuleConfigValue::Severity(s) => {
        assert_eq!(s, "/^[a-z]([a-z0-9-])*$/i");
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
  fn js_string_escapes_are_decoded() {
    let js = "module.exports = { rules: { 'a': ['x', { message: \"it\\'s \\x41\" }] } }";
    let raw = parse_js_config(js, None).unwrap();
    assert_eq!(secondary_string(&raw, "a", "message"), "it's A");
  }
}
