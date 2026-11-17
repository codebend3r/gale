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
mod tests;
