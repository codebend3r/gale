use std::path::{Component, Path, PathBuf};

use gale_diagnostics::{LintResult, Severity, SourceLineIndex, SourceLocation};
use owo_colors::OwoColorize;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Formatter trait
// ---------------------------------------------------------------------------

/// Trait for formatting lint results into a displayable string.
pub trait Formatter {
  /// Renders every result into the formatter's output string.
  fn format(&self, results: &[LintResult]) -> String;
}

// ---------------------------------------------------------------------------
// Helper: compute_location
// ---------------------------------------------------------------------------

/// Converts a byte offset into a 1-indexed (line, column) pair.
pub fn compute_location(source: &str, offset: usize) -> (usize, usize) {
  let loc = SourceLocation::from_offset(source, offset);
  (loc.line, loc.column)
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

/// `path` with `.` and `..` resolved without touching the file system.
fn lexically_normal(path: &Path) -> PathBuf {
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

/// Stylelint's `Invalid Option: <text>` lines, one per distinct invalid
/// option across all results, followed by a blank line; empty when there are
/// none.
fn invalid_options_section(results: &[LintResult], paint: Palette) -> String {
  let mut seen: Vec<&str> = Vec::new();
  for text in results.iter().flat_map(|r| &r.invalid_option_warnings) {
    if !seen.contains(&text.as_str()) {
      seen.push(text);
    }
  }
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
  fn format(&self, results: &[LintResult]) -> String {
    let paint = Palette {
      enabled: self.color,
    };
    let mut output = invalid_options_section(results, paint);
    let mut total_errors: usize = 0;
    let mut total_warnings: usize = 0;
    // Stylelint names each file relative to the working directory.
    let cwd = std::env::current_dir().unwrap_or_default();

    for result in results {
      if result.diagnostics.is_empty() {
        continue;
      }

      let line_index = SourceLineIndex::build(&result.source);

      output.push_str(&paint.underline(&display_path(&result.file_path, &cwd)));
      output.push('\n');

      for diag in &result.diagnostics {
        let (line, col) = line_index.offset_to_location(diag.span.offset);

        let location = format!("{line}:{col}");
        let (icon, colored_message) = match diag.severity {
          Severity::Error => {
            total_errors += 1;
            (paint.red("\u{2716}"), paint.red(&diag.message))
          }
          Severity::Warning | Severity::Info | Severity::Hint => {
            total_warnings += 1;
            (paint.yellow("\u{26A0}"), paint.yellow(&diag.message))
          }
        };

        let rule = paint.dimmed(&diag.rule_name);
        output.push_str(&format!(
          "  {location:<8} {icon}  {colored_message}  {rule}\n"
        ));
      }

      output.push('\n');
    }

    let total = total_errors + total_warnings;
    if total > 0 {
      let problem_word = if total == 1 { "problem" } else { "problems" };
      let error_word = if total_errors == 1 { "error" } else { "errors" };
      let warning_word = if total_warnings == 1 {
        "warning"
      } else {
        "warnings"
      };

      let summary = format!(
        "\u{2716} {total} {problem_word} ({total_errors} {error_word}, {total_warnings} {warning_word})"
      );
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

/// Mirrors Stylelint's per-source JSON object, including the fields Gale never
/// populates. Consumers (editors, CI reporters, `stylelint.lint()` shims) index
/// into these unconditionally, so they must be present even when empty.
#[derive(Serialize)]
struct JsonResult {
  source: String,
  deprecations: Vec<serde_json::Value>,
  #[serde(rename = "invalidOptionWarnings")]
  invalid_option_warnings: Vec<JsonInvalidOption>,
  #[serde(rename = "parseErrors")]
  parse_errors: Vec<serde_json::Value>,
  errored: bool,
  warnings: Vec<JsonWarning>,
}

/// One entry of Stylelint's `invalidOptionWarnings`.
#[derive(Serialize)]
struct JsonInvalidOption {
  text: String,
}

#[derive(Serialize)]
struct JsonWarning {
  line: usize,
  column: usize,
  #[serde(rename = "endLine")]
  end_line: usize,
  #[serde(rename = "endColumn")]
  end_column: usize,
  rule: String,
  severity: String,
  text: String,
  /// Only present when the rule's `url` secondary option is set, as in
  /// Stylelint, where an undefined `url` is dropped by `JSON.stringify`.
  #[serde(skip_serializing_if = "Option::is_none")]
  url: Option<String>,
}

impl Formatter for JsonFormatter {
  /// Emits one `JsonResult` per file, with 1-indexed start and end positions
  /// and Stylelint's `" (rule-name)"` message suffix.
  fn format(&self, results: &[LintResult]) -> String {
    let json_results: Vec<JsonResult> = results
      .iter()
      .map(|result| {
        let line_index = SourceLineIndex::build(&result.source);
        let warnings = result
          .diagnostics
          .iter()
          .map(|diag| {
            let (line, column) = line_index.offset_to_location(diag.span.offset);
            let (end_line, end_column) = line_index.offset_to_location(diag.span.end());
            // Stylelint appends " (rule-name)" to every rule
            // warning, but not to reports about disable comments.
            // Replicate both for byte-for-byte identical output.
            let text = diag.stylelint_text();
            JsonWarning {
              line,
              column,
              end_line,
              end_column,
              rule: diag.rule_name.clone(),
              severity: diag.severity.to_string(),
              text,
              url: diag.url.clone(),
            }
          })
          .collect();

        JsonResult {
          source: result.file_path.clone(),
          deprecations: Vec::new(),
          invalid_option_warnings: result
            .invalid_option_warnings
            .iter()
            .map(|text| JsonInvalidOption { text: text.clone() })
            .collect(),
          parse_errors: Vec::new(),
          errored: result.errored(),
          warnings,
        }
      })
      .collect();

    serde_json::to_string(&json_results).unwrap_or_else(|_| "[]".to_string())
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
  fn format(&self, results: &[LintResult]) -> String {
    let mut output = String::new();

    for result in results {
      let line_index = SourceLineIndex::build(&result.source);
      for diag in &result.diagnostics {
        let (line, col) = line_index.offset_to_location(diag.span.offset);
        output.push_str(&format!(
          "{}: line {}, col {}, {} - {}\n",
          result.file_path,
          line,
          col,
          diag.severity,
          diag.stylelint_text(),
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
  fn format(&self, results: &[LintResult]) -> String {
    let mut output = String::new();
    let mut total = 0usize;

    for result in results {
      let line_index = SourceLineIndex::build(&result.source);
      for diag in &result.diagnostics {
        let (line, col) = line_index.offset_to_location(diag.span.offset);
        output.push_str(&format!(
          "{}:{}:{}: {} [{}]\n",
          result.file_path,
          line,
          col,
          diag.stylelint_text(),
          diag.severity,
        ));
        total += 1;
      }
    }

    if total > 0 {
      let plural = if total == 1 { "problem" } else { "problems" };
      output.push_str(&format!("\n{total} {plural}\n"));
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
  fn format(&self, results: &[LintResult]) -> String {
    let mut output = String::new();
    let cwd = std::env::current_dir().unwrap_or_default();

    let mut seen: Vec<&str> = Vec::new();
    for text in results.iter().flat_map(|r| &r.invalid_option_warnings) {
      if !seen.contains(&text.as_str()) {
        seen.push(text);
        output.push_str(&format!("error: Invalid Option: {text}\n"));
      }
    }

    let (mut errors, mut warnings, mut fixable) = (0usize, 0usize, 0usize);
    for result in results {
      if result.diagnostics.is_empty() {
        continue;
      }
      let path = display_path(&result.file_path, &cwd);
      let line_index = SourceLineIndex::build(&result.source);
      for diag in &result.diagnostics {
        let (line, col) = line_index.offset_to_location(diag.span.offset);
        let severity = match diag.severity {
          Severity::Error => {
            errors += 1;
            "error"
          }
          Severity::Warning | Severity::Info | Severity::Hint => {
            warnings += 1;
            "warning"
          }
        };
        let tag = if diag.fix.is_some() {
          fixable += 1;
          format!("{}, fixable", diag.rule_name)
        } else {
          diag.rule_name.clone()
        };
        output.push_str(&format!(
          "{path}:{line}:{col}: {severity}: {} [{tag}]\n",
          diag.message
        ));
      }
    }

    let total = errors + warnings;
    if total > 0 {
      let plural = |n: usize, word: &str| {
        if n == 1 {
          format!("{n} {word}")
        } else {
          format!("{n} {word}s")
        }
      };
      output.push_str(&format!(
        "{} ({}, {})",
        plural(total, "problem"),
        plural(errors, "error"),
        plural(warnings, "warning")
      ));
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
  fn format(&self, results: &[LintResult]) -> String {
    let mut output = String::from("TAP version 13\n");
    output.push_str(&format!("1..{}\n", results.len()));

    for (i, result) in results.iter().enumerate() {
      let n = i + 1;
      if result.diagnostics.is_empty() {
        output.push_str(&format!("ok {n} - {}\n", result.file_path));
        continue;
      }

      output.push_str(&format!("not ok {n} - {}\n", result.file_path));
      let line_index = SourceLineIndex::build(&result.source);
      for diag in &result.diagnostics {
        let (line, col) = line_index.offset_to_location(diag.span.offset);
        output.push_str("  ---\n");
        output.push_str(&format!("  message: {}\n", yaml_scalar(&diag.message)));
        output.push_str(&format!("  severity: {}\n", diag.severity));
        output.push_str("  data:\n");
        output.push_str(&format!("    line: {line}\n"));
        output.push_str(&format!("    column: {col}\n"));
        output.push_str(&format!("    ruleId: {}\n", yaml_scalar(&diag.rule_name)));
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
  fn format(&self, results: &[LintResult]) -> String {
    let mut output = TextFormatter { color: self.color }.format(results);

    let file_count = results.len();
    let plural = if file_count == 1 { "source" } else { "sources" };
    if !output.is_empty() && !output.ends_with("\n\n") {
      output.push('\n');
    }
    output.push_str(&format!("{file_count} {plural} checked\n"));

    // Per-rule tallies, most frequent first, then alphabetical so repeated
    // runs produce identical output.
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for result in results {
      for diag in &result.diagnostics {
        *counts.entry(diag.rule_name.as_str()).or_insert(0) += 1;
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

/// Create a formatter by name, with colour on for the human-readable ones.
///
/// `"string"` is an alias for `"text"`, matching Stylelint, where `string` is
/// the name of the default human-readable formatter.
///
/// Defaults to `TextFormatter` for unknown format types.
pub fn create_formatter(format_type: &str) -> Box<dyn Formatter> {
  create_formatter_with_color(format_type, true)
}

/// Create a formatter by name, choosing whether `text` and `verbose` colour
/// their output.  The machine-readable formatters never do.
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
mod tests {
  use super::*;
  use gale_diagnostics::{Diagnostic, Span};

  fn sample_results() -> Vec<LintResult> {
    let source = "a {\n  \n}\n";
    let diag = Diagnostic::new("block-no-empty", "Unexpected empty block")
      .severity(Severity::Warning)
      .span(Span::new(4, 3))
      .file_path("src/app.css");

    vec![LintResult::new("src/app.css", source, vec![diag])]
  }

  #[test]
  fn text_formatter_output() {
    let formatter = TextFormatter::default();
    let output = formatter.format(&sample_results());
    assert!(output.contains("src/app.css"));
    assert!(output.contains("Unexpected empty block"));
    assert!(output.contains("block-no-empty"));
    assert!(output.contains("1 problem"));
  }

  #[test]
  fn json_result_carries_every_stylelint_field() {
    let output = JsonFormatter.format(&sample_results());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let entry = &parsed[0];
    for field in [
      "source",
      "deprecations",
      "invalidOptionWarnings",
      "parseErrors",
      "errored",
      "warnings",
    ] {
      assert!(!entry[field].is_null(), "missing result field {field}");
    }
    let warning = &entry["warnings"][0];
    for field in [
      "line",
      "column",
      "endLine",
      "endColumn",
      "rule",
      "severity",
      "text",
    ] {
      assert!(!warning[field].is_null(), "missing warning field {field}");
    }
    // The sample is a warning, not an error.
    assert_eq!(entry["errored"], serde_json::json!(false));
  }

  /// Two files that both carry the same invalid option, and no problems.
  fn results_with_invalid_option() -> Vec<LintResult> {
    let text = "Invalid option value \"[\" for rule \"selector-class-pattern\": bad";
    ["a.css", "b.css"]
      .into_iter()
      .map(|path| {
        let mut result = LintResult::new(path, "a {}", vec![]);
        result.invalid_option_warnings.push(text.to_string());
        result
      })
      .collect()
  }

  #[test]
  fn json_lists_invalid_options_and_marks_the_result_errored() {
    let output = JsonFormatter.format(&results_with_invalid_option());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
      parsed[0]["invalidOptionWarnings"],
      serde_json::json!([{
        "text": "Invalid option value \"[\" for rule \"selector-class-pattern\": bad"
      }])
    );
    assert_eq!(parsed[0]["errored"], serde_json::json!(true));
    assert_eq!(parsed[0]["warnings"], serde_json::json!([]));
  }

  #[test]
  fn text_lists_each_invalid_option_once_before_the_problems() {
    let output = TextFormatter { color: false }.format(&results_with_invalid_option());
    assert_eq!(
      output,
      "Invalid Option: Invalid option value \"[\" for rule \"selector-class-pattern\": bad\n\n"
    );

    let mut mixed = sample_results();
    mixed.extend(results_with_invalid_option());
    let output = TextFormatter { color: false }.format(&mixed);
    assert!(output.starts_with("Invalid Option: "), "{output}");
    assert_eq!(output.matches("Invalid Option: ").count(), 1);
    assert!(output.contains("Unexpected empty block"));
  }

  #[test]
  fn json_formatter_output() {
    let formatter = JsonFormatter;
    let output = formatter.format(&sample_results());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["source"], "src/app.css");
    assert_eq!(arr[0]["warnings"][0]["rule"], "block-no-empty");
    assert_eq!(arr[0]["warnings"][0]["severity"], "warning");
  }

  #[test]
  fn json_columns_count_utf16_units() {
    // Stylelint's columns are JavaScript string indices: the emoji before
    // the block takes two, `é` one.
    let source = "a::before { content: \"😀é\"; } b {}\n";
    let at = source.rfind("{}").unwrap();
    let diag = Diagnostic::new("block-no-empty", "Unexpected empty block")
      .severity(Severity::Error)
      .span(Span::new(at, 2));
    let output = JsonFormatter.format(&[LintResult::new("a.css", source, vec![diag])]);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let warning = &parsed[0]["warnings"][0];
    assert_eq!(warning["column"], 33);
    assert_eq!(warning["endColumn"], 35);
  }

  #[test]
  fn display_paths_are_relative_to_the_working_directory() {
    let cwd = Path::new("/work/project");
    assert_eq!(display_path("/work/project/src/a.css", cwd), "src/a.css");
    assert_eq!(display_path("/work/other/b.css", cwd), "../other/b.css");
    assert_eq!(display_path("/work/project/src/../a.css", cwd), "a.css");
    assert_eq!(display_path("./src/a.css", cwd), "src/a.css");
    assert_eq!(display_path("stdin.css", cwd), "stdin.css");
    assert_eq!(display_path("<input css 1>", cwd), "<input css 1>");
  }

  #[test]
  fn compact_formatter_output() {
    let formatter = CompactFormatter;
    let output = formatter.format(&sample_results());
    assert!(
      output
        .contains("src/app.css: line 2, col 1, warning - Unexpected empty block (block-no-empty)")
    );
  }

  fn comment_problem_results() -> Vec<LintResult> {
    let source = "/* stylelint-disable x */\na {}\n";
    let diag = Diagnostic::new("--report-needless-disables", "Needless disable for \"x\"")
      .severity(Severity::Error)
      .span(Span::new(0, 0));
    vec![LintResult::new("src/app.css", source, vec![diag])]
  }

  #[test]
  fn json_warning_carries_url_only_when_set() {
    let output = JsonFormatter.format(&sample_results());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed[0]["warnings"][0].get("url").is_none());

    let source = "a {\n  \n}\n";
    let diag = Diagnostic::new("block-no-empty", "Unexpected empty block")
      .span(Span::new(4, 3))
      .url("https://example.com/empty");
    let results = vec![LintResult::new("src/app.css", source, vec![diag])];
    let output = JsonFormatter.format(&results);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed[0]["warnings"][0]["url"], "https://example.com/empty");
  }

  #[test]
  fn comment_problems_have_no_rule_suffix_in_json() {
    let output = JsonFormatter.format(&comment_problem_results());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
      parsed[0]["warnings"][0]["text"],
      "Needless disable for \"x\""
    );
    assert_eq!(
      parsed[0]["warnings"][0]["rule"],
      "--report-needless-disables"
    );
  }

  #[test]
  fn comment_problems_have_no_rule_suffix_in_compact_and_unix() {
    let compact = CompactFormatter.format(&comment_problem_results());
    assert!(
      compact.contains("error - Needless disable for \"x\"\n"),
      "{compact}"
    );
    let unix = UnixFormatter.format(&comment_problem_results());
    assert!(
      unix.contains(": Needless disable for \"x\" [error]"),
      "{unix}"
    );
  }

  #[test]
  fn text_formatter_without_colour_matches_the_stripped_coloured_output() {
    let coloured = TextFormatter { color: true }.format(&sample_results());
    let plain = TextFormatter { color: false }.format(&sample_results());
    assert!(coloured.contains('\x1b'));
    assert!(!plain.contains('\x1b'));
    assert_eq!(plain, strip_ansi(&coloured));
  }

  #[test]
  fn verbose_formatter_follows_the_colour_switch() {
    assert!(
      VerboseFormatter { color: true }
        .format(&sample_results())
        .contains('\x1b')
    );
    assert!(
      !VerboseFormatter { color: false }
        .format(&sample_results())
        .contains('\x1b')
    );
  }

  #[test]
  fn factory_colour_switch_only_affects_human_formatters() {
    for name in ["text", "string", "verbose"] {
      let out = create_formatter_with_color(name, false).format(&sample_results());
      assert!(!out.contains('\x1b'), "{name}");
      let out = create_formatter_with_color(name, true).format(&sample_results());
      assert!(out.contains('\x1b'), "{name}");
    }
    for name in ["json", "compact", "unix", "tap"] {
      let out = create_formatter_with_color(name, true).format(&sample_results());
      assert!(!out.contains('\x1b'), "{name}");
    }
  }

  #[test]
  fn strip_ansi_removes_colour_codes_and_keeps_text() {
    let coloured =
      "\x1b[4msrc/app.css\x1b[0m\n  \x1b[31m\u{2716}\x1b[39m  plain \x1b[1mbold\x1b[22m";
    assert_eq!(strip_ansi(coloured), "src/app.css\n  \u{2716}  plain bold");
    assert_eq!(strip_ansi("no escapes"), "no escapes");
    assert_eq!(strip_ansi(""), "");
  }

  #[test]
  fn text_formatter_output_strips_clean() {
    let coloured = TextFormatter::default().format(&sample_results());
    let plain = strip_ansi(&coloured);
    assert!(!plain.contains('\x1b'));
    assert!(plain.contains("src/app.css\n"));
    assert!(plain.contains("Unexpected empty block  block-no-empty"));
  }

  #[test]
  fn compute_location_basic() {
    let source = "abc\ndef\nghi";
    let (line, col) = compute_location(source, 5);
    assert_eq!(line, 2);
    assert_eq!(col, 2);
  }

  #[test]
  fn compute_location_start_of_file() {
    let (line, col) = compute_location("hello", 0);
    assert_eq!(line, 1);
    assert_eq!(col, 1);
  }

  #[test]
  fn create_formatter_returns_correct_types() {
    let _ = create_formatter("text");
    let _ = create_formatter("json");
    let _ = create_formatter("compact");
    let _ = create_formatter("unknown");
  }

  #[test]
  fn every_advertised_formatter_name_is_constructible() {
    for name in FORMATTER_NAMES {
      let out = create_formatter(name).format(&sample_results());
      assert!(!out.is_empty(), "formatter {name} produced no output");
    }
  }

  #[test]
  fn unix_formatter_output() {
    let output = UnixFormatter.format(&sample_results());
    assert!(
      output.contains("src/app.css:2:1: Unexpected empty block (block-no-empty) [warning]"),
      "unexpected unix output: {output}"
    );
    assert!(output.contains("1 problem"), "missing summary: {output}");
  }

  #[test]
  fn agent_formatter_marks_fixable_problems_and_sums_up() {
    use gale_diagnostics::{Edit, Fix};

    let source = "a { color: #ffffff; }\nb {}\n";
    let hex = Diagnostic::new("color-hex-length", "Expected \"#ffffff\" to be \"#fff\"")
      .severity(Severity::Error)
      .span(Span::new(11, 7))
      .fix(Fix::new(
        "Shorten the hex colour",
        vec![Edit::new(Span::new(11, 7), "#fff")],
      ));
    let empty = Diagnostic::new("block-no-empty", "Unexpected empty block")
      .severity(Severity::Warning)
      .span(Span::new(24, 2));
    let results = vec![LintResult::new("a.css", source, vec![hex, empty])];

    assert_eq!(
      AgentFormatter.format(&results),
      "a.css:1:12: error: Expected \"#ffffff\" to be \"#fff\" [color-hex-length, fixable]\n\
       a.css:2:3: warning: Unexpected empty block [block-no-empty]\n\
       2 problems (1 error, 1 warning), 1 fixable with `gale --fix`\n"
    );
  }

  #[test]
  fn agent_formatter_prints_nothing_for_a_clean_run() {
    let results = vec![LintResult::new("a.css", "a { color: red; }\n", vec![])];
    assert_eq!(AgentFormatter.format(&results), "");
  }

  #[test]
  fn agent_formatter_leaves_out_the_fixable_clause_when_nothing_is_fixable() {
    let output = AgentFormatter.format(&sample_results());
    assert!(
      output.ends_with("1 problem (0 errors, 1 warning)\n"),
      "{output}"
    );
  }

  #[test]
  fn tap_formatter_output() {
    let output = TapFormatter.format(&sample_results());
    assert!(output.starts_with("TAP version 13\n1..1\n"), "{output}");
    assert!(output.contains("not ok 1 - src/app.css"), "{output}");
    assert!(output.contains("    line: 2"), "{output}");
    assert!(
      output.contains("    ruleId: \"block-no-empty\""),
      "{output}"
    );
  }

  #[test]
  fn tap_formatter_marks_clean_files_ok() {
    let clean = vec![LintResult::new("src/ok.css", "a { color: red; }", vec![])];
    let output = TapFormatter.format(&clean);
    assert!(output.contains("ok 1 - src/ok.css"), "{output}");
    assert!(!output.contains("not ok"), "{output}");
  }

  #[test]
  fn tap_formatter_escapes_quotes_in_messages() {
    let diag = Diagnostic::new("color-hex-length", "Expected \"#FFFFFF\" to be \"#fff\"")
      .severity(Severity::Warning)
      .span(Span::new(0, 1))
      .file_path("src/app.css");
    let results = vec![LintResult::new("src/app.css", "a{}", vec![diag])];
    let output = TapFormatter.format(&results);
    // The YAML scalar must carry backslash-escaped quotes.
    assert!(
      output.contains("\\\"#FFFFFF\\\""),
      "quotes not escaped: {output}"
    );
  }

  #[test]
  fn verbose_formatter_appends_a_summary() {
    let output = VerboseFormatter::default().format(&sample_results());
    assert!(output.contains("1 source checked"), "{output}");
    assert!(output.contains("block-no-empty: 1"), "{output}");
  }

  #[test]
  fn empty_results_produce_no_output() {
    let formatter = TextFormatter::default();
    let output = formatter.format(&[]);
    assert!(output.is_empty());
  }
}
