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
