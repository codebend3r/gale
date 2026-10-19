//! Printing the results and deciding the exit code.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Result;
use gale_diagnostics::{LintResult, Severity};
use gale_formatter::{StylelintResult, create_formatter_with_color, strip_ansi};
use gale_linter::runner::PERF_TARGET;

use crate::args::Cli;
use crate::exit::{EXIT_INVALID_USAGE, EXIT_LINT_PROBLEM, Exit};

/// The formatter to use: the `--formatter` flag, else the config's
/// `formatter`, else `text`.
///
/// The flag is already restricted to known names by clap.  A config value
/// that names no formatter is an error carrying Stylelint's wording, which
/// lists the valid names, rather than a silent fallback to text.
pub(crate) fn choose_formatter(cli: Option<&str>, config: Option<&str>) -> Result<String, String> {
  match (cli, config) {
    (Some(name), _) => Ok(name.to_string()),
    (None, Some(name)) if gale_formatter::FORMATTER_NAMES.contains(&name) => Ok(name.to_string()),
    (None, Some(name)) => {
      let valid = gale_formatter::FORMATTER_NAMES
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
      Err(format!(
        "The config's formatter \"{name}\" is not a formatter gale has. \
         You must use a valid formatter option: {valid}"
      ))
    }
    (None, None) => Ok("text".to_string()),
  }
}

/// Decide whether the human-readable formatters colour their output.
///
/// This is picocolors' rule, which Stylelint relies on:
///
/// ```text
/// colour = !(NO_COLOR || --no-color)
///       && (FORCE_COLOR || --color || CI || (stdout is a TTY && TERM != dumb))
/// ```
///
/// An environment variable counts as set only when it is non-empty, matching
/// JavaScript truthiness.
fn color_enabled(
  force_on: bool,
  force_off: bool,
  stdout_is_tty: bool,
  env: impl Fn(&str) -> Option<String>,
) -> bool {
  let set = |name: &str| env(name).is_some_and(|v| !v.is_empty());
  if force_off || set("NO_COLOR") {
    return false;
  }
  if force_on || set("FORCE_COLOR") || set("CI") {
    return true;
  }
  stdout_is_tty && env("TERM").as_deref() != Some("dumb")
}

/// Whether this run's report is coloured: [`color_enabled`] for the real
/// flags, environment and stdout.
pub(crate) fn report_color(cli: &Cli) -> bool {
  color_enabled(
    cli.color,
    cli.no_color,
    std::io::IsTerminal::is_terminal(&std::io::stdout()),
    |name| std::env::var(name).ok(),
  )
}

/// `--format-results`: print the Stylelint-shaped results on stdin with the
/// formatter `name`, coloured as a lint run's report would be.  Input that is
/// no results array is a usage error.
pub(crate) fn format_stdin_results(name: &str, cli: &Cli) -> Result<()> {
  let mut input = String::new();
  std::io::stdin().read_to_string(&mut input)?;
  let results: Vec<StylelintResult> = serde_json::from_str(&input).map_err(|err| {
    Exit::new(
      EXIT_INVALID_USAGE,
      format!("Error: stdin is not a JSON array of Stylelint results: {err}"),
    )
  })?;
  print!(
    "{}",
    create_formatter_with_color(name, report_color(cli)).format(&results)
  );
  Ok(())
}

/// Write a report to `path`, creating parent directories and stripping ANSI
/// escapes the way Stylelint's `--output-file` does.
fn write_output_file(path: &Path, report: &str) -> std::io::Result<()> {
  if let Some(parent) = path.parent()
    && !parent.as_os_str().is_empty()
  {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(path, strip_ansi(report))
}

/// The path Stylelint reports a source under: `path` joined to `cwd` when
/// relative, with `.` and `..` resolved without touching the file system
/// (Node's `path.join`), so symlinks stay as written.
///
/// Stylelint normalises an absolute path it found on disk too, but takes an
/// absolute `codeFilename` (`--stdin-filename`) as given;
/// `normalize_absolute` says which this is.
fn reported_path(cwd: &Path, path: &str, normalize_absolute: bool) -> String {
  let path = Path::new(path);
  if path.is_absolute() && !normalize_absolute {
    return path.display().to_string();
  }
  let joined = if path.is_absolute() {
    path.to_path_buf()
  } else {
    cwd.join(path)
  };
  let mut normal = PathBuf::new();
  for component in joined.components() {
    match component {
      std::path::Component::CurDir => {}
      std::path::Component::ParentDir => {
        normal.pop();
      }
      other => normal.push(other),
    }
  }
  normal.display().to_string()
}

/// How the report is printed.
pub(crate) struct ReportOptions<'a> {
  /// The formatter's name.
  pub(crate) formatter: &'a str,
  /// Only report errors (`--quiet` or the config's `quiet`).
  pub(crate) quiet: bool,
  /// Whether the results came from stdin, which keeps its name unless
  /// `--stdin-filename` gave it one.
  pub(crate) stdin: bool,
  pub(crate) stdin_filename: Option<&'a str>,
  /// `--max-warnings`.
  pub(crate) max_warnings: Option<usize>,
  /// `--output-file`.
  pub(crate) output_file: Option<&'a Path>,
  /// Whether the human-readable formatters colour their output.
  pub(crate) color: bool,
}

/// Print `results` and return the exit code they call for.
///
/// Lint problems exit 2, as in Stylelint, and so does an invalid rule
/// option (Stylelint marks the result errored) or more warnings than
/// `--max-warnings` allows.
pub(crate) fn report(mut results: Vec<LintResult>, opts: &ReportOptions<'_>) -> i32 {
  if opts.quiet {
    for result in &mut results {
      result.diagnostics.retain(|d| d.severity == Severity::Error);
    }
  }

  // Stylelint reports each source by its absolute path, whatever form the
  // file was named in.  Fixes have been written and the cache saved under
  // the path as discovered, so only the report changes.  Without a
  // `--stdin-filename` there is no path to report, and the default name
  // stays as it is.
  if !opts.stdin || opts.stdin_filename.is_some() {
    let cwd = std::env::current_dir().unwrap_or_default();
    for result in &mut results {
      result.file_path = reported_path(&cwd, &result.file_path, !opts.stdin);
    }
  }

  let started = std::time::Instant::now();
  let formatter = create_formatter_with_color(opts.formatter, opts.color);
  let output = formatter.format(&gale_formatter::stylelint_results(&results));
  tracing::debug!(
    target: PERF_TARGET,
    phase = "format",
    secs = started.elapsed().as_secs_f64(),
  );
  if !output.is_empty() {
    print!("{output}");
    if let Some(path) = opts.output_file
      && let Err(err) = write_output_file(path, &output)
    {
      eprintln!("Error writing report to {}: {err}", path.display());
    }
  }

  let count = |severity| -> usize { results.iter().map(|r| r.count_by_severity(severity)).sum() };
  let total_warnings = count(Severity::Warning);
  if let Some(max) = opts.max_warnings
    && total_warnings > max
  {
    eprintln!("Found {total_warnings} warning(s) (max allowed: {max})");
    return EXIT_LINT_PROBLEM;
  }
  if count(Severity::Error) > 0
    || results
      .iter()
      .any(|r| !r.invalid_option_warnings.is_empty())
  {
    return EXIT_LINT_PROBLEM;
  }
  0
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  #[test]
  fn sources_are_reported_by_absolute_path_like_stylelint() {
    let cwd = Path::new("/work/project");
    for (given, reported) in [
      ("src/a.css", "/work/project/src/a.css"),
      ("./src/a.css", "/work/project/src/a.css"),
      ("src/sub/../a.css", "/work/project/src/a.css"),
      ("../other/b.css", "/work/other/b.css"),
      ("/abs/./x/../c.css", "/abs/c.css"),
    ] {
      assert_eq!(reported_path(cwd, given, true), reported, "{given}");
    }
    // An absolute `--stdin-filename` is taken as given, as Stylelint takes
    // `codeFilename`; a relative one is joined to the working directory.
    assert_eq!(reported_path(cwd, "/abs/./c.css", false), "/abs/./c.css");
    assert_eq!(reported_path(cwd, "./x.css", false), "/work/project/x.css");
  }

  #[test]
  fn formatter_comes_from_the_flag_then_the_config_then_text() {
    assert_eq!(choose_formatter(Some("json"), Some("tap")).unwrap(), "json");
    assert_eq!(choose_formatter(None, Some("tap")).unwrap(), "tap");
    assert_eq!(choose_formatter(None, None).unwrap(), "text");
    // An unknown config value is an error, not a quiet switch to text.
    let err = choose_formatter(None, Some("nope")).unwrap_err();
    assert!(err.contains("\"nope\""), "{err}");
    assert!(
      err.contains("You must use a valid formatter option: \"text\""),
      "{err}"
    );
    // A flag overrides even an unknown config value.
    assert_eq!(
      choose_formatter(Some("unix"), Some("nope")).unwrap(),
      "unix"
    );
  }

  #[test]
  fn colour_follows_picocolors_rules() {
    let none = |_: &str| None;
    let with = |vars: &'static [(&'static str, &'static str)]| {
      move |name: &str| {
        vars
          .iter()
          .find(|(k, _)| *k == name)
          .map(|(_, v)| v.to_string())
      }
    };

    // Piped output with no hints: off.  A terminal: on.
    assert!(!color_enabled(false, false, false, none));
    assert!(color_enabled(false, false, true, none));
    assert!(!color_enabled(
      false,
      false,
      true,
      with(&[("TERM", "dumb")])
    ));

    // Flags and FORCE_COLOR / CI turn it on when piped.
    assert!(color_enabled(true, false, false, none));
    assert!(color_enabled(
      false,
      false,
      false,
      with(&[("FORCE_COLOR", "1")])
    ));
    assert!(color_enabled(false, false, false, with(&[("CI", "true")])));

    // NO_COLOR and --no-color win over everything.
    assert!(!color_enabled(
      false,
      true,
      true,
      with(&[("FORCE_COLOR", "1")])
    ));
    assert!(!color_enabled(
      true,
      false,
      true,
      with(&[("NO_COLOR", "1")])
    ));

    // Empty variables do not count as set.
    assert!(color_enabled(false, false, true, with(&[("NO_COLOR", "")])));
    assert!(!color_enabled(
      false,
      false,
      false,
      with(&[("FORCE_COLOR", "")])
    ));
  }

  #[test]
  fn output_file_is_written_without_ansi_into_new_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out").join("nested").join("report.txt");
    write_output_file(&path, "\x1b[31mred\x1b[39m plain\n").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "red plain\n");
  }
}
