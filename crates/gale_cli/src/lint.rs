//! Linting stdin, or the files the command line names.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rayon::prelude::*;
use tracing::debug;

use gale_config::{Ignored, RuleSetResolver};
use gale_css_parser::Syntax;
use gale_diagnostics::LintResult;
use gale_linter::RuleSet;

use crate::args::Cli;
use crate::cache::{
  LintCache, combine_hashes, compute_config_hash, compute_hash, metadata_fingerprint,
};
use crate::config::{Settings, warn_skipped_rules};
use crate::discover::{
  DiscoverOptions, Discovery, SkipReason, Skipped, discover_files, skipped_files_warning,
};
use crate::embedded;
use crate::exit::{EXIT_FATAL, Exit};

/// The name stdin is linted under without `--stdin-filename`.
pub(crate) const DEFAULT_STDIN_FILENAME: &str = "stdin.css";

/// A file's result, with what `--fix` needs to lint it again.
pub(crate) struct Linted {
  pub(crate) result: LintResult,
  /// `None` for a file an override ignores, which is reported clean.
  pub(crate) rules: Option<Arc<RuleSet>>,
  pub(crate) syntax: Syntax,
}

impl Linted {
  /// Lint `source`, the text of `file_path`, with `rules`; no rules is a
  /// clean result.
  fn lint(source: &str, file_path: &str, syntax: Syntax, rules: Option<Arc<RuleSet>>) -> Self {
    let result = match &rules {
      Some(rules) => rules.lint(source, file_path, syntax),
      None => LintResult::new(file_path, source, Vec::new()),
    };
    Self {
      result,
      rules,
      syntax,
    }
  }
}

/// Lint what stdin holds, as `--stdin-filename` (default `stdin.css`) in
/// the working directory.
pub(crate) fn lint_stdin(cli: &Cli, resolver: &mut RuleSetResolver) -> Result<Vec<Linted>> {
  let mut source = String::new();
  std::io::stdin().read_to_string(&mut source)?;

  let file_path = cli
    .stdin_filename
    .as_deref()
    .unwrap_or(DEFAULT_STDIN_FILENAME);
  let settings = resolver.resolve(file_path);
  warn_skipped_rules(resolver);
  let syntax = match settings.syntax {
    Ok(syntax) => syntax,
    Err(name) => {
      let skipped = [Skipped {
        path: PathBuf::from(file_path),
        reason: SkipReason::CustomSyntax(name),
      }];
      if let Some(warning) = skipped_files_warning(&skipped) {
        eprintln!("{warning}");
      }
      return Ok(Vec::new());
    }
  };
  let blocks = embedded::skipped_blocks(Path::new(file_path), &source);
  if let Some(warning) = embedded::skipped_blocks_warning(&blocks) {
    eprintln!("{warning}");
  }
  // Stdin is linted whatever the config's top-level `ignoreFiles` says;
  // an override that ignores the file leaves it clean.
  let rules = (settings.ignored != Some(Ignored::Override)).then_some(settings.rules);
  Ok(vec![Linted::lint(&source, file_path, syntax, rules)])
}

/// A file ready to lint: its parser and its rules, settled before the
/// parallel loop.
struct Planned {
  path: PathBuf,
  syntax: Syntax,
  /// `None` for a file an override ignores, which is reported clean.
  rules: Option<Arc<RuleSet>>,
  /// The cache key part for the file: its config and the run's settings.
  cache_key: u64,
}

/// Lint the files the command line names, or `None` when nothing matched
/// and `allowEmptyInput` makes that fine.
///
/// `root_ignore` holds the root config's `ignoreFiles` for the walk to
/// skip; `run_hash` is the cache key part every file shares.
pub(crate) fn lint_files(
  cli: &Cli,
  settings: &Settings,
  root_ignore: &[String],
  resolver: &mut RuleSetResolver,
  cache: &mut LintCache,
  run_hash: u64,
) -> Result<Option<Vec<Linted>>, Exit> {
  // `--ignore-pattern` globs join the config's ignoreFiles so both exclude
  // matches everywhere.
  let mut ignore_patterns = root_ignore.to_vec();
  ignore_patterns.extend(cli.ignore_pattern.iter().cloned());
  let discover_opts = DiscoverOptions {
    no_ignore: cli.no_ignore,
    ignore_path: cli.ignore_path.as_deref(),
    ignore_patterns: &ignore_patterns,
    disable_default_ignores: cli.disable_default_ignores,
  };
  let Discovery { files, mut skipped } = discover_files(&cli.files, &discover_opts);
  debug!("Discovered {} CSS file(s)", files.len());

  // Nothing matched at all.  Files that matched but are skipped as
  // unsupported are not empty input: they are warned about below and the
  // run carries on with nothing to report, as for an unsupported
  // `customSyntax`.
  if files.is_empty() && skipped.is_empty() {
    if settings.allow_empty_input {
      return Ok(None);
    }
    // Stylelint raises NoFilesFoundError and exits 1 here; exiting 0 would
    // let a CI step that lints a mistyped path pass.
    let patterns = cli
      .files
      .iter()
      .map(|p| format!("\"{p}\""))
      .collect::<Vec<_>>()
      .join(", ");
    return Err(Exit::new(
      EXIT_FATAL,
      format!("No files matching the pattern {patterns} were found."),
    ));
  }

  let planned = plan(cli, files, &mut skipped, resolver, run_hash);
  warn_skipped_rules(resolver);
  if let Some(warning) = skipped_files_warning(&skipped) {
    eprintln!("{warning}");
  }
  let files: Vec<PathBuf> = planned.iter().map(|plan| plan.path.clone()).collect();
  if let Some(warning) = embedded::skipped_blocks_warning(&embedded::skipped_blocks_in(&files)) {
    eprintln!("{warning}");
  }

  let linted = if settings.use_cache {
    lint_with_cache(planned, settings, cache)
  } else {
    // Without caching: lint each file in parallel (lock-free).
    planned
      .into_par_iter()
      .map(|plan| {
        let source = read_source(&plan.path)?;
        let file_path = plan.path.display().to_string();
        Ok(Linted::lint(&source, &file_path, plan.syntax, plan.rules))
      })
      .collect::<Result<Vec<_>, Exit>>()
  }?;
  Ok(Some(linted))
}

/// Settle each file's config, rules and parser up front, single threaded,
/// so the parallel loop only lints.
///
/// A file its config's `ignoreFiles` covers is dropped (unless
/// `--no-ignore` turns ignore processing off, as it does for walks); one
/// whose `customSyntax` (from the CLI, an override or the top level of its
/// config) names a syntax gale cannot parse joins `skipped`.
fn plan(
  cli: &Cli,
  files: Vec<PathBuf>,
  skipped: &mut Vec<Skipped>,
  resolver: &mut RuleSetResolver,
  run_hash: u64,
) -> Vec<Planned> {
  let mut config_hashes: HashMap<usize, u64> = HashMap::new();
  let mut planned = Vec::with_capacity(files.len());
  for file in files {
    let settings = resolver.resolve(&file.display().to_string());
    if settings.ignored == Some(Ignored::IgnoreFiles) && !cli.no_ignore {
      continue;
    }
    match settings.syntax {
      Ok(syntax) => {
        let config_hash = *config_hashes
          .entry(settings.config_id)
          .or_insert_with(|| compute_config_hash(&settings.config));
        planned.push(Planned {
          path: file,
          syntax,
          rules: (settings.ignored != Some(Ignored::Override)).then_some(settings.rules),
          cache_key: combine_hashes(&[run_hash, config_hash]),
        });
      }
      Err(name) => skipped.push(Skipped {
        path: file,
        reason: SkipReason::CustomSyntax(name),
      }),
    }
  }
  debug!(
    "Resolved {} file(s) against {} config(s)",
    planned.len(),
    resolver.configs().count()
  );
  planned
}

/// Lint `planned` in parallel, skipping the files the cache knows were
/// clean and recording the rest.
fn lint_with_cache(
  planned: Vec<Planned>,
  settings: &Settings,
  cache: &mut LintCache,
) -> Result<Vec<Linted>, Exit> {
  let cache = Mutex::new(cache);
  planned
    .into_par_iter()
    .map(|plan| {
      let file_path = plan.path.display().to_string();

      // Skip only files that were clean (0 diagnostics).  The metadata
      // strategy needs only the file's size and mtime, so a clean file is
      // skipped without reading it.
      let is_clean = |fingerprint: u64| {
        let cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.is_clean(&file_path, fingerprint)
      };
      let early = metadata_fingerprint(&plan.path, settings.cache_strategy, plan.cache_key);
      if early.is_some_and(is_clean) {
        debug!("Cache hit (clean): {file_path}");
        return Ok(None);
      }
      let source = read_source(&plan.path)?;
      let content_hash = match early {
        Some(fingerprint) => fingerprint,
        None => {
          let fingerprint = compute_hash(&source, plan.cache_key);
          if is_clean(fingerprint) {
            debug!("Cache hit (clean): {file_path}");
            return Ok(None);
          }
          fingerprint
        }
      };

      let linted = Linted::lint(&source, &file_path, plan.syntax, plan.rules);

      // A file with an invalid option is not clean either: Stylelint never
      // caches a result that errored.
      let result = &linted.result;
      let problems = result.diagnostics.len() + result.invalid_option_warnings.len();
      cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(file_path, content_hash, problems);

      Ok(Some(linted))
    })
    .collect::<Result<Vec<_>, Exit>>()
    .map(|results| results.into_iter().flatten().collect())
}

fn read_source(path: &Path) -> Result<String, Exit> {
  std::fs::read_to_string(path).map_err(|err| {
    Exit::new(
      EXIT_FATAL,
      format!("Error reading {}: {err}", path.display()),
    )
  })
}
