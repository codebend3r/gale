//! Running a JavaScript config under Node.js, the way Stylelint loads it.
//!
//! Node imports the file, takes its default export (`import()` hands a
//! CommonJS module's `module.exports` over as the default), and prints it
//! as JSON for gale to read.  JSON has no regular expressions or
//! functions, so [`LOADER`] converts what it can and lists what it drops.

use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

/// The module Node runs, given the config's path as its one argument.
///
/// It prints two lines: a JSON array naming each value it dropped, then
/// the config as JSON.  Along the way:
///
/// - A regular expression becomes the string `/source/flags`, which gale's
///   rule options already read as a regex.
/// - A `message` function is called with the placeholder `${name}`, giving
///   the string gale's custom messages fill in with the first message
///   argument; any other function is dropped, and so is a symbol.
/// - A value some CommonJS module exports and that holds code (a plugin, a
///   `customSyntax`) becomes that module's package name, or its path
///   outside `node_modules`, which is how gale recognises plugins and
///   syntaxes by name.  An ES module's export has no such record, so a
///   plugin object gale cannot name is dropped without comment: gale runs
///   no plugin code either way.
/// - `undefined` is left out, as `JSON.stringify` always does.
///
/// Anything the config prints goes to stderr, keeping stdout for the
/// result.  A config that throws, or fails to import, exits 1 with the
/// error on stderr.
const LOADER: &str = r#"
import { builtinModules, createRequire } from "node:module";
import { pathToFileURL } from "node:url";

// `node -e` makes every builtin module a global, as the REPL does.  A
// config Stylelint loads sees only Node's real globals, so take the rest
// away before this one runs.
for (const name of builtinModules) {
  if (!["console", "crypto", "process"].includes(name)) delete globalThis[name];
}

const file = process.argv[1];
const write = process.stdout.write.bind(process.stdout);
process.stdout.write = process.stderr.write.bind(process.stderr);

let config;
try {
  const mod = await import(pathToFileURL(file).href);
  config = mod.default ?? mod;
} catch (error) {
  process.exitCode = 1;
  process.stderr.write(error instanceof Error ? `${error.name}: ${error.message}` : String(error));
}
if (process.exitCode !== 1) {
  // Exit once written: a config may leave a timer or handle open.
  write(serialize(config), () => process.exit(0));
}

function holdsCode(value, seen = new Set()) {
  if (typeof value === "function") return true;
  if (typeof value !== "object" || value === null || seen.has(value)) return false;
  seen.add(value);
  return Object.values(value).some((item) => holdsCode(item, seen));
}

function packageName(filename) {
  const parts = filename.split(/[\\/]node_modules[\\/]/);
  if (parts.length < 2) return filename;
  const [scope, name] = parts.at(-1).split(/[\\/]/);
  return scope.startsWith("@") ? `${scope}/${name}` : scope;
}

function serialize(config) {
  const modules = new Map();
  for (const [filename, entry] of Object.entries(createRequire(file).cache)) {
    if (entry.exports !== config && holdsCode(entry.exports)) {
      modules.set(entry.exports, packageName(filename));
    }
  }

  const dropped = [];
  const paths = new WeakMap();
  const json = JSON.stringify(config, function (key, value) {
    const parent = paths.get(this);
    if (parent === undefined) {
      if (typeof value === "object" && value !== null) paths.set(value, "");
      return value;
    }
    const path = Array.isArray(this) ? `${parent}[${key}]` : parent ? `${parent}.${key}` : key;
    if (modules.has(value)) return modules.get(value);
    if (value instanceof RegExp) return String(value);
    if (/(^|\.)plugins$/.test(parent) && typeof value !== "string") return undefined;
    if (typeof value === "function") {
      if (key === "message") {
        try {
          const message = value("${name}");
          if (typeof message === "string") return message;
        } catch {}
      }
      dropped.push(`${path} (a function)`);
      return undefined;
    }
    if (typeof value === "symbol") {
      dropped.push(`${path} (a symbol)`);
      return undefined;
    }
    if (typeof value === "bigint") return Number(value);
    if (typeof value === "object" && value !== null) paths.set(value, path);
    return value;
  });
  return `${JSON.stringify(dropped)}\n${json ?? "null"}\n`;
}
"#;

/// A config Node.js evaluated: its value, and the key path of each value
/// JSON could not hold (with what it was), which the caller warns about.
pub(super) struct Evaluated {
  pub config: Value,
  pub dropped: Vec<String>,
}

/// Evaluate the JavaScript file at `path` with Node.js: `GALE_NODE` when
/// it is set (the npm launcher sets it to the Node running gale), else
/// `node` on `PATH`.
///
/// `None` when Node.js cannot be started at all; `Some(Err(message))` when
/// the config throws or fails to import, with Node's error message.
pub(super) fn evaluate(path: &Path) -> Option<Result<Evaluated, String>> {
  let node = std::env::var_os("GALE_NODE")
    .filter(|node| !node.is_empty())
    .unwrap_or_else(|| "node".into());
  let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
  let output = Command::new(node)
    .args(["--input-type=module", "-e", LOADER])
    .arg(&path)
    .stdin(Stdio::null())
    .output()
    .ok()?;

  let stderr = String::from_utf8_lossy(&output.stderr);
  if !output.status.success() {
    let message = stderr.trim();
    return Some(Err(if message.is_empty() {
      format!("Node.js exited with {}", output.status)
    } else {
      message.to_string()
    }));
  }
  // What the config itself printed still reaches the user.
  eprint!("{stderr}");

  let stdout = String::from_utf8_lossy(&output.stdout);
  let (dropped, config) = stdout.split_once('\n').unwrap_or((&stdout, "null"));
  let parsed = serde_json::from_str(dropped)
    .and_then(|dropped| serde_json::from_str(config).map(|config| Evaluated { config, dropped }));
  Some(parsed.map_err(|err| format!("could not read what Node.js printed: {err}")))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn values_json_cannot_hold_are_converted_or_named() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("stylelint.config.mjs");
    std::fs::write(
      &config,
      "export default { a: /^x$/gi, b: [{ message: (n) => `bad ${n}`, fn() {} }] };\n",
    )
    .unwrap();
    let Some(result) = evaluate(&config) else {
      return; // Node.js is not installed here.
    };
    let evaluated = result.unwrap();
    assert_eq!(
      evaluated.config,
      serde_json::json!({ "a": "/^x$/gi", "b": [{ "message": "bad ${name}" }] })
    );
    assert_eq!(evaluated.dropped, vec!["b[0].fn (a function)"]);
  }

  #[test]
  fn a_config_that_throws_reports_the_error() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("stylelint.config.js");
    std::fs::write(&config, "throw new TypeError('boom');\n").unwrap();
    if let Some(result) = evaluate(&config) {
      assert_eq!(result.err().unwrap(), "TypeError: boom");
    }
  }
}
