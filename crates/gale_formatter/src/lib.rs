use gale_diagnostics::{LintResult, Severity, SourceLineIndex, SourceLocation};
use owo_colors::OwoColorize;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Formatter trait
// ---------------------------------------------------------------------------

/// Trait for formatting lint results into a displayable string.
pub trait Formatter {
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
pub struct TextFormatter;

impl Formatter for TextFormatter {
  fn format(&self, results: &[LintResult]) -> String {
    let mut output = String::new();
    let mut total_errors: usize = 0;
    let mut total_warnings: usize = 0;

    for result in results {
      if result.diagnostics.is_empty() {
        continue;
      }

      let line_index = SourceLineIndex::build(&result.source);

      output.push_str(&result.file_path.underline().to_string());
      output.push('\n');

      for diag in &result.diagnostics {
        let (line, col) = line_index.offset_to_location(diag.span.offset);

        let location = format!("{line}:{col}");
        let (icon, colored_message) = match diag.severity {
          Severity::Error => {
            total_errors += 1;
            ("\u{2716}".red().to_string(), diag.message.red().to_string())
          }
          Severity::Warning => {
            total_warnings += 1;
            (
              "\u{26A0}".yellow().to_string(),
              diag.message.yellow().to_string(),
            )
          }
          Severity::Info | Severity::Hint => {
            total_warnings += 1;
            (
              "\u{26A0}".yellow().to_string(),
              diag.message.yellow().to_string(),
            )
          }
        };

        let rule = diag.rule_name.dimmed();
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
      output.push_str(&summary.bold().to_string());
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
  invalid_option_warnings: Vec<serde_json::Value>,
  #[serde(rename = "parseErrors")]
  parse_errors: Vec<serde_json::Value>,
  errored: bool,
  warnings: Vec<JsonWarning>,
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
}

impl Formatter for JsonFormatter {
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
            }
          })
          .collect();

        let errored = result
          .diagnostics
          .iter()
          .any(|d| d.severity == Severity::Error);

        JsonResult {
          source: result.file_path.clone(),
          deprecations: Vec::new(),
          invalid_option_warnings: Vec::new(),
          parse_errors: Vec::new(),
          errored,
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
// TapFormatter
// ---------------------------------------------------------------------------

/// TAP version 13 output, matching Stylelint's `tap` formatter.
///
/// One test point per linted file; files with warnings emit a YAML block per
/// warning.
pub struct TapFormatter;

impl Formatter for TapFormatter {
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
pub struct VerboseFormatter;

impl Formatter for VerboseFormatter {
  fn format(&self, results: &[LintResult]) -> String {
    let mut output = TextFormatter.format(results);

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
  "text", "string", "json", "compact", "verbose", "tap", "unix",
];

/// Create a formatter by name.
///
/// `"string"` is an alias for `"text"`, matching Stylelint, where `string` is
/// the name of the default human-readable formatter.
///
/// Defaults to `TextFormatter` for unknown format types.
pub fn create_formatter(format_type: &str) -> Box<dyn Formatter> {
  match format_type {
    "json" => Box::new(JsonFormatter),
    "compact" => Box::new(CompactFormatter),
    "unix" => Box::new(UnixFormatter),
    "tap" => Box::new(TapFormatter),
    "verbose" => Box::new(VerboseFormatter),
    _ => Box::new(TextFormatter),
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
    let formatter = TextFormatter;
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
  fn strip_ansi_removes_colour_codes_and_keeps_text() {
    let coloured =
      "\x1b[4msrc/app.css\x1b[0m\n  \x1b[31m\u{2716}\x1b[39m  plain \x1b[1mbold\x1b[22m";
    assert_eq!(strip_ansi(coloured), "src/app.css\n  \u{2716}  plain bold");
    assert_eq!(strip_ansi("no escapes"), "no escapes");
    assert_eq!(strip_ansi(""), "");
  }

  #[test]
  fn text_formatter_output_strips_clean() {
    let coloured = TextFormatter.format(&sample_results());
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
    let output = VerboseFormatter.format(&sample_results());
    assert!(output.contains("1 source checked"), "{output}");
    assert!(output.contains("block-no-empty: 1"), "{output}");
  }

  #[test]
  fn empty_results_produce_no_output() {
    let formatter = TextFormatter;
    let output = formatter.format(&[]);
    assert!(output.is_empty());
  }
}
