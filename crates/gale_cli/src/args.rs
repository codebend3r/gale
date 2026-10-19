//! The command line, as clap parses it.

use std::path::PathBuf;

use clap::Parser;

use crate::exit::{EXIT_INVALID_USAGE, Exit};

/// Every flag and argument `gale` takes, mirroring Stylelint's CLI where
/// they overlap.
#[derive(Parser)]
#[command(
  name = "gale",
  version,
  about = "An extremely fast CSS linter, written in Rust"
)]
pub(crate) struct Cli {
  /// Files or glob patterns to lint
  #[arg(required_unless_present_any = ["stdin", "init", "lsp", "print_config", "format_results"])]
  pub(crate) files: Vec<String>,

  /// Resolve and print the effective config for FILE as JSON, then exit
  #[arg(long, value_name = "FILE")]
  pub(crate) print_config: Option<PathBuf>,

  /// Start the LSP server (for editor integration)
  #[arg(long)]
  pub(crate) lsp: bool,

  /// Generate a starter gale.json config file in the current directory
  #[arg(long)]
  pub(crate) init: bool,

  /// Read source from stdin instead of files
  #[arg(long)]
  pub(crate) stdin: bool,

  /// Virtual filename for stdin input (used for syntax detection and
  /// diagnostics) [default: stdin.css]
  #[arg(long)]
  pub(crate) stdin_filename: Option<String>,

  /// Config file path
  #[arg(short, long)]
  pub(crate) config: Option<PathBuf>,

  /// Output format [default: the config's `formatter`, else text]
  ///
  /// Restricted to the implemented set so a typo (or a formatter Gale does
  /// not have) is rejected outright rather than silently falling back to
  /// text output.
  #[arg(
        short = 'f',
        long,
        value_parser = clap::builder::PossibleValuesParser::new(gale_formatter::FORMATTER_NAMES)
    )]
  pub(crate) formatter: Option<String>,

  /// Read a Stylelint-shaped JSON results array from stdin and print it
  /// with this formatter, then exit.
  ///
  /// The npm package formats through this, so each formatter has one
  /// implementation whether the report comes from the CLI or the API.
  #[arg(
        long,
        hide = true,
        value_name = "FORMATTER",
        value_parser = clap::builder::PossibleValuesParser::new(gale_formatter::FORMATTER_NAMES)
    )]
  pub(crate) format_results: Option<String>,

  /// Maximum number of warnings before erroring
  #[arg(long)]
  pub(crate) max_warnings: Option<usize>,

  /// Automatically fix problems (default: strict, or specify =lax)
  ///
  /// `require_equals` is essential: without it clap treats a bare `--fix` as
  /// taking the next positional argument as its MODE value, so
  /// `gale --fix "src/**/*.css"` would consume the glob and then fail with
  /// "no files provided".
  #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "strict",
        value_name = "MODE"
    )]
  pub(crate) fix: Option<String>,

  /// Only report errors
  #[arg(short, long)]
  pub(crate) quiet: bool,

  /// Force colour on in the text and verbose formatters
  #[arg(long, overrides_with = "no_color")]
  pub(crate) color: bool,

  /// Force colour off in the text and verbose formatters
  #[arg(long, overrides_with = "color")]
  pub(crate) no_color: bool,

  /// Path to a custom ignore file (uses gitignore syntax)
  #[arg(long, value_name = "FILE")]
  pub(crate) ignore_path: Option<PathBuf>,

  /// Glob pattern of files to ignore, on top of the ignore files (repeatable)
  #[arg(long, visible_alias = "ip", value_name = "PATTERN", action = clap::ArgAction::Append)]
  pub(crate) ignore_pattern: Vec<String>,

  /// Lint node_modules too instead of always skipping it
  #[arg(long, visible_alias = "di")]
  pub(crate) disable_default_ignores: bool,

  /// Accepted for Stylelint compatibility; Gale emits no deprecation warnings
  #[arg(long)]
  pub(crate) quiet_deprecation_warnings: bool,

  /// Parse every file with this syntax: postcss, postcss-scss, postcss-less or postcss-sass
  #[arg(long, value_name = "SYNTAX")]
  pub(crate) custom_syntax: Option<String>,

  /// Write the report to this file as well as printing it
  #[arg(short = 'o', long, value_name = "PATH")]
  pub(crate) output_file: Option<PathBuf>,

  /// Disable all ignore file processing (gitignore, .galeignore, custom)
  #[arg(long)]
  pub(crate) no_ignore: bool,

  /// Enable file-based caching to skip unchanged files
  #[arg(long)]
  pub(crate) cache: bool,

  /// Override the cache file location (default: .gale_cache in the current directory)
  #[arg(long, value_name = "PATH")]
  pub(crate) cache_location: Option<PathBuf>,

  /// How the cache detects changed files: metadata (default) or content
  #[arg(long, value_name = "STRATEGY")]
  pub(crate) cache_strategy: Option<String>,

  /// Don't error when no files match the glob pattern
  #[arg(long)]
  pub(crate) allow_empty_input: bool,

  /// Ignore all `/* stylelint-disable */` comments
  #[arg(long)]
  pub(crate) ignore_disables: bool,

  /// Report `/* stylelint-disable */` comments that don't suppress any warnings
  #[arg(long)]
  pub(crate) report_needless_disables: bool,

  /// Report `/* stylelint-disable */` comments that reference rules not being linted
  #[arg(long)]
  pub(crate) report_invalid_scope_disables: bool,

  /// Report `/* stylelint-disable */` comments without a description
  #[arg(long)]
  pub(crate) report_descriptionless_disables: bool,

  /// Report `/* stylelint-disable */` comments that name no rule
  #[arg(long)]
  pub(crate) report_unscoped_disables: bool,
}

/// The command line, or `None` when it asked for `--help` or `--version`,
/// which clap has printed.  Anything else clap rejects is a usage error,
/// which Stylelint reports with exit code 64.
pub(crate) fn parse() -> Result<Option<Cli>, Exit> {
  match Cli::try_parse() {
    Ok(cli) => Ok(Some(cli)),
    Err(err) => {
      let _ = err.print();
      if err.use_stderr() {
        Err(Exit::quiet(EXIT_INVALID_USAGE))
      } else {
        Ok(None)
      }
    }
  }
}
