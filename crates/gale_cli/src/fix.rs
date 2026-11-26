//! `--fix`: applying the fixes the rules offer and linting again.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use gale_diagnostics::{LintResult, apply_fixes};
use rayon::prelude::*;

use crate::atomic_write::atomic_write;
use crate::lint::Linted;

/// `apply_fixes` drops overlapping edits, so when two rules want to rewrite
/// the same span only one lands per pass (e.g. color-hex-case then
/// color-hex-length on the same `#FFFFFF`).  Passes repeat until the source
/// stops changing so a single `--fix` converges the way Stylelint's does,
/// with this ceiling in case two rules disagree and would otherwise fight
/// forever.
const MAX_FIX_PASSES: usize = 10;

/// A file's source once the fix passes settle, and what linting it found.
struct Converged {
  source: String,
  result: LintResult,
  /// How many problems the passes fixed; more than zero.
  fixed: usize,
}

/// Fix every file in `linted` and leave each result describing what is
/// left.  Fixed files are written back, or printed to stdout when the
/// source came from stdin.
///
/// The fixing runs in parallel, as linting does; the writing then goes in
/// input order, so stdout and the first failing write are what they would
/// be one file at a time.  A failed write stops the run there: that file
/// and every later one keep the results they were linted with.
///
/// `--fix` and `--fix=lax` behave alike: Stylelint's strict mode skips files
/// with a syntax error, and gale's parser recovers from every one.
pub(crate) fn fix(linted: &mut [Linted], from_stdin: bool) -> Result<()> {
  let converged: Vec<Option<Converged>> = linted.par_iter().map(converge).collect();
  let mut total_fixed = 0usize;
  for (linted, converged) in linted.iter_mut().zip(converged) {
    let Some(Converged {
      source,
      result,
      fixed,
    }) = converged
    else {
      continue;
    };
    if from_stdin {
      std::io::stdout().lock().write_all(source.as_bytes())?;
    } else {
      write_fixed(Path::new(&linted.result.file_path), &source)
        .with_context(|| format!("Error writing {}", linted.result.file_path))?;
    }
    // Publish the new diagnostics only after persistence succeeds.
    linted.result = result;
    total_fixed += fixed;
  }
  if total_fixed > 0 {
    eprintln!("Fixed {total_fixed} problem(s).");
  }
  Ok(())
}

/// Apply the file's fixes pass after pass until its source stops
/// changing, without touching the file; `None` when there was nothing to
/// fix (or the file is ignored).
fn converge(linted: &Linted) -> Option<Converged> {
  let Linted {
    result,
    rules,
    syntax,
  } = linted;
  // An ignored file has nothing to fix.
  let rules = rules.as_ref()?;

  let mut current = result.source.clone();
  let mut updated = None;
  let mut file_fixed = 0usize;
  for _ in 0..MAX_FIX_PASSES {
    let diagnostics = &updated.as_ref().unwrap_or(result).diagnostics;
    let (fixed_source, count) = apply_fixes(&current, diagnostics);
    if count == 0 || fixed_source == current {
      break;
    }
    current = fixed_source;
    file_fixed += count;
    // Re-lint the fixed source with the file's own rules to get the
    // remaining diagnostics, which also feed the next pass.
    updated = Some(rules.lint(&current, &result.file_path, *syntax));
  }

  (file_fixed > 0).then(|| Converged {
    source: current,
    result: updated.expect("a changed source was linted"),
    fixed: file_fixed,
  })
}

/// Replace a regular file atomically, retaining permissions and following
/// symlinks. Hard-linked files keep their identity via an in-place write.
fn write_fixed(path: &Path, source: &str) -> std::io::Result<()> {
  let target = path.canonicalize()?;
  let metadata = std::fs::metadata(&target)?;
  if metadata.permissions().readonly() {
    return Err(std::io::Error::new(
      std::io::ErrorKind::PermissionDenied,
      "file is read-only",
    ));
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::MetadataExt;
    if metadata.nlink() > 1 {
      return std::fs::write(target, source);
    }
  }
  atomic_write(&target, source.as_bytes(), Some(metadata.permissions()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use gale_linter::{RuleSet, RuleSetSpec, RuleSetting};
  use std::sync::Arc;

  #[test]
  fn failed_write_keeps_the_original_result() {
    let dir = tempfile::tempdir().unwrap();
    // A missing target is deterministic even for privileged test users.
    let path = dir.path().join("missing.css");
    let rules = Arc::new(RuleSet::new(RuleSetSpec {
      rules: vec![RuleSetting::new("color-hex-length").with_options(serde_json::json!("short"))],
      ..Default::default()
    }));
    let source = "a {color:#ffffff}";
    let mut linted = [Linted {
      result: rules.lint(source, path.to_str().unwrap(), Syntax::Css),
      rules: Some(rules),
      syntax: Syntax::Css,
    }];
    assert!(fix(&mut linted, false).is_err());
    assert_eq!(linted[0].result.source, source);
    assert!(!linted[0].result.diagnostics.is_empty());
  }

  #[test]
  fn atomic_replacement_preserves_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.css");
    std::fs::write(&path, "old").unwrap();
    let permissions = std::fs::metadata(&path).unwrap().permissions();
    write_fixed(&path, "new").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions(), permissions);
  }

  #[cfg(unix)]
  #[test]
  fn linked_files_keep_their_targets() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.css");
    let link = dir.path().join("link.css");
    let hard = dir.path().join("hard.css");
    std::fs::write(&target, "old").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    write_fixed(&link, "new").unwrap();
    assert!(link.is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    std::fs::hard_link(&target, &hard).unwrap();
    write_fixed(&target, "hard").unwrap();
    assert_eq!(std::fs::read_to_string(&hard).unwrap(), "hard");
  }
}
