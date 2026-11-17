use std::path::{Component, Path, PathBuf};

use gale_diagnostics::{LintResult, SourceLineIndex};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Formatter trait
// ---------------------------------------------------------------------------

/// Trait for formatting lint results into a displayable string.
pub trait Formatter {
  /// Renders every result into the formatter's output string.
  fn format(&self, results: &[StylelintResult]) -> String;
}

// ---------------------------------------------------------------------------
// Results model
// ---------------------------------------------------------------------------

/// One source in Stylelint's results model: what the `json` formatter prints
/// and what every formatter renders.
///
/// The CLI converts its lint results once, with [`stylelint_results`];
/// `--format-results` reads the same shape back from JSON, which is how the
/// npm package's `formatters` reach these formatters.  A field the JSON
/// leaves out takes its empty value.
///
/// Mirrors Stylelint's per-source JSON object, including the fields Gale never
/// populates. Consumers (editors, CI reporters, `stylelint.lint()` shims) index
/// into these unconditionally, so they must be present even when empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StylelintResult {
  pub source: String,
  pub deprecations: Vec<serde_json::Value>,
  pub invalid_option_warnings: Vec<InvalidOptionWarning>,
  pub parse_errors: Vec<serde_json::Value>,
  pub errored: bool,
  pub warnings: Vec<StylelintWarning>,
}

/// One entry of Stylelint's `invalidOptionWarnings`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InvalidOptionWarning {
  pub text: String,
}

/// One warning of a [`StylelintResult`], with 1-indexed positions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StylelintWarning {
  pub line: usize,
  pub column: usize,
  /// Always set by the CLI; left out of the JSON for a warning read without one.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub end_line: Option<usize>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub end_column: Option<usize>,
  pub rule: String,
  pub severity: String,
  /// The message with Stylelint's `" (rule-name)"` suffix, when it has one.
  pub text: String,
  /// Only present when the rule's `url` secondary option is set, as in
  /// Stylelint, where an undefined `url` is dropped by `JSON.stringify`.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  /// Whether `gale --fix` can fix it, for the agent formatter.  Not part of
  /// Stylelint's model, so it is never written or read as JSON.
  #[serde(skip)]
  pub fixable: bool,
}

impl StylelintWarning {
  /// The message without Stylelint's trailing `" (rule-name)"`, which is how
  /// Stylelint's string formatter shows it.
  pub fn message(&self) -> &str {
    self
      .text
      .strip_suffix(format!(" ({})", self.rule).as_str())
      .unwrap_or(&self.text)
  }

  /// Whether it counts as an error; every other severity counts as a warning.
  fn is_error(&self) -> bool {
    self.severity == "error"
  }
}

/// Converts lint results into Stylelint's results model: 1-indexed start and
/// end positions, and Stylelint's `" (rule-name)"` message suffix.
pub fn stylelint_results(results: &[LintResult]) -> Vec<StylelintResult> {
  results
    .iter()
    .map(|result| {
      let line_index = SourceLineIndex::build(&result.source);
      let warnings = result
        .diagnostics
        .iter()
        .map(|diag| {
          let (line, column) = line_index.offset_to_location(diag.span.offset);
          let (mut end_line, mut end_column) = line_index.offset_to_location(diag.span.end());
          // PostCSS's `rangeBy` never ends a range at or before its start:
          // a problem at one index ends one column on, on the same line.
          // A span cannot say that for a line break or the end of the text,
          // so rules report those as an empty span there.
          let at_line_end = matches!(
            result.source.as_bytes().get(diag.span.offset),
            None | Some(b'\n' | b'\r')
          );
          if diag.span.length == 0 && at_line_end {
            (end_line, end_column) = (line, column + 1);
          }
          StylelintWarning {
            line,
            column,
            end_line: Some(end_line),
            end_column: Some(end_column),
            rule: diag.rule_name.clone(),
            severity: diag.severity.to_string(),
            // Stylelint appends " (rule-name)" to every rule warning, but
            // not to reports about disable comments.
            text: diag.stylelint_text(),
            url: diag.url.clone(),
            fixable: diag.fix.is_some(),
          }
        })
        .collect();

      StylelintResult {
        source: result.file_path.clone(),
        deprecations: Vec::new(),
        invalid_option_warnings: result
          .invalid_option_warnings
          .iter()
          .map(|text| InvalidOptionWarning { text: text.clone() })
          .collect(),
        parse_errors: Vec::new(),
        errored: result.errored(),
        warnings,
      }
    })
    .collect()
}

// ---------------------------------------------------------------------------
// Helper: display_path
// ---------------------------------------------------------------------------

/// How Stylelint's string formatter names a source in its header: relative
/// to `cwd`, with `/` between the parts (Node's `path.relative`).  A name in
/// angle brackets, such as `<input css 1>`, is no path and stays as it is.
pub fn display_path(path: &str, cwd: &Path) -> String {
  if path.starts_with('<') {
    return path.to_string();
  }
  let absolute = lexically_normal(&cwd.join(path));
  let base = lexically_normal(cwd);
  let parts: Vec<Component> = absolute.components().collect();
  let base_parts: Vec<Component> = base.components().collect();
  let common = parts
    .iter()
    .zip(&base_parts)
    .take_while(|(a, b)| a == b)
    .count();
  std::iter::repeat_n("..".to_string(), base_parts.len() - common)
    .chain(
      parts[common..]
        .iter()
        .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    )
    .collect::<Vec<_>>()
    .join("/")
}

/// `path` with `.` and `..` resolved without touching the file system, so
/// symlinks stay as written.  A `..` past the root is dropped.
pub fn lexically_normal(path: &Path) -> PathBuf {
  let mut normal = PathBuf::new();
  for component in path.components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        normal.pop();
      }
      other => normal.push(other),
    }
  }
  normal
}

/// `count` followed by `word`, with an `s` added unless `count` is 1:
/// `1 problem`, `2 problems`, `0 problems`.
fn plural(count: usize, word: &str) -> String {
  if count == 1 {
    format!("{count} {word}")
  } else {
    format!("{count} {word}s")
  }
}

/// The problem tally the text and agent formatters end with:
/// `3 problems (1 error, 2 warnings)`.
fn problem_summary(errors: usize, warnings: usize) -> String {
  format!(
    "{} ({}, {})",
    plural(errors + warnings, "problem"),
    plural(errors, "error"),
    plural(warnings, "warning")
  )
}

// ---------------------------------------------------------------------------
// TextFormatter
// ---------------------------------------------------------------------------

/// Human-readable formatter similar to Stylelint's string formatter.
///
/// ```text
/// src/app.css
///   2:3  ⚠  Unexpected empty block  block-no-empty
///
/// ✖ 1 problem (0 errors, 1 warning)
/// ```
///
/// `color` switches the ANSI styling on or off; the CLI decides it the way
/// picocolors does for Stylelint (flags, `NO_COLOR`, `FORCE_COLOR`, `CI`,
/// and whether stdout is a terminal).
#[derive(Debug, Clone, Copy)]
pub struct TextFormatter {
  pub color: bool,
}

impl Default for TextFormatter {
  /// Colour is on unless the CLI turns it off.
  fn default() -> Self {
    Self { color: true }
  }
}

/// Paints text with ANSI styles, or leaves it alone when colour is off.
#[derive(Debug, Clone, Copy)]
struct Palette {
  enabled: bool,
}

impl Palette {
  /// Red, for errors.
  fn red(self, text: &str) -> String {
    if self.enabled {
      text.red().to_string()
    } else {
      text.to_string()
    }
  }

  /// Yellow, for warnings and below.
  fn yellow(self, text: &str) -> String {
    if self.enabled {
      text.yellow().to_string()
    } else {
      text.to_string()
    }
  }

  /// Dimmed, for the trailing rule name.
  fn dimmed(self, text: &str) -> String {
    if self.enabled {
      text.dimmed().to_string()
    } else {
      text.to_string()
    }
  }

  /// Underlined, for file headers.
  fn underline(self, text: &str) -> String {
    if self.enabled {
      text.underline().to_string()
    } else {
      text.to_string()
    }
  }

  /// Bold, for the summary line.
  fn bold(self, text: &str) -> String {
    if self.enabled {
      text.bold().to_string()
    } else {
      text.to_string()
    }
  }
}

/// Each distinct invalid option across all results, in first-seen order.
fn distinct_invalid_options(results: &[StylelintResult]) -> Vec<&str> {
  let mut seen: Vec<&str> = Vec::new();
  for option in results.iter().flat_map(|r| &r.invalid_option_warnings) {
    if !seen.contains(&option.text.as_str()) {
      seen.push(&option.text);
    }
  }
  seen
}

/// Stylelint's `Invalid Option: <text>` lines, one per distinct invalid
/// option across all results, followed by a blank line; empty when there are
/// none.
fn invalid_options_section(results: &[StylelintResult], paint: Palette) -> String {
  let seen = distinct_invalid_options(results);
  if seen.is_empty() {
    return String::new();
  }
  let mut out = String::new();
  for text in seen {
    out.push_str(&paint.red("Invalid Option: "));
    out.push_str(text);
    out.push('\n');
  }
  out.push('\n');
  out
}

impl Formatter for TextFormatter {
  /// Lists invalid options first, as Stylelint does, then groups warnings
  /// under an underlined file header (the path relative to the working
  /// directory) and appends a `N problems (E errors, W warnings)` summary.
  fn format(&self, results: &[StylelintResult]) -> String {
    let paint = Palette {
      enabled: self.color,
    };
    let mut output = invalid_options_section(results, paint);
    let mut total_errors: usize = 0;
    let mut total_warnings: usize = 0;
    // Stylelint names each file relative to the working directory.
    let cwd = std::env::current_dir().unwrap_or_default();

    for result in results {
      if result.warnings.is_empty() {
        continue;
      }

      output.push_str(&paint.underline(&display_path(&result.source, &cwd)));
      output.push('\n');

      for warning in &result.warnings {
        let location = format!("{}:{}", warning.line, warning.column);
        let (icon, colored_message) = if warning.is_error() {
          total_errors += 1;
          (paint.red("\u{2716}"), paint.red(warning.message()))
        } else {
          total_warnings += 1;
          (paint.yellow("\u{26A0}"), paint.yellow(warning.message()))
        };

        let rule = paint.dimmed(&warning.rule);
        output.push_str(&format!(
          "  {location:<8} {icon}  {colored_message}  {rule}\n"
        ));
      }

      output.push('\n');
    }

    let total = total_errors + total_warnings;
    if total > 0 {
      let summary = format!("\u{2716} {}", problem_summary(total_errors, total_warnings));
      output.push_str(&paint.bold(&summary));
      output.push('\n');
    }

    output
  }
}

// ---------------------------------------------------------------------------
// JsonFormatter
// ---------------------------------------------------------------------------

/// JSON formatter matching Stylelint's JSON output format.
pub struct JsonFormatter;

impl Formatter for JsonFormatter {
  /// Emits the results model as it is, one object per source.
  fn format(&self, results: &[StylelintResult]) -> String {
    serde_json::to_string(results).unwrap_or_else(|_| "[]".to_string())
  }
}

// ---------------------------------------------------------------------------
// CompactFormatter
// ---------------------------------------------------------------------------

/// One-line-per-warning compact format.
///
/// ```text
/// src/app.css: line 2, col 3, warning - Unexpected empty block (block-no-empty)
/// ```
pub struct CompactFormatter;

impl Formatter for CompactFormatter {
  /// Emits `file: line N, col N, severity - message` per warning.
  fn format(&self, results: &[StylelintResult]) -> String {
    let mut output = String::new();

    for result in results {
      for warning in &result.warnings {
        output.push_str(&format!(
          "{}: line {}, col {}, {} - {}\n",
          result.source, warning.line, warning.column, warning.severity, warning.text,
        ));
      }
    }

    output
  }
}

// ---------------------------------------------------------------------------
// UnixFormatter
// ---------------------------------------------------------------------------

/// Unix-style `file:line:column: message [severity]`, matching Stylelint's
/// `unix` formatter.
///
/// ```text
/// src/app.css:2:3: Unexpected empty block [warning]
///
/// 1 problem
/// ```
pub struct UnixFormatter;

impl Formatter for UnixFormatter {
  /// Emits `file:line:col: message [severity]` per warning, then a total.
  fn format(&self, results: &[StylelintResult]) -> String {
    let mut output = String::new();
    let mut total = 0usize;

    for result in results {
      for warning in &result.warnings {
        output.push_str(&format!(
          "{}:{}:{}: {} [{}]\n",
          result.source, warning.line, warning.column, warning.text, warning.severity,
        ));
        total += 1;
      }
    }

    if total > 0 {
      output.push_str(&format!("\n{}\n", plural(total, "problem")));
    }

    output
  }
}

// ---------------------------------------------------------------------------
// AgentFormatter
// ---------------------------------------------------------------------------

/// Gale's own formatter for coding agents that run the linter after an edit
/// and act on what it prints.  Not a Stylelint formatter.
///
/// One compiler-style line per problem, with the path relative to the
/// working directory, the rule, and whether `gale --fix` can fix it; then a
/// one-line summary.  A clean run prints nothing, so silence means success.
///
/// ```text
/// src/app.css:1:12: error: Expected "#ffffff" to be "#fff" [color-hex-length, fixable]
/// src/app.css:2:3: warning: Unexpected empty block [block-no-empty]
/// 2 problems (1 error, 1 warning), 1 fixable with `gale --fix`
/// ```
pub struct AgentFormatter;

impl Formatter for AgentFormatter {
  /// Lists invalid options, then one line per problem, then the summary.
  fn format(&self, results: &[StylelintResult]) -> String {
    let mut output = String::new();
    let cwd = std::env::current_dir().unwrap_or_default();

    for text in distinct_invalid_options(results) {
      output.push_str(&format!("error: Invalid Option: {text}\n"));
    }

    let (mut errors, mut warnings, mut fixable) = (0usize, 0usize, 0usize);
    for result in results {
      if result.warnings.is_empty() {
        continue;
      }
      let path = display_path(&result.source, &cwd);
      for warning in &result.warnings {
        let severity = if warning.is_error() {
          errors += 1;
          "error"
        } else {
          warnings += 1;
          "warning"
        };
        let tag = if warning.fixable {
          fixable += 1;
          format!("{}, fixable", warning.rule)
        } else {
          warning.rule.clone()
        };
        output.push_str(&format!(
          "{path}:{}:{}: {severity}: {} [{tag}]\n",
          warning.line,
          warning.column,
          warning.message()
        ));
      }
    }

    let total = errors + warnings;
    if total > 0 {
      output.push_str(&problem_summary(errors, warnings));
      if fixable > 0 {
        output.push_str(&format!(", {fixable} fixable with `gale --fix`"));
      }
      output.push('\n');
    }

    output
  }
}

// ---------------------------------------------------------------------------
// TapFormatter
// ---------------------------------------------------------------------------

/// TAP version 13 output, matching Stylelint's `tap` formatter.
///
/// One test point per linted file; files with warnings emit a YAML block per
/// warning.
pub struct TapFormatter;

impl Formatter for TapFormatter {
  /// Emits one TAP test point per file; failing files carry a YAML block
  /// per warning.
  fn format(&self, results: &[StylelintResult]) -> String {
    let mut output = String::from("TAP version 13\n");
    output.push_str(&format!("1..{}\n", results.len()));

    for (i, result) in results.iter().enumerate() {
      let n = i + 1;
      if result.warnings.is_empty() {
        output.push_str(&format!("ok {n} - {}\n", result.source));
        continue;
      }

      output.push_str(&format!("not ok {n} - {}\n", result.source));
      for warning in &result.warnings {
        output.push_str("  ---\n");
        output.push_str(&format!("  message: {}\n", yaml_scalar(warning.message())));
        output.push_str(&format!("  severity: {}\n", warning.severity));
        output.push_str("  data:\n");
        output.push_str(&format!("    line: {}\n", warning.line));
        output.push_str(&format!("    column: {}\n", warning.column));
        output.push_str(&format!("    ruleId: {}\n", yaml_scalar(&warning.rule)));
        output.push_str("  ...\n");
      }
    }

    output
  }
}

/// Quote a value for a TAP YAML block, escaping what a double-quoted YAML
/// scalar cannot contain literally.
fn yaml_scalar(value: &str) -> String {
  let escaped = value
    .replace('\\', "\\\\")
    .replace('"', "\\\"")
    .replace('\n', "\\n");
  format!("\"{escaped}\"")
}

// ---------------------------------------------------------------------------
// VerboseFormatter
// ---------------------------------------------------------------------------

/// Stylelint's `verbose` formatter: the standard text output followed by a
/// summary of how many files were checked and which rules fired.
#[derive(Debug, Clone, Copy)]
pub struct VerboseFormatter {
  pub color: bool,
}

impl Default for VerboseFormatter {
  /// Colour is on unless the CLI turns it off.
  fn default() -> Self {
    Self { color: true }
  }
}

impl Formatter for VerboseFormatter {
  /// Text output followed by the file count and per-rule tallies.
  fn format(&self, results: &[StylelintResult]) -> String {
    let mut output = TextFormatter { color: self.color }.format(results);

    let file_count = results.len();
    if !output.is_empty() && !output.ends_with("\n\n") {
      output.push('\n');
    }
    output.push_str(&format!("{} checked\n", plural(file_count, "source")));

    // Per-rule tallies, most frequent first, then alphabetical so repeated
    // runs produce identical output.
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for result in results {
      for warning in &result.warnings {
        *counts.entry(warning.rule.as_str()).or_insert(0) += 1;
      }
    }

    if counts.is_empty() {
      return output;
    }

    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    output.push('\n');
    output.push_str("warnings by rule\n");
    for (rule, count) in ranked {
      output.push_str(&format!("  {rule}: {count}\n"));
    }

    output
  }
}

// ---------------------------------------------------------------------------
// ANSI stripping
// ---------------------------------------------------------------------------

/// Remove ANSI escape sequences (colours, styles, cursor moves) from `input`.
///
/// Used when a coloured report is written to a file, where Stylelint strips
/// the escapes too.
pub fn strip_ansi(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  let mut chars = input.chars().peekable();
  while let Some(c) = chars.next() {
    if c != '\x1b' {
      out.push(c);
      continue;
    }
    match chars.peek() {
      // CSI sequence: ESC [ <params> <final byte in 0x40..=0x7e>
      Some('[') => {
        chars.next();
        for next in chars.by_ref() {
          if ('\x40'..='\x7e').contains(&next) {
            break;
          }
        }
      }
      // Two-character escapes such as ESC ( B.
      Some(_) => {
        chars.next();
      }
      None => {}
    }
  }
  out
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Every formatter name `--formatter` accepts.
pub const FORMATTER_NAMES: &[&str] = &[
  "text", "string", "json", "compact", "verbose", "tap", "unix", "agent",
];

/// Create a formatter by name, choosing whether `text` and `verbose` colour
/// their output.  The machine-readable formatters never do.
///
/// `"string"` is an alias for `"text"`, matching Stylelint, where `string` is
/// the name of the default human-readable formatter.
///
/// Defaults to `TextFormatter` for unknown format types.
pub fn create_formatter_with_color(format_type: &str, color: bool) -> Box<dyn Formatter> {
  match format_type {
    "json" => Box::new(JsonFormatter),
    "compact" => Box::new(CompactFormatter),
    "unix" => Box::new(UnixFormatter),
    "agent" => Box::new(AgentFormatter),
    "tap" => Box::new(TapFormatter),
    "verbose" => Box::new(VerboseFormatter { color }),
    _ => Box::new(TextFormatter { color }),
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
