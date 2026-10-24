//! The `gale` command line.
//!
//! A run goes through stages, each in its own module and each returning
//! plain values: parse the arguments (`args`), load the configuration
//! (`config`), find the files (`discover`, `glob`), lint them (`lint`),
//! fix them (`fix`) and print the report (`report`).  A stage that has to
//! stop the run returns an `exit::Exit`; [`run`] is the one place that
//! becomes the process's exit code.

mod args;
mod cache;
mod config;
mod discover;
mod embedded;
mod exit;
mod fix;
mod glob;
mod lint;
mod report;

use anyhow::Result;
use tracing::debug;

use gale_config::WithoutConfig;
use gale_linter::runner::PERF_TARGET;

use crate::args::Cli;
use crate::cache::{LintCache, compute_run_hash};
use crate::config::{RootConfig, Settings};
use crate::exit::Exit;
use crate::report::ReportOptions;

/// Send `tracing` output to stderr, filtered by `GALE_LOG` (default
/// `warn`).  `GALE_DEBUG_PERF=1` adds the per-phase timings
/// ([`PERF_TARGET`] at debug level) to whatever `GALE_LOG` selects.
///
/// Stderr keeps logs out of the report on stdout and out of the language
/// server's protocol stream.
fn init_tracing() {
  let mut filter = tracing_subscriber::EnvFilter::try_from_env("GALE_LOG")
    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
  if std::env::var("GALE_DEBUG_PERF").as_deref() == Ok("1")
    && let Ok(directive) = format!("{PERF_TARGET}=debug").parse()
  {
    filter = filter.add_directive(directive);
  }
  tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_writer(std::io::stderr)
    .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
    .init();
}

/// Parses the CLI arguments and runs the requested mode (the LSP server,
/// `--init`, `--format-results`, `--print-config` or a lint), exiting with
/// Stylelint's code for the outcome.
pub fn run() -> Result<()> {
  let code = match execute() {
    Ok(code) => code,
    Err(err) => {
      // Anything but an `Exit` is an unexpected failure, which `main`
      // reports as an error.
      let exit = err.downcast::<Exit>()?;
      if let Some(message) = &exit.message {
        eprintln!("{message}");
      }
      exit.code
    }
  };
  if code != 0 {
    std::process::exit(code);
  }
  Ok(())
}

/// Run the stages the command line asks for and return the exit code.
fn execute() -> Result<i32> {
  let Some(cli) = args::parse()? else {
    return Ok(0);
  };
  init_tracing();

  if cli.lsp {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(gale_lsp::run_server(cli.config.clone()));
    return Ok(0);
  }
  if cli.init {
    config::generate_init_config()?;
    return Ok(0);
  }
  if let Some(name) = &cli.format_results {
    report::format_stdin_results(name, &cli)?;
    return Ok(0);
  }

  let root = RootConfig::load(&cli)?;
  root.warn_unsupported_plugins();
  if let Some(file) = &cli.print_config {
    config::print_config(&cli, &root, file)?;
    return Ok(0);
  }
  let settings = Settings::new(&cli, &root.config)?;
  lint_and_report(&cli, &root, &settings)
}

/// Lint stdin or the files the command line names, fix them if asked, and
/// print the report.
fn lint_and_report(cli: &Cli, root: &RootConfig, settings: &Settings) -> Result<i32> {
  let mut cache = if settings.use_cache {
    debug!("Loading cache from {}", settings.cache_path.display());
    LintCache::load(&settings.cache_path, settings.cache_strategy)
  } else {
    LintCache::new(settings.cache_strategy)
  };

  let run = config::run_settings(cli);
  // The cache key part every file shares: the forced syntax and what the
  // run adds to every config.
  let run_hash = compute_run_hash(cli.custom_syntax.as_deref(), &run);
  // Stdin without any config file is linted with every rule, as it always
  // has been; files are not linted at all.
  let without_config = if cli.stdin {
    WithoutConfig::AllRules
  } else {
    WithoutConfig::NoRules
  };
  let mut resolver = root.resolver(cli, without_config, cli.custom_syntax.clone(), run);

  let mut linted = if cli.stdin {
    lint::lint_stdin(cli, &mut resolver)?
  } else {
    let ignore = &root.config.ignore_patterns;
    match lint::lint_files(cli, settings, ignore, &mut resolver, &mut cache, run_hash)? {
      Some(linted) => linted,
      None => return Ok(0),
    }
  };

  if settings.fix.is_some() {
    fix::fix(&mut linted, cli.stdin)?;
  }

  if settings.use_cache {
    debug!("Saving cache to {}", settings.cache_path.display());
    cache.save(&settings.cache_path);
  }

  let results = linted.into_iter().map(|linted| linted.result).collect();
  Ok(report::report(
    results,
    &ReportOptions {
      formatter: &settings.formatter,
      quiet: settings.quiet,
      stdin: cli.stdin,
      stdin_filename: cli.stdin_filename.as_deref(),
      max_warnings: cli.max_warnings,
      output_file: cli.output_file.as_deref(),
      color: report::report_color(cli),
    },
  ))
}
