//! `--fix`: applying the fixes the rules offer and linting again.

use gale_diagnostics::apply_fixes;

use crate::lint::Linted;

/// `apply_fixes` drops overlapping edits, so when two rules want to rewrite
/// the same span only one lands per pass (e.g. color-hex-case then
/// color-hex-length on the same `#FFFFFF`).  Passes repeat until the source
/// stops changing so a single `--fix` converges the way Stylelint's does,
/// with this ceiling in case two rules disagree and would otherwise fight
/// forever.
const MAX_FIX_PASSES: usize = 10;

/// Fix every file in `linted` and leave each result describing what is
/// left.  Fixed files are written back, or printed to stdout when the
/// source came from stdin.
///
/// `--fix` and `--fix=lax` behave alike: Stylelint's strict mode skips files
/// with a syntax error, and gale's parser recovers from every one.
pub(crate) fn fix(linted: &mut [Linted], from_stdin: bool) {
  let mut total_fixed = 0usize;
  for Linted {
    result,
    rules,
    syntax,
  } in linted
  {
    // An ignored file has nothing to fix.
    let Some(rules) = rules else {
      continue;
    };

    let mut current = result.source.clone();
    let mut file_fixed = 0usize;
    for _ in 0..MAX_FIX_PASSES {
      let (fixed_source, count) = apply_fixes(&current, &result.diagnostics);
      if count == 0 || fixed_source == current {
        break;
      }
      current = fixed_source;
      file_fixed += count;
      // Re-lint the fixed source with the file's own rules to get the
      // remaining diagnostics, which also feed the next pass.
      *result = rules.lint(&current, &result.file_path, *syntax);
    }

    if file_fixed == 0 {
      continue;
    }
    if from_stdin {
      print!("{current}");
      total_fixed += file_fixed;
    } else if let Err(err) = std::fs::write(&result.file_path, &current) {
      eprintln!("Error writing {}: {err}", result.file_path);
    } else {
      total_fixed += file_fixed;
    }
  }
  if total_fixed > 0 {
    eprintln!("Fixed {total_fixed} problem(s).");
  }
}
