//! Finding the file a module specifier names, the way Node.js `require`
//! does: a relative path with an extension or an index file added, or an
//! npm package found in the nearest `node_modules` that has it.
//!
//! `extends` and the static JavaScript reader both resolve modules, and
//! each tries its own list of candidate files.  The walk itself lives here
//! so the two cannot drift apart; each caller passes the candidates it
//! tries and decides which candidate is the module.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Why a path is a candidate, so a caller can treat some kinds of
/// candidate differently (`extends` loads some without checking that they
/// are files first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateKind {
  /// The path as the specifier wrote it.
  AsWritten,
  /// The path with one of the caller's extensions appended.
  WithExtension,
  /// One of the caller's index files inside the path, when it is a
  /// directory, or inside a package its manifest does not resolve.
  Index,
  /// The file a package's `exports` field names.
  Export,
  /// The file a package's `main` field names.
  Main,
}

/// One file a module specifier may name.
pub(crate) struct Candidate {
  pub path: PathBuf,
  pub kind: CandidateKind,
}

/// The files a path is tried as: itself, then with each extension, then
/// each index file inside it.
pub(crate) struct PathCandidates<'a> {
  pub extensions: &'a [&'a str],
  pub index_files: &'a [&'a str],
}

/// The files an npm package specifier is tried as, after the ones its
/// `package.json` names.
pub(crate) struct PackageCandidates<'a> {
  /// What a subpath (`pkg/sub`) is tried as, inside the package.
  pub subpath: PathCandidates<'a>,
  /// The files tried for the package itself after its `exports` and `main`.
  pub index_files: &'a [&'a str],
}

/// The candidates for `spec` relative to `dir`, in the order they are
/// tried.  Index files are only candidates when the path is a directory.
pub(crate) fn path_candidates<'a>(
  dir: &Path,
  spec: &str,
  candidates: &'a PathCandidates<'a>,
) -> impl Iterator<Item = Candidate> + 'a {
  let base = dir.join(spec);
  let with_extensions = candidates.extensions.iter().map({
    let base = base.clone();
    move |ext| Candidate {
      path: with_suffix(&base, ext),
      kind: CandidateKind::WithExtension,
    }
  });
  let index_dir = base.clone();
  let index_files = candidates
    .index_files
    .iter()
    .filter(move |_| index_dir.is_dir())
    .map({
      let base = base.clone();
      move |index| Candidate {
        path: base.join(index),
        kind: CandidateKind::Index,
      }
    });
  std::iter::once(Candidate {
    path: base,
    kind: CandidateKind::AsWritten,
  })
  .chain(with_extensions)
  .chain(index_files)
}

/// The candidates for the npm package `spec` (`pkg` or `pkg/sub`), from
/// `dir` up through every ancestor's `node_modules` that has the package,
/// so a package nested in another's `node_modules` (such as
/// `stylelint-config-recommended-scss` inside
/// `stylelint-config-standard-scss/node_modules/`) resolves the way Node.js
/// resolves it.
///
/// Within a package: the file its `exports` names, then (for the package
/// itself, not a subpath) its `main`, then the caller's files.
pub(crate) fn package_candidates<'a>(
  dir: &'a Path,
  spec: &'a str,
  candidates: &'a PackageCandidates<'a>,
) -> impl Iterator<Item = Candidate> + 'a {
  let (name, subpath) = split_npm_package_subpath(spec);
  dir
    .ancestors()
    .map(move |dir| dir.join("node_modules").join(name))
    .filter(|package| package.is_dir())
    .flat_map(move |package| {
      let manifest = std::fs::read_to_string(package.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
      let exported = manifest
        .as_ref()
        .and_then(|manifest| resolve_package_exports(manifest, subpath.unwrap_or(".")))
        .map(|entry| Candidate {
          path: package.join(entry),
          kind: CandidateKind::Export,
        });
      let main = manifest
        .as_ref()
        .filter(|_| subpath.is_none())
        .and_then(|manifest| manifest.get("main").and_then(Value::as_str))
        .map(|entry| Candidate {
          path: package.join(entry),
          kind: CandidateKind::Main,
        });
      let files: Box<dyn Iterator<Item = Candidate>> = match subpath {
        Some(subpath) => Box::new(path_candidates(&package, subpath, &candidates.subpath)),
        None => Box::new(candidates.index_files.iter().map({
          let package = package.clone();
          move |index| Candidate {
            path: package.join(index),
            kind: CandidateKind::Index,
          }
        })),
      };
      exported.into_iter().chain(main).chain(files)
    })
}

/// `path` with `suffix` appended to its last component, so `./base` with
/// `.js` is `./base.js` even when the name already has a dot.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
  let mut name = OsString::from(path.as_os_str());
  name.push(suffix);
  PathBuf::from(name)
}

/// Resolve a subpath from a package.json `exports` field.
///
/// Handles these `exports` shapes:
/// - `"exports": { "./sub": "./sub.mjs" }` (subpath → string)
/// - `"exports": { "./sub": { "default": "./sub.mjs" } }` (subpath → condition map)
/// - `"exports": { ".": "./index.mjs" }` (main entry point)
/// - `"exports": "./index.mjs"` (sugar for `{ ".": "./index.mjs" }`)
///
/// `subpath` should be the bare subpath (e.g. `"stylelint"` not `"./stylelint"`).
/// The special value `"."` resolves the package main entry point.
pub(crate) fn resolve_package_exports(pkg: &serde_json::Value, subpath: &str) -> Option<String> {
  let exports = pkg.get("exports")?;

  // Normalise the lookup key: `"."` stays as-is, otherwise prepend `"./"`
  let key = if subpath == "." {
    ".".to_string()
  } else {
    format!("./{subpath}")
  };

  // `exports` can be a string (sugar for `{ ".": "<string>" }`)
  if let Some(s) = exports.as_str() {
    if key == "." {
      return Some(s.to_string());
    }
    return None;
  }

  let exports_obj = exports.as_object()?;
  let entry = exports_obj.get(&key)?;

  // The entry can be a string or a condition map
  if let Some(s) = entry.as_str() {
    return Some(s.to_string());
  }

  // Condition map: try "default", "require", "import" in that order
  if let Some(obj) = entry.as_object() {
    for condition in &["default", "require", "import"] {
      if let Some(val) = obj.get(*condition).and_then(|v| v.as_str()) {
        return Some(val.to_string());
      }
    }
  }

  None
}

/// Split an npm package name into the package name and optional subpath.
///
/// Examples:
/// - `"@scope/pkg/sub/path"` → `("@scope/pkg", Some("sub/path"))`
/// - `"@scope/pkg"` → `("@scope/pkg", None)`
/// - `"pkg/sub"` → `("pkg", Some("sub"))`
/// - `"pkg"` → `("pkg", None)`
pub(crate) fn split_npm_package_subpath(name: &str) -> (&str, Option<&str>) {
  if let Some(rest) = name.strip_prefix('@') {
    // Scoped package: @scope/pkg[/subpath]
    // Find the second `/` (after the scope)
    if let Some(slash1) = rest.find('/') {
      let after_scope = &rest[slash1 + 1..];
      if let Some(slash2) = after_scope.find('/') {
        let pkg_end = 1 + slash1 + 1 + slash2; // "@" + "scope/" + "pkg"
        return (&name[..pkg_end], Some(&name[pkg_end + 1..]));
      }
    }
    (name, None)
  } else {
    // Unscoped package: pkg[/subpath]
    if let Some(slash) = name.find('/') {
      (&name[..slash], Some(&name[slash + 1..]))
    } else {
      (name, None)
    }
  }
}
