//! Resolving the imports, re-exports and constants a JavaScript config
//! refers to, so its exported object can be read on its own.

use std::path::{Path, PathBuf};

use super::to_json::js_object_to_json;
use crate::extends::{resolve_package_exports, split_npm_package_subpath};

/// Extract import bindings from JS source.
///
/// Recognises:
/// - `import <name> from '<path>'`  (ESM default import)
/// - `const <name> = require('<path>')` (CJS require)
///
/// Returns `(variable_name, module_path)` pairs.
fn extract_imports(source: &str) -> Vec<(String, String)> {
  let mut imports = Vec::new();

  for line in source.lines() {
    let trimmed = line.trim();

    // ESM: import <name> from '<path>'
    if let Some(after_import) = trimmed.strip_prefix("import ") {
      // Skip destructured imports like `import { foo } from ...`
      let rest = after_import.trim_start();
      if rest.starts_with('{') || rest.starts_with('*') {
        continue;
      }
      // Extract: <name> from '<path>'
      if let Some(from_idx) = rest.find(" from ") {
        let var_name = rest[..from_idx].trim().to_string();
        // Validate variable name (simple identifier)
        if var_name.is_empty()
          || !var_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
          continue;
        }
        let path_part = rest[from_idx + " from ".len()..].trim();
        if let Some(path) = extract_string_literal(path_part) {
          imports.push((var_name, path));
        }
      }
    }

    // CJS: const <name> = require('<path>')
    if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
      let rest = if let Some(r) = trimmed.strip_prefix("const ") {
        r
      } else if let Some(r) = trimmed.strip_prefix("let ") {
        r
      } else if let Some(r) = trimmed.strip_prefix("var ") {
        r
      } else {
        unreachable!()
      };
      let rest = rest.trim_start();
      // Skip destructured: const { foo } = require(...)
      if rest.starts_with('{') || rest.starts_with('[') {
        continue;
      }
      // Find `= require(`
      if let Some(eq_idx) = rest.find('=') {
        let var_name = rest[..eq_idx].trim().to_string();
        if var_name.is_empty()
          || !var_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
          continue;
        }
        let after_eq = rest[eq_idx + 1..].trim();
        if let Some(inside) = after_eq.strip_prefix("require(")
          && let Some(path) = extract_string_literal(inside)
        {
          imports.push((var_name, path));
        }
      }
    }
  }

  imports
}

/// Extract a string literal value from text that starts with `'`, `"`, or a
/// backtick. Returns the content without quotes.
/// Detect re-export patterns like `module.exports = require("./path")`.
///
/// Returns the require path if the entire JS file is a re-export, i.e. the
/// exported value is a `require()` call rather than an object literal.
pub(super) fn extract_reexport_require(source: &str) -> Option<String> {
  for line in source.lines() {
    let trimmed = line.trim();
    // Skip comments and empty lines
    if trimmed.is_empty()
      || trimmed.starts_with("//")
      || trimmed.starts_with("/*")
      || trimmed.starts_with('*')
      || trimmed.starts_with('"')
    {
      continue;
    }
    // Match: module.exports = require("./path")
    // or:    module.exports=require('./path');
    for prefix in &["module.exports", "exports"] {
      if let Some(rest) = trimmed.strip_prefix(prefix) {
        let rest = rest.trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
          let rest = rest.trim_start();
          if let Some(rest) = rest.strip_prefix("require(") {
            let rest = rest.trim_start();
            if let Some(path) = extract_string_literal(rest) {
              return Some(path);
            }
          }
        }
      }
    }
  }
  None
}

/// Reads the contents of a leading single- or double-quoted string literal.
fn extract_string_literal(s: &str) -> Option<String> {
  let s = s.trim();
  let quote = s.chars().next()?;
  if quote != '\'' && quote != '"' {
    return None;
  }
  let rest = &s[1..];
  let end = rest.find(quote)?;
  Some(rest[..end].to_string())
}

/// Try to resolve an import path to an actual file on disk.
/// Tries the path as-is, then with `.js` and `.json` extensions, then as
/// a directory with `index.js`.
pub(super) fn resolve_import_path(file_dir: &Path, rel_path: &str) -> Option<PathBuf> {
  let base = file_dir.join(rel_path);

  // Try as-is first
  if base.is_file() {
    return Some(base);
  }

  // Try with extensions
  for ext in &[".js", ".mjs", ".cjs", ".json"] {
    let with_ext = PathBuf::from(format!("{}{}", base.display(), ext));
    if with_ext.is_file() {
      return Some(with_ext);
    }
  }

  // Try as directory with index.js
  let index = base.join("index.js");
  if index.is_file() {
    return Some(index);
  }

  None
}

/// Resolve an npm package import by walking up the directory tree looking for
/// `node_modules/<package>/`.  Tries the package's `main` field, then
/// `index.js`, then `index.cjs`.
#[allow(dead_code)]
fn resolve_npm_package_path(file_dir: &Path, package: &str) -> Option<PathBuf> {
  // Split into package name and optional subpath for exports resolution
  let (pkg_name, subpath) = split_npm_package_subpath(package);

  let mut dir = file_dir.to_path_buf();
  loop {
    let candidate = dir.join("node_modules").join(pkg_name);
    if candidate.is_dir() {
      // Try package.json "exports" field first, then "main"
      let pkg_json = candidate.join("package.json");
      if pkg_json.is_file()
        && let Ok(contents) = std::fs::read_to_string(&pkg_json)
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&contents)
      {
        // Try exports field
        let exports_subpath = subpath.unwrap_or(".");
        if let Some(resolved) = resolve_package_exports(&parsed, exports_subpath) {
          let export_path = candidate.join(&resolved);
          if export_path.is_file() {
            return Some(export_path);
          }
        }
        // Try main field (only when no subpath)
        if subpath.is_none()
          && let Some(main) = parsed.get("main").and_then(|v| v.as_str())
        {
          let main_path = candidate.join(main);
          if main_path.is_file() {
            return Some(main_path);
          }
        }
      }

      if subpath.is_none() {
        // Try index.js, index.cjs
        for name in &["index.js", "index.cjs", "index.mjs"] {
          let idx = candidate.join(name);
          if idx.is_file() {
            return Some(idx);
          }
        }
      }
    }

    // Also try as a file directly (e.g. scoped package with subpath)
    let candidate_file = dir.join("node_modules").join(package);
    for ext in &["", ".js", ".cjs", ".json", ".mjs"] {
      let with_ext = PathBuf::from(format!("{}{}", candidate_file.display(), ext));
      if with_ext.is_file() {
        return Some(with_ext);
      }
    }
    if !dir.pop() {
      return None;
    }
  }
}

/// Extract the default export value from a JS source file.
///
/// Handles:
/// - `export default <value>`
/// - `module.exports = <value>`
/// - `const X = <value>; export default X` (variable indirection)
fn extract_default_export(source: &str) -> Option<String> {
  // First try direct `export default <literal>` or `module.exports = <literal>`
  let export_markers: &[&str] = &["export default ", "module.exports =", "module.exports="];

  for marker in export_markers {
    if let Some(pos) = source.find(marker) {
      let after = source[pos + marker.len()..].trim_start();
      // If it starts with a literal value (array or object), extract it
      if after.starts_with('[')
        && let Some(val) = extract_balanced(after, '[', ']')
      {
        return Some(val);
      }
      if after.starts_with('{')
        && let Some(val) = extract_balanced(after, '{', '}')
      {
        return Some(val);
      }
      // Otherwise it might be a variable name: `export default X`
      // Find the identifier
      let ident_end = after
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(after.len());
      let ident = after[..ident_end].trim();
      if !ident.is_empty() {
        // Look for `const <ident> = <value>` in the source
        if let Some(val) = find_variable_value(source, ident) {
          return Some(val);
        }
      }
    }
  }

  None
}

/// Find the value assigned to a `const`/`let`/`var` variable in JS source.
///
/// Handles object literals (`{…}`), array literals (`[…]`), and scalar values
/// (e.g. `null`, `true`, `false`, numbers, quoted strings).
pub(super) fn find_variable_value(source: &str, var_name: &str) -> Option<String> {
  for keyword in &["const ", "let ", "var "] {
    let pattern = format!("{}{}", keyword, var_name);
    if let Some(pos) = source.find(&pattern) {
      let after_name = &source[pos + pattern.len()..];
      let after_name = after_name.trim_start();
      if !after_name.starts_with('=') {
        continue;
      }
      let after_eq = after_name[1..].trim_start();
      if after_eq.starts_with('[') {
        return extract_balanced(after_eq, '[', ']');
      }
      if after_eq.starts_with('{') {
        return extract_balanced(after_eq, '{', '}');
      }
      // Scalar value: read up to `;` or end of line.
      let end = after_eq.find([';', '\n']).unwrap_or(after_eq.len());
      let val = after_eq[..end].trim();
      if !val.is_empty() {
        return Some(val.to_string());
      }
    }
  }
  None
}

/// Extract a balanced bracket/brace expression, respecting strings and nesting.
fn extract_balanced(s: &str, open: char, close: char) -> Option<String> {
  let mut depth = 0i32;
  let mut result = String::new();
  let mut in_single = false;
  let mut in_double = false;
  let mut in_template = false;
  let mut escape_next = false;

  for c in s.chars() {
    if escape_next {
      result.push(c);
      escape_next = false;
      continue;
    }
    if c == '\\' && (in_single || in_double || in_template) {
      result.push(c);
      escape_next = true;
      continue;
    }
    if !in_double && !in_template && c == '\'' {
      in_single = !in_single;
    } else if !in_single && !in_template && c == '"' {
      in_double = !in_double;
    } else if !in_single && !in_double && c == '`' {
      in_template = !in_template;
    }

    if !in_single && !in_double && !in_template {
      if c == open {
        depth += 1;
      } else if c == close {
        depth -= 1;
      }
    }

    result.push(c);

    if depth == 0 {
      return Some(result);
    }
  }

  None
}

/// Resolve relative imports in JS source by reading imported files and
/// substituting variable references with the literal exported values.
pub(super) fn resolve_js_imports(source: &str, file_dir: Option<&Path>) -> String {
  let file_dir = match file_dir {
    Some(d) => d,
    None => return source.to_string(),
  };

  let imports = extract_imports(source);
  if imports.is_empty() {
    return source.to_string();
  }

  let mut result = source.to_string();

  for (var_name, rel_path) in &imports {
    let import_path = if rel_path.starts_with("./") || rel_path.starts_with("../") {
      // Relative import — resolve from file_dir.
      match resolve_import_path(file_dir, rel_path) {
        Some(p) => p,
        None => continue,
      }
    } else {
      // npm package import — try to resolve it and inline only if the
      // exported value converts cleanly to valid JSON (e.g. an array of
      // strings like @github/browserslist-config).  Packages that export
      // non-serialisable JS (functions, shorthand properties, etc.) are
      // left as-is so the JS→JSON converter can substitute `null` for
      // unresolved identifiers rather than producing invalid JSON.
      let candidate = match resolve_npm_package_path(file_dir, rel_path) {
        Some(p) => p,
        None => continue,
      };
      let npm_source = match std::fs::read_to_string(&candidate) {
        Ok(s) => s,
        Err(_) => continue,
      };
      if let Some(raw_value) = extract_default_export(&npm_source) {
        let json_value = js_object_to_json(&raw_value);
        // Only inline if the result is valid JSON.
        if serde_json::from_str::<serde_json::Value>(&json_value).is_ok() {
          result = replace_whole_word(&result, var_name, &json_value);
        }
      }
      continue;
    };

    let imported_source = match std::fs::read_to_string(&import_path) {
      Ok(s) => s,
      Err(_) => continue,
    };

    if let Some(value) = extract_default_export(&imported_source) {
      // Replace occurrences of the variable name with the literal value.
      // We need to be careful to only replace whole-word occurrences
      // (not substrings of other identifiers).
      result = replace_whole_word(&result, var_name, &value);
    }
  }

  result
}

/// Replace whole-word occurrences of `word` with `replacement` in `source`.
/// A word boundary is a position where the adjacent character is not
/// alphanumeric or `_` or `$`.
fn replace_whole_word(source: &str, word: &str, replacement: &str) -> String {
  if word.is_empty() {
    return source.to_string();
  }
  let mut result = String::with_capacity(source.len());
  let mut remaining = source;

  while let Some(pos) = remaining.find(word) {
    // Check character before the match
    let before_ok = if pos == 0 {
      true
    } else {
      let ch = remaining[..pos].chars().next_back().unwrap_or('\0');
      !ch.is_alphanumeric() && ch != '_' && ch != '$'
    };
    // Check character after the match
    let after_pos = pos + word.len();
    let after_ok = if after_pos >= remaining.len() {
      true
    } else {
      let ch = remaining[after_pos..].chars().next().unwrap_or('\0');
      !ch.is_alphanumeric() && ch != '_' && ch != '$'
    };

    if before_ok && after_ok {
      result.push_str(&remaining[..pos]);
      result.push_str(replacement);
      remaining = &remaining[after_pos..];
    } else {
      result.push_str(&remaining[..after_pos]);
      remaining = &remaining[after_pos..];
    }
  }
  result.push_str(remaining);
  result
}

/// Strip `import ... from ...` and `const/let/var ... = require(...)` lines
/// from the source so they don't confuse the JS-to-JSON converter.
pub(super) fn strip_import_lines(source: &str) -> String {
  let mut lines: Vec<&str> = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim();
    // Skip ESM import lines
    if trimmed.starts_with("import ") && trimmed.contains(" from ") {
      continue;
    }
    // Skip CJS require lines
    if (trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var "))
      && trimmed.contains("require(")
    {
      continue;
    }
    lines.push(line);
  }
  lines.join("\n")
}

/// Substitute scalar `const`/`let`/`var` variables in JS source.
///
/// Finds declarations like `const OFF = null;` and replaces every subsequent
/// bare-identifier usage of `OFF` (in a value position) with `null`.  This
/// allows the JS→JSON converter to handle files that use named constants for
/// rule severities or other scalar values.
pub(super) fn substitute_scalar_vars(source: &str) -> String {
  // Collect all scalar variable declarations: `const/let/var NAME = VALUE;`
  let mut vars: Vec<(String, String)> = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim();
    for keyword in &["const ", "let ", "var "] {
      if let Some(rest) = trimmed.strip_prefix(keyword) {
        // `NAME = VALUE;` or `NAME = VALUE`
        if let Some(eq_pos) = rest.find('=') {
          let name = rest[..eq_pos].trim();
          // Only simple identifiers (no destructuring).
          if name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            && !name.is_empty()
          {
            let val = rest[eq_pos + 1..].trim();
            let val = val.strip_suffix(';').unwrap_or(val).trim();
            // Only scalar values: null, true, false, numbers, quoted strings.
            // Skip objects/arrays (handled elsewhere) and complex expressions.
            let is_scalar = val == "null"
              || val == "true"
              || val == "false"
              || val == "undefined"
              || val.parse::<f64>().is_ok()
              || (val.starts_with('\'') && val.ends_with('\''))
              || (val.starts_with('"') && val.ends_with('"'));
            if is_scalar {
              vars.push((name.to_string(), val.to_string()));
            }
          }
        }
      }
    }
  }

  if vars.is_empty() {
    return source.to_string();
  }

  // Replace bare identifier usages with their values.
  // We do a simple word-boundary replacement: replace occurrences that are
  // not preceded or followed by an alphanumeric or underscore character.
  let mut result = source.to_string();
  for (name, value) in &vars {
    let mut new_result = String::with_capacity(result.len());
    let name_len = name.len();
    let mut i = 0;
    let bytes = result.as_bytes();

    while i < bytes.len() {
      if i + name_len <= bytes.len() && &result[i..i + name_len] == name.as_str() {
        // Check word boundary before
        let before_ok = i == 0
          || !(bytes[i - 1] as char).is_alphanumeric()
            && bytes[i - 1] != b'_'
            && bytes[i - 1] != b'$';
        // Check word boundary after
        let after_ok = i + name_len >= bytes.len()
          || !(bytes[i + name_len] as char).is_alphanumeric()
            && bytes[i + name_len] != b'_'
            && bytes[i + name_len] != b'$';

        // Don't replace in declaration context (const NAME =)
        let in_decl = {
          let prefix = &result[..i];
          let trimmed = prefix.trim_end();
          trimmed.ends_with("const") || trimmed.ends_with("let") || trimmed.ends_with("var")
        };

        if before_ok && after_ok && !in_decl {
          new_result.push_str(value);
          i += name_len;
          continue;
        }
      }
      new_result.push(bytes[i] as char);
      i += 1;
    }
    result = new_result;
  }

  result
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn extract_imports_esm_default() {
    let src = r#"
import propertyGroups from './groups.js'
import something from 'not-relative'
const config = {}
"#;
    let imports = extract_imports(src);
    assert_eq!(imports.len(), 2);
    assert_eq!(
      imports[0],
      ("propertyGroups".to_string(), "./groups.js".to_string())
    );
    assert_eq!(
      imports[1],
      ("something".to_string(), "not-relative".to_string())
    );
  }

  #[test]
  fn extract_imports_cjs_require() {
    let src = r#"
const groups = require('./groups')
const stylelint = require('stylelint')
"#;
    let imports = extract_imports(src);
    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0], ("groups".to_string(), "./groups".to_string()));
    assert_eq!(
      imports[1],
      ("stylelint".to_string(), "stylelint".to_string())
    );
  }

  #[test]
  fn extract_imports_skips_destructured() {
    let src = r#"
import { foo } from './bar'
const { baz } = require('./qux')
"#;
    let imports = extract_imports(src);
    assert!(imports.is_empty());
  }

  #[test]
  fn extract_default_export_direct_array() {
    let src = r#"
const foo = 'bar'
export default [1, 2, 3]
"#;
    let val = extract_default_export(src).unwrap();
    assert_eq!(val, "[1, 2, 3]");
  }

  #[test]
  fn extract_default_export_via_variable() {
    let src = r#"
const myArray = [
    { "properties": ["a", "b"] },
    { "properties": ["c", "d"] }
]
export default myArray
"#;
    let val = extract_default_export(src).unwrap();
    assert!(val.starts_with('['));
    assert!(val.contains("\"properties\""));
    assert!(val.ends_with(']'));
  }

  #[test]
  fn extract_default_export_module_exports() {
    let src = r#"
const data = [1, 2, 3]
module.exports = data
"#;
    let val = extract_default_export(src).unwrap();
    assert_eq!(val, "[1, 2, 3]");
  }

  #[test]
  fn extract_reexport_require_detects_pattern() {
    let src = r#""use strict"
module.exports = require("./stylelint.config")
"#;
    let path = extract_reexport_require(src);
    assert_eq!(path, Some("./stylelint.config".to_string()));
  }

  #[test]
  fn extract_reexport_require_no_match_for_object() {
    let src = r#"module.exports = { rules: {} }"#;
    let path = extract_reexport_require(src);
    assert_eq!(path, None);
  }

  #[test]
  fn extract_reexport_require_single_quotes() {
    let src = "module.exports = require('./config');\n";
    let path = extract_reexport_require(src);
    assert_eq!(path, Some("./config".to_string()));
  }

  #[test]
  fn strip_import_lines_removes_imports() {
    let src = "import foo from './bar'\nconst x = require('./baz')\nexport default {}";
    let stripped = strip_import_lines(src);
    assert!(!stripped.contains("import foo"));
    assert!(!stripped.contains("require"));
    assert!(stripped.contains("export default {}"));
  }

  #[test]
  fn replace_whole_word_basic() {
    let src = "foo + foobar + foo";
    let result = replace_whole_word(src, "foo", "42");
    assert_eq!(result, "42 + foobar + 42");
  }
}
