//! Finding the files to lint from the paths and globs on the command line.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ignore::WalkBuilder;

use crate::{embedded, glob};

/// The style sheet extensions gale lints.
const CSS_EXTENSIONS: &[&str] = &["css", "scss", "less", "sass"];

/// Whether the path's extension is one Gale knows how to lint: a style
/// sheet, or an HTML-like file with style blocks in it.
fn is_lintable_file(path: &Path) -> bool {
  path
    .extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| CSS_EXTENSIONS.contains(&ext))
    || embedded::is_host_file(path)
}

/// The kind of source a path holds when it is a file that Stylelint lints
/// (through a `customSyntax`) but gale cannot parse yet: styles embedded in
/// Markdown or JavaScript.
///
/// This is the one list to shrink as gale learns to lint these files.
fn unsupported_kind(path: &Path) -> Option<&'static str> {
  let ext = path.extension()?.to_str()?.to_ascii_lowercase();
  Some(match ext.as_str() {
    "md" | "markdown" | "mdx" => "Markdown",
    "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => "CSS-in-JS",
    _ => return None,
  })
}

/// Why a file that matched the input was not linted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SkipReason {
  /// A file type gale cannot lint yet, named by [`unsupported_kind`] (or
  /// `None` for an explicitly named file of some other type).
  FileType(Option<&'static str>),
  /// A `customSyntax` gale cannot parse.
  CustomSyntax(String),
}

/// A file that matched the input but was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Skipped {
  pub(crate) path: PathBuf,
  pub(crate) reason: SkipReason,
}

/// What [`discover_files`] found: the files to lint and those it skipped.
#[derive(Debug, Default)]
pub(crate) struct Discovery {
  pub(crate) files: Vec<PathBuf>,
  pub(crate) skipped: Vec<Skipped>,
}

/// The single stderr warning listing every skipped file, grouped by reason,
/// or `None` when nothing was skipped.
///
/// At most a handful of paths are named per reason, so a glob that sweeps
/// up hundreds of `.vue` files still prints a short warning.
pub(crate) fn skipped_files_warning(skipped: &[Skipped]) -> Option<String> {
  const NAMED_PER_REASON: usize = 5;
  if skipped.is_empty() {
    return None;
  }
  let mut by_reason: std::collections::BTreeMap<&SkipReason, Vec<String>> =
    std::collections::BTreeMap::new();
  for skip in skipped {
    by_reason
      .entry(&skip.reason)
      .or_default()
      .push(skip.path.display().to_string());
  }

  let count = skipped.len();
  let noun = if count == 1 { "file" } else { "files" };
  let mut out = format!("warning: Skipped {count} {noun} that gale cannot lint yet:");
  for (reason, mut paths) in by_reason {
    paths.sort();
    let label = match reason {
      SkipReason::FileType(Some(kind)) => format!("{kind} files are not supported"),
      SkipReason::FileType(None) => {
        "not a CSS, SCSS, Less, Sass, Vue, Svelte, Astro or HTML file".to_string()
      }
      SkipReason::CustomSyntax(name) => format!("customSyntax \"{name}\" is not supported"),
    };
    let more = paths.len().saturating_sub(NAMED_PER_REASON);
    paths.truncate(NAMED_PER_REASON);
    let mut names = paths.join(", ");
    if more > 0 {
      names.push_str(&format!(" and {more} more"));
    }
    out.push_str(&format!("\n  {names} ({label})"));
  }
  Some(out)
}

/// How a discovery treats ignore files and patterns.
pub(crate) struct DiscoverOptions<'a> {
  /// `--no-ignore`: read no ignore files and apply no ignore patterns to
  /// walks.
  pub(crate) no_ignore: bool,
  /// `--ignore-path`: one more ignore file.
  pub(crate) ignore_path: Option<&'a Path>,
  /// The root config's `ignoreFiles` and the `--ignore-pattern` globs.
  pub(crate) ignore_patterns: &'a [String],
  /// Stylelint's `--disable-default-ignores`: walk into `node_modules`.
  pub(crate) disable_default_ignores: bool,
}

/// Build an `ignore::gitignore::Gitignore` matcher from `.stylelintignore`,
/// `.galeignore`, and an optional user-supplied ignore file in the cwd.
///
/// This is used to filter explicitly-passed files, matching Stylelint's
/// behaviour of always respecting `.stylelintignore` even for explicit paths.
fn build_explicit_ignore_matcher(
  user_ignore: Option<&Path>,
) -> Option<ignore::gitignore::Gitignore> {
  let cwd = std::env::current_dir().ok()?;
  let mut builder = ignore::gitignore::GitignoreBuilder::new(&cwd);

  // Load .stylelintignore from cwd (if it exists).
  let stylelintignore = cwd.join(".stylelintignore");
  if stylelintignore.is_file() {
    builder.add(&stylelintignore);
  }

  // Load .galeignore from cwd (if it exists).
  let galeignore = cwd.join(".galeignore");
  if galeignore.is_file() {
    builder.add(&galeignore);
  }

  // Load user-supplied ignore file.
  if let Some(path) = user_ignore
    && path.is_file()
  {
    builder.add(path);
  }

  builder.build().ok()
}

/// What decides that a file found by a glob, or named outright, is ignored:
/// the ignore files in the working directory and the config's ignore
/// patterns, which Stylelint applies to explicit paths too.
struct Filters {
  /// `.stylelintignore`, `.galeignore` and `--ignore-path`; `None` under
  /// `--no-ignore`.
  ignore_files: Option<ignore::gitignore::Gitignore>,
  /// The ignore patterns, matched against paths relative to `cwd`.
  ignore_patterns: Vec<globset::GlobMatcher>,
  cwd: PathBuf,
}

impl Filters {
  /// The filters `opts` describe.
  fn new(opts: &DiscoverOptions<'_>) -> Self {
    let ignore_files = if opts.no_ignore {
      None
    } else {
      build_explicit_ignore_matcher(opts.ignore_path)
    };
    // Stylelint's ignoreFiles patterns are relative to the config, usually
    // the project root: a leading `./` adds nothing.
    let ignore_patterns = opts
      .ignore_patterns
      .iter()
      .filter_map(|pat| {
        let clean = pat.strip_prefix("./").unwrap_or(pat);
        globset::Glob::new(clean).ok().map(|g| g.compile_matcher())
      })
      .collect();
    Self {
      ignore_files,
      ignore_patterns,
      cwd: std::env::current_dir().unwrap_or_default(),
    }
  }

  /// Whether `path` is ignored.  The ignore files match the path or any of
  /// its parents, so `packages/theme/src/prebuilt/x.css` is ignored by a
  /// pattern `packages/theme/src/prebuilt`.
  fn ignores(&self, path: &Path) -> bool {
    if let Some(ignore_files) = &self.ignore_files
      && ignore_files
        .matched_path_or_any_parents(path, false)
        .is_ignore()
    {
      return true;
    }
    if self.ignore_patterns.is_empty() {
      return false;
    }
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let rel = abs.strip_prefix(&self.cwd).unwrap_or(&abs);
    self.ignore_patterns.iter().any(|m| m.is_match(rel))
  }
}

/// A walker rooted at `root` that honours `opts`: Stylelint's ignore files
/// (never `.gitignore`, which Stylelint does not read), `--ignore-path`,
/// the ignore patterns, and the default `node_modules` exclusion.
///
/// `warn_missing_ignore_file` reports an `--ignore-path` that does not
/// exist, which a walk of a directory argument does and a glob's does not.
fn walker(root: &Path, opts: &DiscoverOptions<'_>, warn_missing_ignore_file: bool) -> WalkBuilder {
  let mut builder = WalkBuilder::new(root);
  // Show hidden CSS files (e.g. .hidden.css) but respect ignore files.
  builder.hidden(false);
  builder.git_ignore(false);
  builder.git_global(false);
  builder.git_exclude(false);
  if opts.no_ignore {
    return builder;
  }

  builder.add_custom_ignore_filename(".galeignore");
  builder.add_custom_ignore_filename(".stylelintignore");
  if let Some(ignore_file) = opts.ignore_path {
    if ignore_file.exists() {
      if let Some(err) = builder.add_ignore(ignore_file) {
        eprintln!(
          "Warning: failed to load ignore file {}: {err}",
          ignore_file.display()
        );
      }
    } else if warn_missing_ignore_file {
      eprintln!("Warning: ignore file not found: {}", ignore_file.display());
    }
  }

  let mut overrides = ignore::overrides::OverrideBuilder::new(root);
  if !opts.disable_default_ignores {
    let _ = overrides.add("!**/node_modules/**");
  }
  for pat in opts.ignore_patterns {
    // Negated, so matching files are excluded.
    if let Err(err) = overrides.add(&format!("!{pat}")) {
      eprintln!("Warning: invalid ignore pattern '{pat}': {err}");
    }
  }
  if let Ok(built) = overrides.build() {
    builder.overrides(built);
  }
  builder
}

/// Expands the given paths into a deduplicated file list, walking directories
/// and dropping anything matched by ignore files or config ignore patterns.
///
/// Files the input names or a glob matches that gale cannot lint yet (a
/// `.md` file, say) are returned as skipped rather than dropped silently.
/// Directory walks only ever pick up files gale lints.
pub(crate) fn discover_files(paths: &[String], opts: &DiscoverOptions<'_>) -> Discovery {
  let mut files: Vec<PathBuf> = Vec::new();
  let mut skipped: Vec<Skipped> = Vec::new();
  let filters = Filters::new(opts);

  for pattern in glob::expand(paths) {
    if glob::is_glob_pattern(&pattern) {
      let matcher = match glob::compile(&pattern) {
        Ok(matcher) => matcher,
        Err(err) => {
          eprintln!("Warning: invalid glob pattern '{pattern}': {err}");
          continue;
        }
      };
      let (matched, skipped_here) = walk_glob(&matcher, &glob::walk_root(&pattern), opts, &filters);
      files.extend(matched);
      skipped.extend(skipped_here);
      continue;
    }

    let path = Path::new(&pattern);
    if path.is_file() {
      if filters.ignores(path) {
        continue;
      }
      if is_lintable_file(path) {
        files.push(path.to_path_buf());
      } else {
        // A file named outright is one the user expects linted: say why
        // it was not, whatever its type.
        skipped.push(Skipped {
          path: path.to_path_buf(),
          reason: SkipReason::FileType(unsupported_kind(path)),
        });
      }
      continue;
    }

    files.extend(walk_dir(path, opts));
  }

  // A file that several arguments match is linted and reported once, at
  // its first match, as Stylelint's globby does.
  let mut seen = std::collections::HashSet::new();
  files.retain(|file| seen.insert(without_cur_dir(file)));

  Discovery { files, skipped }
}

/// The files under `root` that `matcher` matches and the filters let
/// through, and those of them gale cannot lint yet, each breadth first.
///
/// The walk honours the same ignore files and patterns as a directory walk
/// (and `node_modules` exclusion); the glob is a filter on what it finds.
fn walk_glob(
  matcher: &globset::GlobMatcher,
  root: &Path,
  opts: &DiscoverOptions<'_>,
  filters: &Filters,
) -> (Vec<PathBuf>, Vec<Skipped>) {
  let matched_files = Mutex::new(Vec::new());
  let skipped_files = Mutex::new(Vec::new());
  walker(root, opts, false).build_parallel().run(|| {
    Box::new(|entry_result| {
      let Ok(entry) = entry_result else {
        return ignore::WalkState::Continue;
      };
      if !entry.file_type().is_some_and(|ft| ft.is_file()) {
        return ignore::WalkState::Continue;
      }
      let entry_path = entry.path();
      let unsupported = if is_lintable_file(entry_path) {
        None
      } else {
        match unsupported_kind(entry_path) {
          Some(kind) => Some(kind),
          None => return ignore::WalkState::Continue,
        }
      };
      if !matcher.is_match(entry_path) {
        let rel = entry_path.strip_prefix(&filters.cwd).unwrap_or(entry_path);
        // A walk rooted at "." yields paths like "./a.css".  A bare
        // pattern such as `*.css` carries no "./" and `*` does not cross a
        // separator, so it would never match without stripping the prefix
        // first.
        let bare = rel.strip_prefix("./").unwrap_or(rel);
        if !matcher.is_match(rel) && !matcher.is_match(bare) {
          return ignore::WalkState::Continue;
        }
      }
      if filters.ignores(entry_path) {
        return ignore::WalkState::Continue;
      }

      match unsupported {
        Some(kind) => skipped_files
          .lock()
          .unwrap_or_else(|e| e.into_inner())
          .push(Skipped {
            path: entry_path.to_path_buf(),
            reason: SkipReason::FileType(Some(kind)),
          }),
        None => matched_files
          .lock()
          .unwrap_or_else(|e| e.into_inner())
          .push(entry_path.to_path_buf()),
      }
      ignore::WalkState::Continue
    })
  });
  let mut matched = matched_files.into_inner().unwrap_or_default();
  breadth_first(&mut matched);
  let mut skipped = skipped_files.into_inner().unwrap_or_default();
  skipped.sort_by(|a, b| breadth_first_order(&a.path, &b.path));
  (matched, skipped)
}

/// The files gale lints under the directory `dir`, breadth first.
fn walk_dir(dir: &Path, opts: &DiscoverOptions<'_>) -> Vec<PathBuf> {
  let found = Mutex::new(Vec::new());
  walker(dir, opts, true).build_parallel().run(|| {
    Box::new(|entry_result| {
      if let Ok(entry) = entry_result
        && entry.file_type().is_some_and(|ft| ft.is_file())
        && is_lintable_file(entry.path())
      {
        found
          .lock()
          .unwrap_or_else(|e| e.into_inner())
          .push(entry.path().to_path_buf());
      }
      ignore::WalkState::Continue
    })
  });
  let mut found = found.into_inner().unwrap_or_default();
  breadth_first(&mut found);
  found
}

/// Order one walk's files the way Stylelint's glob lists them: breadth
/// first, so a directory's own files come before anything in its
/// subdirectories, and by path within a depth.
///
/// The walk itself runs in parallel and yields files in whatever order its
/// threads reach them; without this the report order changed between runs.
/// Stylelint's own order within a depth depends on when each directory read
/// completes, so the path order there is the deterministic stand-in.
fn breadth_first(files: &mut [PathBuf]) {
  files.sort_by(|a, b| breadth_first_order(a, b));
}

/// The comparison behind [`breadth_first`]: fewer path components first,
/// then by path.
fn breadth_first_order(a: &Path, b: &Path) -> std::cmp::Ordering {
  a.components()
    .count()
    .cmp(&b.components().count())
    .then_with(|| a.cmp(b))
}

/// `path` without `.` components, so `./a.css` and `a.css` count as the
/// same file.
fn without_cur_dir(path: &Path) -> PathBuf {
  path
    .components()
    .filter(|c| !matches!(c, std::path::Component::CurDir))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  /// Helper: create a temporary directory tree with CSS files for glob tests.
  fn create_test_tree(base: &Path) {
    let sub = base.join("sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(base.join("a.css"), "a {}").unwrap();
    fs::write(base.join("b.scss"), "$x: 1;").unwrap();
    fs::write(base.join("c.txt"), "not css").unwrap();
    fs::write(sub.join("d.css"), "d {}").unwrap();
    fs::write(sub.join("e.less"), ".e {}").unwrap();
  }

  fn default_opts() -> DiscoverOptions<'static> {
    DiscoverOptions {
      no_ignore: true,
      ignore_path: None,
      ignore_patterns: &[],
      disable_default_ignores: false,
    }
  }

  #[test]
  fn unsupported_files_named_or_globbed_are_skipped_not_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.css"), "a {}").unwrap();
    fs::write(tmp.path().join("b.md"), "```css\na {}\n```").unwrap();
    fs::write(tmp.path().join("c.txt"), "notes").unwrap();
    fs::write(tmp.path().join("d.vue"), "<style>a {}</style>").unwrap();

    // Named outright: skipped with a reason, whatever the type.
    let named = discover_files(
      &[
        tmp.path().join("b.md").display().to_string(),
        tmp.path().join("c.txt").display().to_string(),
      ],
      &default_opts(),
    );
    assert!(named.files.is_empty());
    let reasons: Vec<&SkipReason> = named.skipped.iter().map(|s| &s.reason).collect();
    assert_eq!(
      reasons,
      vec![
        &SkipReason::FileType(Some("Markdown")),
        &SkipReason::FileType(None)
      ]
    );

    // Matched by a glob: only the kinds Stylelint lints are reported.
    let globbed = discover_files(&[format!("{}/*", tmp.path().display())], &default_opts());
    assert_eq!(globbed.files.len(), 2);
    assert_eq!(globbed.skipped.len(), 1);
    assert!(globbed.skipped[0].path.ends_with("b.md"));

    // A directory walk only ever looks at files gale lints.
    let walked = discover_files(&[tmp.path().display().to_string()], &default_opts());
    assert_eq!(walked.files.len(), 2);
    assert!(walked.skipped.is_empty());
  }

  #[test]
  fn html_like_files_are_linted_when_named_globbed_or_walked() {
    let tmp = tempfile::tempdir().unwrap();
    let names = [
      "a.vue", "b.svelte", "c.astro", "d.html", "e.htm", "f.php", "g.xhtml",
    ];
    for name in names {
      fs::write(tmp.path().join(name), "<style>a {}</style>").unwrap();
    }
    let named: Vec<String> = names
      .iter()
      .map(|n| tmp.path().join(n).display().to_string())
      .collect();
    let found = discover_files(&named, &default_opts());
    assert_eq!(found.files.len(), names.len());
    assert!(found.skipped.is_empty());

    let globbed = discover_files(
      &[format!(
        "{}/**/*.{{vue,svelte,astro}}",
        tmp.path().display()
      )],
      &default_opts(),
    );
    assert_eq!(globbed.files.len(), 3);

    let walked = discover_files(&[tmp.path().display().to_string()], &default_opts());
    assert_eq!(walked.files.len(), names.len());
  }

  #[test]
  fn skipped_files_warning_groups_by_reason() {
    let skip = |path: &str, reason: SkipReason| Skipped {
      path: PathBuf::from(path),
      reason,
    };
    let mut skipped: Vec<Skipped> = (1..=7)
      .map(|i| skip(&format!("c{i}.md"), SkipReason::FileType(Some("Markdown"))))
      .collect();
    skipped.push(skip(
      "x.css",
      SkipReason::CustomSyntax("postcss-lit".into()),
    ));
    assert_eq!(
      skipped_files_warning(&skipped).unwrap(),
      "warning: Skipped 8 files that gale cannot lint yet:\n  \
       c1.md, c2.md, c3.md, c4.md, c5.md and 2 more (Markdown files are not supported)\n  \
       x.css (customSyntax \"postcss-lit\" is not supported)"
    );
    assert_eq!(skipped_files_warning(&[]), None);
    assert_eq!(
      skipped_files_warning(&[skip("b.jsx", SkipReason::FileType(Some("CSS-in-JS")))]).unwrap(),
      "warning: Skipped 1 file that gale cannot lint yet:\n  b.jsx (CSS-in-JS files are not supported)"
    );
  }

  #[test]
  fn test_glob_star_css() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/*.css", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts()).files;

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("a.css"));
  }

  #[test]
  fn test_glob_recursive_double_star() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/**/*.css", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts()).files;

    // Should find a.css and sub/d.css
    assert_eq!(files.len(), 2);
    let names: Vec<String> = files
      .iter()
      .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
      .collect();
    assert!(names.contains(&"a.css".to_string()));
    assert!(names.contains(&"d.css".to_string()));
  }

  #[test]
  fn discovered_files_are_breadth_first_in_argument_order_and_unique() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for file in [
      "z.css",
      "a/b/deep.css",
      "a/x.css",
      "a/b.css",
      "m.css",
      "b/y.css",
      "a/b/c/deeper.css",
    ] {
      let path = root.join(file);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(&path, "a {}").unwrap();
    }
    let rel = |files: &[PathBuf]| -> Vec<String> {
      files
        .iter()
        .map(|p| p.strip_prefix(root).unwrap().display().to_string())
        .collect()
    };

    // The walk is parallel, so check the order holds run after run.
    let glob = format!("{}/**/*.css", root.display());
    for _ in 0..5 {
      let files = discover_files(std::slice::from_ref(&glob), &default_opts()).files;
      assert_eq!(
        rel(&files),
        [
          "m.css",
          "z.css",
          "a/b.css",
          "a/x.css",
          "b/y.css",
          "a/b/deep.css",
          "a/b/c/deeper.css",
        ]
      );
    }

    // Arguments keep their order, and a file several of them match is
    // listed once, where it first appears.
    let explicit = root.join("z.css").display().to_string();
    let dir = root.join("a").display().to_string();
    let files = discover_files(&[explicit, dir, glob], &default_opts()).files;
    assert_eq!(
      rel(&files),
      [
        "z.css",
        "a/b.css",
        "a/x.css",
        "a/b/deep.css",
        "a/b/c/deeper.css",
        "m.css",
        "b/y.css",
      ]
    );
  }

  #[test]
  fn test_glob_scss_only() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/**/*.scss", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts()).files;

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("b.scss"));
  }

  #[test]
  fn test_glob_no_match_returns_empty() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/**/*.sass", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts()).files;

    assert!(files.is_empty());
  }

  #[test]
  fn test_glob_mixed_with_directory_arg() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    // Mix a glob pattern with a plain directory argument.
    let glob_pat = format!("{}/*.scss", tmp.path().display());
    let dir_arg = tmp.path().join("sub").display().to_string();
    let files = discover_files(&[glob_pat, dir_arg], &default_opts()).files;

    // Glob should find b.scss; directory walk should find d.css + e.less.
    assert_eq!(files.len(), 3);
  }

  #[test]
  fn test_non_glob_file_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let file_arg = tmp.path().join("a.css").display().to_string();
    let files = discover_files(&[file_arg], &default_opts()).files;

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("a.css"));
  }

  #[test]
  fn test_non_glob_directory_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let dir_arg = tmp.path().display().to_string();
    let files = discover_files(&[dir_arg], &default_opts()).files;

    // Should find all CSS files recursively: a.css, b.scss, sub/d.css, sub/e.less
    assert_eq!(files.len(), 4);
  }

  #[test]
  fn test_glob_brace_expansion_css_scss() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    // Brace expansion: should find both .css and .scss files recursively.
    let pattern = format!("{}/**/*.{{css,scss}}", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts()).files;

    // a.css, b.scss, sub/d.css (not sub/e.less)
    assert_eq!(files.len(), 3);
  }

  #[test]
  fn node_modules_is_walked_only_with_disable_default_ignores() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.css"), "a {}").unwrap();
    let nm = tmp.path().join("node_modules").join("pkg");
    std::fs::create_dir_all(&nm).unwrap();
    std::fs::write(nm.join("x.css"), "a {}").unwrap();
    let pattern = format!("{}/**/*.css", tmp.path().display());

    let default = discover_files(
      std::slice::from_ref(&pattern),
      &DiscoverOptions {
        no_ignore: false,
        ..default_opts()
      },
    )
    .files;
    assert_eq!(default.len(), 1);

    let lifted = discover_files(
      std::slice::from_ref(&pattern),
      &DiscoverOptions {
        no_ignore: false,
        disable_default_ignores: true,
        ..default_opts()
      },
    )
    .files;
    assert_eq!(lifted.len(), 2);
  }

  #[test]
  fn ignore_patterns_exclude_matching_files_from_a_walk() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());
    let pattern = format!("{}/**/*.css", tmp.path().display());
    let patterns = vec!["**/sub/**".to_string()];

    let files = discover_files(
      std::slice::from_ref(&pattern),
      &DiscoverOptions {
        no_ignore: false,
        ignore_patterns: &patterns,
        ..default_opts()
      },
    )
    .files;
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("a.css"));
  }

  #[test]
  fn test_extglob_file_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    // Use extglob pattern +(css|scss) to find both CSS and SCSS files.
    let pattern = format!("{}/**/*.+(css|scss)", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts()).files;

    // Should find a.css, b.scss, sub/d.css (not c.txt or sub/e.less)
    assert_eq!(files.len(), 3);
    let names: Vec<String> = files
      .iter()
      .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
      .collect();
    assert!(names.contains(&"a.css".to_string()));
    assert!(names.contains(&"b.scss".to_string()));
    assert!(names.contains(&"d.css".to_string()));
  }

  #[test]
  fn test_extglob_bare_parens_file_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    // Create directory structure with two source dirs
    let src = tmp.path().join("src");
    let docs = tmp.path().join("docs");
    let other = tmp.path().join("other");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&docs).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::write(src.join("a.css"), "a {}").unwrap();
    fs::write(docs.join("b.scss"), "$x: 1;").unwrap();
    fs::write(other.join("c.css"), "c {}").unwrap();

    // Pattern: (src|docs)/**/*.+(css|scss) — should find files in src/ and docs/ only
    let pattern = format!("{}/(src|docs)/**/*.+(css|scss)", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts()).files;

    assert_eq!(files.len(), 2);
    let names: Vec<String> = files
      .iter()
      .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
      .collect();
    assert!(names.contains(&"a.css".to_string()));
    assert!(names.contains(&"b.scss".to_string()));
    // c.css in other/ should NOT be included
    assert!(!names.contains(&"c.css".to_string()));
  }

  #[test]
  fn nested_alternatives_and_explicit_ignores_hold() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());
    let root = tmp.path().display();
    let names = |files: &[PathBuf]| -> Vec<String> {
      files
        .iter()
        .map(|p| p.strip_prefix(tmp.path()).unwrap().display().to_string())
        .collect()
    };

    // A group inside a path-level group is left to globset.
    let pattern = format!("{root}/{{sub/{{d,e}},a}}.*");
    let found = discover_files(&[pattern], &default_opts()).files;
    assert_eq!(names(&found), ["sub/d.css", "sub/e.less", "a.css"]);

    // An ignore pattern drops a file named outright, too.
    let patterns = vec!["**/a.css".to_string()];
    let opts = DiscoverOptions {
      ignore_patterns: &patterns,
      ..default_opts()
    };
    let named = tmp.path().join("a.css").display().to_string();
    assert!(discover_files(&[named], &opts).files.is_empty());
  }
}
