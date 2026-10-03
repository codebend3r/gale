//! Finding the config file that applies to a directory or file, and reading
//! it in whichever format it is written.

use std::path::{Path, PathBuf};

use crate::js::parse_js_config;
use crate::resolve::resolve_raw;
use crate::{ConfigError, ConfigFile, GaleConfig};

/// Well-known config file names in priority order.
const CONFIG_FILENAMES: &[&str] = &[
  "gale.json",
  "gale.toml",
  ".stylelintrc",
  ".stylelintrc.json",
  ".stylelintrc.yml",
  ".stylelintrc.yaml",
  "stylelint.config.js",
  "stylelint.config.mjs",
  "stylelint.config.cjs",
  ".stylelintrc.js",
  ".stylelintrc.cjs",
  ".stylelintrc.mjs",
];

/// Walk upward from `start_dir` looking for the first config file.
///
/// After checking all well-known config filenames, also checks for a
/// `"stylelint"` field inside `package.json` (lowest priority, matching
/// Stylelint's cosmiconfig-based resolution).
pub fn find_config(start_dir: &Path) -> Option<PathBuf> {
  let mut dir = start_dir.to_path_buf();
  loop {
    if let Some(found) = config_in_dir(&dir) {
      return Some(found);
    }
    if !dir.pop() {
      return None;
    }
  }
}

/// The config file `dir` itself provides, without looking at its ancestors:
/// the first of [`CONFIG_FILENAMES`] present, else a `package.json` with a
/// `"stylelint"` field.
pub(crate) fn config_in_dir(dir: &Path) -> Option<PathBuf> {
  for name in CONFIG_FILENAMES {
    let candidate = dir.join(name);
    if candidate.is_file() {
      return Some(candidate);
    }
  }
  // Lowest priority: check for a `"stylelint"` field in package.json.
  let pkg_path = dir.join("package.json");
  if pkg_path.is_file()
    && let Ok(content) = std::fs::read_to_string(&pkg_path)
    && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
    && pkg.get("stylelint").is_some()
  {
    return Some(pkg_path);
  }
  None
}

/// Load and parse a config file at the given path.
///
/// When `path` points to a `package.json`, the `"stylelint"` field is
/// extracted and used as the config.  If the field is a string it is
/// treated as `{ "extends": "<that-string>" }`.
pub fn load_config(path: &Path) -> Result<GaleConfig, ConfigError> {
  let contents = std::fs::read_to_string(path)?;
  let file_name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or_default();

  let base_dir = path.parent().unwrap_or(Path::new("."));

  let raw = if file_name == "package.json" {
    parse_package_json_stylelint(&contents)?
  } else {
    parse_config_file(file_name, &contents, Some(base_dir))?
  };
  Ok(resolve_raw(raw, base_dir))
}

/// Extract and parse the `"stylelint"` field from the contents of a
/// `package.json` file.
///
/// - If the field is an object it is deserialized as a [`ConfigFile`].
/// - If the field is a string it is treated as `{ "extends": "<value>" }`.
/// - If the field is missing an error is returned.
fn parse_package_json_stylelint(contents: &str) -> Result<ConfigFile, ConfigError> {
  let pkg: serde_json::Value = serde_json::from_str(contents)?;
  match pkg.get("stylelint") {
    Some(value) if value.is_object() => Ok(serde_json::from_value::<ConfigFile>(value.clone())?),
    Some(value) if value.is_string() => {
      let extends_str = value.as_str().unwrap().to_string();
      Ok(ConfigFile {
        extends: Some(vec![extends_str]),
        ..Default::default()
      })
    }
    Some(_) => Err(ConfigError::UnsupportedFormat(
      "package.json \"stylelint\" field must be an object or a string".to_string(),
    )),
    None => Err(ConfigError::UnsupportedFormat(
      "package.json has no \"stylelint\" field".to_string(),
    )),
  }
}

/// Parses config contents using the format implied by the file name.
/// `.stylelintrc` has no extension, so it is tried as JSON then YAML.
fn parse_config_file(
  file_name: &str,
  contents: &str,
  file_dir: Option<&Path>,
) -> Result<ConfigFile, ConfigError> {
  if file_name.ends_with(".json") || file_name == "gale.json" {
    Ok(serde_json::from_str(contents)?)
  } else if file_name.ends_with(".toml") || file_name == "gale.toml" {
    Ok(toml::from_str(contents)?)
  } else if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
    Ok(serde_yaml::from_str(contents)?)
  } else if file_name.ends_with(".js") || file_name.ends_with(".mjs") || file_name.ends_with(".cjs")
  {
    parse_js_config(contents, file_dir)
  } else if file_name == ".stylelintrc" {
    // Try JSON first, fall back to YAML.
    serde_json::from_str(contents)
      .map_err(ConfigError::from)
      .or_else(|_| serde_yaml::from_str(contents).map_err(ConfigError::from))
  } else {
    Err(ConfigError::UnsupportedFormat(file_name.to_string()))
  }
}

/// Read a file and parse it as a [`ConfigFile`], inferring format from the file name.
pub(crate) fn load_config_file(path: &Path) -> Result<ConfigFile, ConfigError> {
  let contents = std::fs::read_to_string(path)?;
  let file_name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or_default();
  let file_dir = path.parent();

  // Handle package.json specially: extract the "stylelint" field.
  if file_name == "package.json" {
    return parse_package_json_stylelint(&contents);
  }

  // For files without a recognised extension (e.g. bare `.stylelintrc`),
  // or JSON files, try JSON first then YAML as a fallback.
  if file_name.ends_with(".json") {
    Ok(serde_json::from_str(&contents)?)
  } else if file_name.ends_with(".toml") {
    Ok(toml::from_str(&contents)?)
  } else if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
    Ok(serde_yaml::from_str(&contents)?)
  } else if file_name.ends_with(".js") || file_name.ends_with(".mjs") || file_name.ends_with(".cjs")
  {
    parse_js_config(&contents, file_dir)
  } else {
    // Unknown extension — try JSON, then YAML.
    serde_json::from_str(&contents)
      .map_err(ConfigError::from)
      .or_else(|_| serde_yaml::from_str(&contents).map_err(ConfigError::from))
  }
}

/// Find the config file that applies to a specific file path.
///
/// Walks up from the file's parent directory looking for the nearest config
/// file — matching Stylelint's cosmiconfig-based per-file resolution.
pub fn find_config_for_file(file_path: &Path) -> Option<PathBuf> {
  let dir = if file_path.is_dir() {
    file_path.to_path_buf()
  } else {
    file_path.parent()?.to_path_buf()
  };
  find_config(&dir)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn js_config_parse_config_file_dispatches() {
    let js = r#"
module.exports = {
  'rules': {
    'block-no-empty': true,
  }
};
"#;
    let raw = parse_config_file("stylelint.config.js", js, None).unwrap();
    let rules = raw.rules.unwrap();
    assert!(rules.contains_key("block-no-empty"));

    // Also works for .mjs
    let raw2 = parse_config_file("stylelint.config.mjs", js, None).unwrap();
    assert!(raw2.rules.is_some());

    // Also works for .cjs
    let raw3 = parse_config_file("stylelint.config.cjs", js, None).unwrap();
    assert!(raw3.rules.is_some());
  }

  #[test]
  fn find_config_discovers_package_json_stylelint_field() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("package.json");
    std::fs::write(
      &pkg,
      r#"{
                "name": "my-project",
                "stylelint": {
                    "rules": { "block-no-empty": true }
                }
            }"#,
    )
    .unwrap();

    let found = find_config(tmp.path());
    assert_eq!(found, Some(pkg));
  }

  #[test]
  fn find_config_prefers_stylelintrc_over_package_json() {
    let tmp = tempfile::tempdir().unwrap();
    // Both .stylelintrc.json and package.json with stylelint field exist.
    std::fs::write(
      tmp.path().join(".stylelintrc.json"),
      r#"{ "rules": { "color-no-invalid-hex": true } }"#,
    )
    .unwrap();
    std::fs::write(
      tmp.path().join("package.json"),
      r#"{ "name": "x", "stylelint": { "rules": { "block-no-empty": true } } }"#,
    )
    .unwrap();

    let found = find_config(tmp.path()).unwrap();
    assert_eq!(found.file_name().unwrap(), ".stylelintrc.json");
  }

  #[test]
  fn find_config_ignores_package_json_without_stylelint_field() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
      tmp.path().join("package.json"),
      r#"{ "name": "no-lint-config" }"#,
    )
    .unwrap();

    assert_eq!(find_config(tmp.path()), None);
  }

  #[test]
  fn load_config_from_package_json_object() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("package.json");
    std::fs::write(
      &pkg,
      r#"{
                "name": "my-project",
                "stylelint": {
                    "rules": {
                        "block-no-empty": true,
                        "color-no-invalid-hex": "warning"
                    }
                }
            }"#,
    )
    .unwrap();

    let cfg = load_config(&pkg).unwrap();
    assert_eq!(cfg.rules.len(), 2);
    assert!(cfg.rules.contains_key("block-no-empty"));
    assert!(cfg.rules.contains_key("color-no-invalid-hex"));
  }

  #[test]
  fn load_config_from_package_json_string_extends() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("package.json");
    std::fs::write(
      &pkg,
      r#"{
                "name": "my-project",
                "stylelint": "stylelint-config-standard"
            }"#,
    )
    .unwrap();

    // load_config will try to resolve extends, which will fail for
    // "stylelint-config-standard" without node_modules.  Instead,
    // test parse_package_json_stylelint directly.
    let contents = std::fs::read_to_string(&pkg).unwrap();
    let raw = parse_package_json_stylelint(&contents).unwrap();
    assert_eq!(
      raw.extends,
      Some(vec!["stylelint-config-standard".to_string()])
    );
  }

  #[test]
  fn parse_package_json_stylelint_missing_field() {
    let contents = r#"{ "name": "no-lint" }"#;
    assert!(parse_package_json_stylelint(contents).is_err());
  }
}
