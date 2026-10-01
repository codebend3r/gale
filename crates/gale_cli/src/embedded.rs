//! The CLI's side of linting style sheets embedded in HTML-like files
//! (`.vue`, `.svelte`, `.astro`, `.html`, `.htm`, `.xhtml`, `.php`).
//!
//! Such a file is linted whatever its `customSyntax`, as Stylelint lints it
//! with `customSyntax: "postcss-html"`.  Each style block goes through the
//! same lint call a standalone file would, so the file's config, overrides
//! and options all apply.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gale_css_parser::Syntax;
use gale_css_parser::embedded::HostLanguage;
use gale_diagnostics::LintResult;
use gale_linter::embedded::{HostOptions, lint_host, unsupported_blocks};

/// Stylelint's `ignoreDisables` for the run, which also stops a
/// `stylelint-disable` comment in one block from reaching the next.
static IGNORE_DISABLES: AtomicBool = AtomicBool::new(false);

/// Record the run's `ignoreDisables` setting.
pub(crate) fn set_ignore_disables(ignore: bool) {
  IGNORE_DISABLES.store(ignore, Ordering::Relaxed);
}

/// Whether `path` is a file gale reads embedded style sheets from.
pub(crate) fn is_host_file(path: &Path) -> bool {
  path.to_str().and_then(HostLanguage::from_path).is_some()
}

/// Whether a `customSyntax` names postcss-html, the syntax Stylelint lints
/// these files with.
pub(crate) fn is_postcss_html(custom_syntax: &str) -> bool {
  custom_syntax.eq_ignore_ascii_case("postcss-html")
}

/// Lint `source` with `lint_css`: as one style sheet, or, for an HTML-like
/// file, once per embedded style block with the problems moved to their
/// place in the file.
pub(crate) fn lint(
  source: &str,
  file_path: &str,
  syntax: Syntax,
  mut lint_css: impl FnMut(&str, Syntax) -> LintResult,
) -> LintResult {
  match HostLanguage::from_path(file_path) {
    Some(host) => {
      let options = HostOptions {
        ignore_disables: IGNORE_DISABLES.load(Ordering::Relaxed),
      };
      lint_host(source, file_path, host, options, lint_css)
    }
    None => lint_css(source, syntax),
  }
}

/// A style block gale skipped because it cannot parse its language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkippedBlock {
  pub path: PathBuf,
  pub lang: String,
}

/// The style blocks in `source`, the text of the file at `path`, written in
/// a language gale cannot parse.
pub(crate) fn skipped_blocks(path: &Path, source: &str) -> Vec<SkippedBlock> {
  let Some(host) = path.to_str().and_then(HostLanguage::from_path) else {
    return Vec::new();
  };
  unsupported_blocks(source, host)
    .into_iter()
    .map(|lang| SkippedBlock {
      path: path.to_path_buf(),
      lang,
    })
    .collect()
}

/// The skipped style blocks among `files`, read from disk.
pub(crate) fn skipped_blocks_in(files: &[PathBuf]) -> Vec<SkippedBlock> {
  files
    .iter()
    .filter(|file| is_host_file(file))
    .filter_map(|file| {
      let source = std::fs::read_to_string(file).ok()?;
      Some(skipped_blocks(file, &source))
    })
    .flatten()
    .collect()
}

/// The single stderr warning about style blocks gale skipped, grouped by
/// language, or `None` when there are none.  A handful of files are named
/// per language.
pub(crate) fn skipped_blocks_warning(skipped: &[SkippedBlock]) -> Option<String> {
  const NAMED_PER_LANG: usize = 5;
  if skipped.is_empty() {
    return None;
  }
  let mut by_lang: std::collections::BTreeMap<&str, std::collections::BTreeSet<String>> =
    std::collections::BTreeMap::new();
  for block in skipped {
    by_lang
      .entry(block.lang.as_str())
      .or_default()
      .insert(block.path.display().to_string());
  }
  let count = skipped.len();
  let noun = if count == 1 { "block" } else { "blocks" };
  let mut out = format!("warning: Skipped {count} <style> {noun} that gale cannot lint yet:");
  for (lang, paths) in by_lang {
    let more = paths.len().saturating_sub(NAMED_PER_LANG);
    let mut names = paths
      .into_iter()
      .take(NAMED_PER_LANG)
      .collect::<Vec<_>>()
      .join(", ");
    if more > 0 {
      names.push_str(&format!(" and {more} more"));
    }
    out.push_str(&format!("\n  {names} (lang=\"{lang}\" is not supported)"));
  }
  Some(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn host_files_are_recognised_by_extension() {
    for host in [
      "a.vue", "b.svelte", "c.astro", "d.html", "e.HTM", "f.xhtml", "g.php",
    ] {
      assert!(is_host_file(Path::new(host)), "{host}");
    }
    for other in ["a.css", "b.scss", "c.md", "d.jsx", "vue"] {
      assert!(!is_host_file(Path::new(other)), "{other}");
    }
  }

  #[test]
  fn css_files_lint_whole_and_host_files_per_block() {
    let mut calls = Vec::new();
    let mut record = |text: &str, syntax: Syntax| {
      calls.push((text.to_string(), syntax));
      LintResult::new("x", text, Vec::new())
    };
    lint("a {}", "a.less", Syntax::Less, &mut record);
    lint(
      "<style lang=\"scss\">b {}</style>",
      "b.vue",
      Syntax::Css,
      &mut record,
    );
    assert_eq!(
      calls,
      vec![
        ("a {}".to_string(), Syntax::Less),
        ("b {}".to_string(), Syntax::Scss),
      ]
    );
  }

  #[test]
  fn skipped_blocks_are_warned_about_by_language() {
    let source = "<style lang=\"stylus\">a</style><style>b{}</style><style lang=\"styl\">c</style>";
    let mut skipped = skipped_blocks(Path::new("a.vue"), source);
    assert_eq!(skipped.len(), 2);
    skipped.extend(skipped_blocks(
      Path::new("b.svelte"),
      "<style lang=\"sugarss\">a</style>",
    ));
    assert!(skipped_blocks(Path::new("c.css"), source).is_empty());
    assert_eq!(
      skipped_blocks_warning(&skipped).unwrap(),
      "warning: Skipped 3 <style> blocks that gale cannot lint yet:\n  \
       a.vue (lang=\"stylus\" is not supported)\n  \
       b.svelte (lang=\"sugarss\" is not supported)"
    );
    assert_eq!(skipped_blocks_warning(&[]), None);
  }
}
