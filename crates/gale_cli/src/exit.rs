//! Stylelint's exit codes, which CI scripts key off, and the error a stage
//! returns to stop the run with one.

use std::fmt;

/// A fatal error, such as no files matching the patterns.
pub(crate) const EXIT_FATAL: i32 = 1;
/// Lint problems were found, or `--max-warnings` was exceeded.
pub(crate) const EXIT_LINT_PROBLEM: i32 = 2;
/// The command line could not be understood.
pub(crate) const EXIT_INVALID_USAGE: i32 = 64;
/// The configuration could not be loaded or is invalid.
pub(crate) const EXIT_INVALID_CONFIG: i32 = 78;

/// A run that stops early: the exit code for why, and what to tell the user
/// on stderr first (nothing when it has been said already).
///
/// Stages return it as an error; [`crate::run`] is the one place that turns
/// it into the process's exit.
#[derive(Debug)]
pub(crate) struct Exit {
  pub(crate) code: i32,
  pub(crate) message: Option<String>,
}

impl Exit {
  /// Stop with `code`, printing `message`.
  pub(crate) fn new(code: i32, message: impl Into<String>) -> Self {
    Self {
      code,
      message: Some(message.into()),
    }
  }

  /// Stop with `code`, the reason already printed.
  pub(crate) fn quiet(code: i32) -> Self {
    Self {
      code,
      message: None,
    }
  }
}

impl fmt::Display for Exit {
  /// The message, or the bare exit code when there is none.
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match &self.message {
      Some(message) => f.write_str(message),
      None => write!(f, "exit code {}", self.code),
    }
  }
}

impl std::error::Error for Exit {}
