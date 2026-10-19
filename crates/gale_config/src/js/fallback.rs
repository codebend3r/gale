//! Reading a JavaScript config without running it, for when Node.js is not
//! available.
//!
//! The reader understands the part of JavaScript a config is usually
//! written in: an exported object literal of strings, numbers, booleans,
//! regular expressions, arrays and nested objects, `+` between strings,
//! constants declared with `const`/`let`/`var`, `require(...)` and default
//! `import`s of other files, and a one-parameter arrow function returning a
//! template literal (a custom `message`).  Anything else (a spread, a call,
//! `process.env`, a name it cannot resolve, a statement that changes the
//! config) is skipped, and its source text is reported so the caller can
//! say what was left out.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Number, Value};

use super::scan::{Kind, Token, tokenize};
use crate::ConfigError;
use crate::extends::{resolve_package_exports, split_npm_package_subpath};

/// How deep imports are followed, so a cycle of files cannot recurse
/// forever.
const MAX_IMPORT_DEPTH: usize = 8;

/// The longest skipped source text reported, in characters.
const MAX_SKIPPED_LEN: usize = 80;

/// What reading a file produced: its exported value (`None` when the
/// export itself could not be evaluated) and the source text of everything
/// skipped on the way.
pub(super) struct Reading {
  pub value: Option<Value>,
  pub skipped: Vec<String>,
}

/// Read the JavaScript (or JSON) module at `path` and evaluate its export.
pub(super) fn read(path: &Path) -> Result<Reading, ConfigError> {
  read_module(path, 0)
}

fn read_module(path: &Path, depth: usize) -> Result<Reading, ConfigError> {
  let source = std::fs::read_to_string(path)?;
  if path.extension().is_some_and(|ext| ext == "json") {
    return Ok(Reading {
      value: Some(serde_json::from_str(&source)?),
      skipped: Vec::new(),
    });
  }
  read_source(&source, path.parent(), depth)
}

/// Evaluate the export of the JavaScript `source`, resolving relative
/// imports from `dir`.
pub(super) fn read_source(
  source: &str,
  dir: Option<&Path>,
  depth: usize,
) -> Result<Reading, ConfigError> {
  let mut reader = Reader {
    src: source,
    tokens: tokenize(source),
    pos: 0,
    dir,
    depth,
    bindings: HashMap::new(),
    evaluating: HashSet::new(),
    skipped: Vec::new(),
  };
  let export = reader.program().ok_or_else(|| {
    ConfigError::UnsupportedFormat(
      "no `module.exports =` or `export default` found in JS config".to_string(),
    )
  })?;
  reader.pos = export;
  let value = reader.expr();
  let mut seen = HashSet::new();
  reader.skipped.retain(|text| seen.insert(text.clone()));
  Ok(Reading {
    value,
    skipped: reader.skipped,
  })
}

/// What a top-level name was declared as.
#[derive(Clone)]
enum Binding {
  /// `const name = <expression starting at this token>`.
  Init(usize),
  /// `import name from '<module>'`.  (`const name = require('<module>')`
  /// is an `Init`, evaluated through `require`.)
  Import(String),
}

/// The expression at the cursor is not one the reader evaluates.
struct Unsupported;

struct Reader<'a> {
  src: &'a str,
  tokens: Vec<Token>,
  pos: usize,
  dir: Option<&'a Path>,
  depth: usize,
  bindings: HashMap<&'a str, Binding>,
  /// Names being evaluated right now, so `const a = b, b = a` stops.
  evaluating: HashSet<&'a str>,
  skipped: Vec<String>,
}

impl<'a> Reader<'a> {
  fn token(&self, at: usize) -> Option<&Token> {
    self.tokens.get(at)
  }

  fn text(&self, at: usize) -> &'a str {
    self
      .tokens
      .get(at)
      .map_or("", |t| &self.src[t.start..t.end])
  }

  /// The identifier at `at`, if the token there is one.
  fn ident(&self, at: usize) -> Option<&'a str> {
    self
      .token(at)
      .filter(|t| t.kind == Kind::Ident)
      .map(|_| self.text(at))
  }

  fn at(&self, p: &str) -> bool {
    self.token(self.pos).is_some_and(|t| t.is(p))
  }

  fn eat(&mut self, p: &str) -> bool {
    let found = self.at(p);
    if found {
      self.pos += 1;
    }
    found
  }

  /// Record the source text of tokens `start..end` as skipped.
  fn skip(&mut self, start: usize, end: usize) {
    if end <= start {
      return;
    }
    let text = &self.src[self.tokens[start].start..self.tokens[end - 1].end];
    let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((cut, _)) = text.char_indices().nth(MAX_SKIPPED_LEN) {
      text.truncate(cut);
      text.push('…');
    }
    self.skipped.push(text);
  }

  /// Whether the token at `at` ends the expression before it: a separator,
  /// a closing bracket, or a new line where JavaScript would insert a `;`
  /// (the line ends in a value and the next starts with one).
  fn ends_expression(&self, at: usize) -> bool {
    let Some(token) = self.token(at) else {
      return true;
    };
    if [",", ")", "]", "}", ";"].iter().any(|p| token.is(p)) {
      return true;
    }
    let starts_value = !matches!(token.kind, Kind::Punct(_));
    let previous_ends_value = at > 0
      && match self.tokens[at - 1].kind {
        Kind::Punct(p) => matches!(p, ")" | "]" | "}"),
        _ => true,
      };
    token.newline_before && starts_value && previous_ends_value
  }

  /// Move past the expression at the cursor without evaluating it.
  fn skip_expr(&mut self) {
    let start = self.pos;
    let mut depth = 0usize;
    while let Some(token) = self.token(self.pos) {
      if depth == 0 && (self.pos > start || token.is(";")) && self.ends_expression(self.pos) {
        break;
      }
      match token.kind {
        Kind::Punct("(" | "[" | "{") => depth += 1,
        Kind::Punct(")" | "]" | "}") if depth == 0 => break,
        Kind::Punct(")" | "]" | "}") => depth -= 1,
        _ => {}
      }
      self.pos += 1;
    }
  }

  /// Read the top-level statements, recording what each name is bound to,
  /// and return where the exported expression starts.
  fn program(&mut self) -> Option<usize> {
    let mut export = None;
    while self.pos < self.tokens.len() {
      let start = self.pos;
      match self.ident(self.pos) {
        Some("import")
          if self.ident(self.pos + 1).is_some()
            || self
              .token(self.pos + 1)
              .is_some_and(|t| t.is("{") || t.is("*") || matches!(t.kind, Kind::Str(_))) =>
        {
          self.import();
        }
        Some("export") if self.ident(self.pos + 1) == Some("default") => {
          self.pos += 2;
          export = Some(self.pos);
          self.skip_expr();
        }
        Some("export") if matches!(self.ident(self.pos + 1), Some("const" | "let" | "var")) => {
          self.pos += 1;
          self.declaration();
        }
        Some("const" | "let" | "var") => self.declaration(),
        Some("module")
          if self.token(self.pos + 1).is_some_and(|t| t.is("."))
            && self.ident(self.pos + 2) == Some("exports")
            && self.token(self.pos + 3).is_some_and(|t| t.is("=")) =>
        {
          self.pos += 4;
          export = Some(self.pos);
          self.skip_expr();
        }
        // Declaring a function or class changes nothing until it is called.
        Some("function" | "async" | "class") => {
          self.skip_expr();
        }
        // A directive such as `'use strict'`.
        _ if matches!(self.token(self.pos).map(|t| &t.kind), Some(Kind::Str(_)))
          && self.ends_expression(self.pos + 1) =>
        {
          self.pos += 1;
        }
        _ if self.at(";") => {}
        _ => {
          self.skip_expr();
          self.skip(start, self.pos);
        }
      }
      self.eat(";");
      if self.pos == start {
        // Never stall on a token nothing above consumes (a stray `)`).
        self.pos += 1;
      }
    }
    export
  }

  /// `import name from '<module>'`, and the other import forms, which bind
  /// nothing the reader can use.
  fn import(&mut self) {
    self.pos += 1;
    let default = self.ident(self.pos).filter(|_| {
      self.ident(self.pos + 1) == Some("from")
        || self.token(self.pos + 1).is_some_and(|t| t.is(","))
    });
    while let Some(token) = self.token(self.pos) {
      let spec = match &token.kind {
        Kind::Str(spec) => Some(spec.clone()),
        _ => None,
      };
      self.pos += 1;
      if let Some(spec) = spec {
        if let Some(name) = default {
          self.bindings.insert(name, Binding::Import(spec));
        }
        break;
      }
    }
    // Import attributes: `with { type: 'json' }`.
    if matches!(self.ident(self.pos), Some("with" | "assert"))
      && !self.token(self.pos).is_some_and(|t| t.newline_before)
    {
      self.pos += 1;
      self.skip_expr();
    }
  }

  /// `const a = 1, b = 2`: record where each initializer starts.
  /// Destructuring binds nothing the reader can use.
  fn declaration(&mut self) {
    self.pos += 1;
    loop {
      match self.ident(self.pos) {
        Some(name) if self.token(self.pos + 1).is_some_and(|t| t.is("=")) => {
          self.pos += 2;
          self.bindings.insert(name, Binding::Init(self.pos));
          self.skip_expr();
        }
        _ => self.skip_expr(),
      }
      if !self.eat(",") {
        break;
      }
    }
  }

  /// Evaluate the expression at the cursor, consuming all of it.  `None`
  /// stands for `undefined`, which is also what an expression the reader
  /// cannot evaluate becomes (after its text is recorded as skipped).
  fn expr(&mut self) -> Option<Value> {
    let start = self.pos;
    let recorded = self.skipped.len();
    if let Ok(value) = self.operand()
      && self.ends_expression(self.pos)
    {
      return value;
    }
    // Whatever the parts recorded is covered by skipping the whole.
    self.skipped.truncate(recorded);
    self.pos = start;
    self.skip_expr();
    self.skip(start, self.pos);
    None
  }

  /// Evaluate one operand: a literal, a name, `require(...)` or a message
  /// function.
  fn operand(&mut self) -> Result<Option<Value>, Unsupported> {
    let token = self.token(self.pos).ok_or(Unsupported)?.clone();
    let text = self.text(self.pos);
    self.pos += 1;
    let value = match token.kind {
      Kind::Str(value) | Kind::Template(value) => Value::String(self.concat(value)?),
      Kind::Number => number(text)?,
      Kind::Regex => Value::String(text.to_string()),
      Kind::Punct("-") if self.token(self.pos).is_some_and(|t| t.kind == Kind::Number) => {
        let negated = format!("-{}", self.text(self.pos));
        self.pos += 1;
        number(&negated)?
      }
      Kind::Punct("{") => self.object()?,
      Kind::Punct("[") => self.array()?,
      Kind::Punct("(") => return self.arrow_function(self.pos - 1),
      Kind::Ident => match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        "undefined" => return Ok(None),
        "require" => return self.require(),
        _ if self.at("=>") => return self.arrow_function(self.pos - 1),
        name => return self.binding(name),
      },
      _ => return Err(Unsupported),
    };
    Ok(Some(value))
  }

  /// `first` followed by any `+ '...'` joining more strings to it.
  fn concat(&mut self, mut first: String) -> Result<String, Unsupported> {
    while self.at("+") {
      match self.token(self.pos + 1).map(|t| &t.kind) {
        Some(Kind::Str(next) | Kind::Template(next)) => {
          first.push_str(next);
          self.pos += 2;
        }
        _ => return Err(Unsupported),
      }
    }
    Ok(first)
  }

  /// An object literal, the cursor just past its `{`.
  fn object(&mut self) -> Result<Value, Unsupported> {
    let mut map = Map::new();
    while !self.eat("}") {
      let entry = self.pos;
      let token = self.token(entry).ok_or(Unsupported)?;
      let key = match &token.kind {
        Kind::Ident | Kind::Number => Some(self.text(entry).to_string()),
        Kind::Str(key) => Some(key.clone()),
        _ => None,
      };
      let next = self.token(entry + 1);
      match key {
        Some(key) if next.is_some_and(|t| t.is(":")) => {
          self.pos += 2;
          if let Some(value) = self.expr() {
            map.insert(key, value);
          }
        }
        // Shorthand: `{ rules }`.
        Some(key) if token.kind == Kind::Ident && next.is_some_and(|t| t.is(",") || t.is("}")) => {
          self.pos += 1;
          match self.binding(self.text(entry)) {
            Ok(Some(value)) => {
              map.insert(key, value);
            }
            Ok(None) => {}
            Err(Unsupported) => self.skip(entry, self.pos),
          }
        }
        // A spread, computed key, method or accessor.
        _ => {
          self.eat("...");
          self.skip_expr();
          self.skip(entry, self.pos);
        }
      }
      if !self.eat(",") && !self.at("}") {
        return Err(Unsupported);
      }
    }
    Ok(Value::Object(map))
  }

  /// An array literal, the cursor just past its `[`.  An element that
  /// cannot be evaluated becomes `null`, keeping the others in place; a
  /// spread contributes nothing.
  fn array(&mut self) -> Result<Value, Unsupported> {
    let mut items = Vec::new();
    while !self.eat("]") {
      if self.at(",") {
        items.push(Value::Null);
      } else if self.at("...") {
        let start = self.pos;
        self.pos += 1;
        self.skip_expr();
        self.skip(start, self.pos);
      } else {
        items.push(self.expr().unwrap_or(Value::Null));
      }
      if !self.eat(",") && !self.at("]") {
        return Err(Unsupported);
      }
    }
    Ok(Value::Array(items))
  }

  /// A message function, `(name) => `...``, as the template with its
  /// parameter written `${name}`, which is how gale's custom messages stand
  /// for the first message argument.  `params` is the token where the
  /// parameter list starts.
  fn arrow_function(&mut self, params: usize) -> Result<Option<Value>, Unsupported> {
    let param = if self.tokens[params].is("(") {
      let close = self.matching_close(params).ok_or(Unsupported)?;
      self.pos = close + 1;
      self.src[self.tokens[params].end..self.tokens[close].start].trim()
    } else {
      self.text(params)
    };
    if !self.eat("=>") {
      return Err(Unsupported);
    }
    let Some(Kind::Template(body)) = self.token(self.pos).map(|t| t.kind.clone()) else {
      return Err(Unsupported);
    };
    self.pos += 1;
    let message = if param.is_empty() || param == "name" {
      body
    } else {
      body.replace(&format!("${{{param}}}"), "${name}")
    };
    Ok(Some(Value::String(message)))
  }

  /// The index of the bracket closing the one at `open`.
  fn matching_close(&self, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, token) in self.tokens.iter().enumerate().skip(open) {
      match token.kind {
        Kind::Punct("(" | "[" | "{") => depth += 1,
        Kind::Punct(")" | "]" | "}") => {
          depth -= 1;
          if depth == 0 {
            return Some(at);
          }
        }
        _ => {}
      }
    }
    None
  }

  /// `require('<module>')` or `require.resolve('<module>')`, the cursor
  /// just past `require`.
  fn require(&mut self) -> Result<Option<Value>, Unsupported> {
    let resolve = self.at(".") && self.ident(self.pos + 1) == Some("resolve");
    if resolve {
      self.pos += 2;
    }
    let spec = match (
      self.token(self.pos),
      self.token(self.pos + 1).map(|t| &t.kind),
      self.token(self.pos + 2),
    ) {
      (Some(open), Some(Kind::Str(spec)), Some(close)) if open.is("(") && close.is(")") => {
        spec.clone()
      }
      _ => return Err(Unsupported),
    };
    self.pos += 3;
    if resolve {
      return Ok(Some(Value::String(spec)));
    }
    Ok(Some(self.module(&spec)))
  }

  /// The value a top-level name was declared with.
  fn binding(&mut self, name: &'a str) -> Result<Option<Value>, Unsupported> {
    let binding = self.bindings.get(name).cloned().ok_or(Unsupported)?;
    if !self.evaluating.insert(name) {
      return Err(Unsupported);
    }
    let value = match binding {
      Binding::Init(at) => {
        let resume = std::mem::replace(&mut self.pos, at);
        let value = self.expr();
        self.pos = resume;
        value
      }
      Binding::Import(spec) => Some(self.module(&spec)),
    };
    self.evaluating.remove(name);
    Ok(value)
  }

  /// The exported value of the module `spec` names.
  ///
  /// A module gale cannot read stands for its name, which is what
  /// `plugins`, `customSyntax` and `extends` need: a plugin or syntax is
  /// code gale does not run.  So does a package whose export is not plain
  /// data; a relative file's export is used even if parts were skipped.
  fn module(&mut self, spec: &str) -> Value {
    let relative = spec.starts_with("./") || spec.starts_with("../");
    let path = self
      .dir
      .filter(|_| self.depth < MAX_IMPORT_DEPTH)
      .and_then(|dir| {
        if relative {
          resolve_import_path(dir, spec)
        } else {
          resolve_npm_package_path(dir, spec)
        }
      });
    match path.and_then(|path| read_module(&path, self.depth + 1).ok()) {
      Some(Reading {
        value: Some(value),
        skipped,
      }) if relative || skipped.is_empty() => {
        self.skipped.extend(skipped);
        value
      }
      _ => Value::String(spec.to_string()),
    }
  }
}

/// A numeric literal's value.  Whole numbers stay integers, so `0` reads
/// as `0` rather than `0.0`.
fn number(text: &str) -> Result<Value, Unsupported> {
  let (negative, digits) = match text.strip_prefix('-') {
    Some(rest) => (true, rest),
    None => (false, text),
  };
  let digits = digits.replace('_', "");
  let digits = digits.strip_suffix('n').unwrap_or(&digits);
  let radix = match digits.get(..2).map(str::to_ascii_lowercase).as_deref() {
    Some("0x") => Some(16),
    Some("0o") => Some(8),
    Some("0b") => Some(2),
    _ => None,
  };
  let value = match radix {
    Some(radix) => i64::from_str_radix(&digits[2..], radix).map_err(|_| Unsupported)? as f64,
    None => digits.parse::<f64>().map_err(|_| Unsupported)?,
  };
  let value = if negative { -value } else { value };
  if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
    Ok(Value::Number((value as i64).into()))
  } else {
    Number::from_f64(value)
      .map(Value::Number)
      .ok_or(Unsupported)
  }
}

/// The file a relative import names: the path as written, with a
/// JavaScript or JSON extension added, or a directory's `index.js`.
fn resolve_import_path(dir: &Path, spec: &str) -> Option<PathBuf> {
  let base = dir.join(spec);
  std::iter::once(base.clone())
    .chain(
      [".js", ".mjs", ".cjs", ".json"].map(|ext| PathBuf::from(format!("{}{ext}", base.display()))),
    )
    .chain(std::iter::once(base.join("index.js")))
    .find(|path| path.is_file())
}

/// The entry file of the npm package `spec` names, found in the nearest
/// `node_modules` that has it: its `exports`, then `main`, then an index
/// file, or for a subpath the file itself.
fn resolve_npm_package_path(dir: &Path, spec: &str) -> Option<PathBuf> {
  let (name, subpath) = split_npm_package_subpath(spec);
  dir.ancestors().find_map(|dir| {
    let package = dir.join("node_modules").join(name);
    if !package.is_dir() {
      return None;
    }
    let manifest = std::fs::read_to_string(package.join("package.json"))
      .ok()
      .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let from_manifest = manifest.as_ref().and_then(|manifest| {
      let exported = resolve_package_exports(manifest, subpath.unwrap_or("."));
      let main = subpath
        .is_none()
        .then(|| {
          manifest
            .get("main")
            .and_then(Value::as_str)
            .map(str::to_string)
        })
        .flatten();
      exported
        .into_iter()
        .chain(main)
        .map(|entry| package.join(entry))
        .find(|path| path.is_file())
    });
    from_manifest.or_else(|| match subpath {
      Some(subpath) => resolve_import_path(&package, &format!("./{subpath}")),
      None => ["index.js", "index.cjs", "index.mjs"]
        .iter()
        .map(|index| package.join(index))
        .find(|path| path.is_file()),
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The exported value of `source`, and what was skipped.
  fn read_js(source: &str, dir: Option<&Path>) -> (Value, Vec<String>) {
    let reading = read_source(source, dir, 0).unwrap();
    (reading.value.unwrap_or(Value::Null), reading.skipped)
  }

  #[test]
  fn constants_resolve_but_their_names_inside_strings_do_not() {
    let (value, skipped) = read_js(
      "const OFF = null;\nmodule.exports = { rules: { a: OFF, b: [true, { message: 'OFF means off' }] } };",
      None,
    );
    assert_eq!(
      value,
      serde_json::json!({ "rules": { "a": null, "b": [true, { "message": "OFF means off" }] } })
    );
    assert!(skipped.is_empty());
  }

  #[test]
  fn multibyte_text_around_constants_reads_without_panicking() {
    let (value, _) = read_js(
      "const OFF = null; // désactivé\nmodule.exports = { rules: { 'é': OFF, b: [true, { message: 'Empty — remove it “now”' }] } };",
      None,
    );
    assert_eq!(value["rules"]["é"], Value::Null);
    assert_eq!(value["rules"]["b"][1]["message"], "Empty — remove it “now”");
  }

  #[test]
  fn what_cannot_be_evaluated_is_skipped_and_named() {
    let (value, skipped) = read_js(
      r#"
const base = { "block-no-empty": true };
const rules = {};
rules["color-named"] = "never";
module.exports = {
  rules: { ...base, "a": process.env.X ? null : true, "b": ["x"].map((r) => r), "c": 1 },
  extends: ["x", path.resolve(__dirname, "y")],
};
"#,
      None,
    );
    assert_eq!(
      value,
      serde_json::json!({ "rules": { "c": 1 }, "extends": ["x", null] })
    );
    assert_eq!(
      skipped,
      vec![
        r#"rules["color-named"] = "never""#,
        "...base",
        "process.env.X ? null : true",
        r#"["x"].map((r) => r)"#,
        r#"path.resolve(__dirname, "y")"#,
      ]
    );
  }

  #[test]
  fn shorthand_properties_and_late_declarations_resolve() {
    let (value, skipped) = read_js(
      "export default config;\nconst rules = { a: 1 };\nconst config = { rules };",
      None,
    );
    assert_eq!(value, serde_json::json!({ "rules": { "a": 1 } }));
    assert!(skipped.is_empty());
  }

  #[test]
  fn regexes_keep_their_flags_and_numbers_their_type() {
    let (value, _) = read_js(
      "module.exports = { a: /^[a-z\\/]+$/i, b: [0, -1, 1.5, 0x10] };",
      None,
    );
    assert_eq!(value["a"], "/^[a-z\\/]+$/i");
    assert_eq!(value["b"], serde_json::json!([0, -1, 1.5, 16]));
  }

  #[test]
  fn modules_gale_cannot_read_stand_for_their_names() {
    let (value, _) = read_js(
      "import scss from 'postcss-scss';\nmodule.exports = { customSyntax: scss, plugins: [require('stylelint-order')], extends: require.resolve('./base') };",
      None,
    );
    assert_eq!(
      value,
      serde_json::json!({
        "customSyntax": "postcss-scss",
        "plugins": ["stylelint-order"],
        "extends": "./base",
      })
    );
  }

  #[test]
  fn a_file_without_an_export_is_an_error() {
    assert!(read_source("const config = { rules: {} };", None, 0).is_err());
  }

  #[test]
  fn imports_cycles_stop() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
      tmp.path().join("a.js"),
      "module.exports = require('./b');\n",
    )
    .unwrap();
    std::fs::write(
      tmp.path().join("b.js"),
      "module.exports = require('./a');\n",
    )
    .unwrap();
    let (value, _) = read_js("module.exports = { x: require('./a') };", Some(tmp.path()));
    assert!(value["x"].is_string());
  }
}
