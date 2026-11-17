use super::*;
use gale_diagnostics::{Diagnostic, Severity, Span};

fn sample_results() -> Vec<StylelintResult> {
  let source = "a {\n  \n}\n";
  let diag = Diagnostic::new("block-no-empty", "Unexpected empty block")
    .severity(Severity::Warning)
    .span(Span::new(4, 3))
    .file_path("src/app.css");

  stylelint_results(&[LintResult::new("src/app.css", source, vec![diag])])
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
fn results_with_invalid_option() -> Vec<StylelintResult> {
  let text = "Invalid option value \"[\" for rule \"selector-class-pattern\": bad";
  let results: Vec<LintResult> = ["a.css", "b.css"]
    .into_iter()
    .map(|path| {
      let mut result = LintResult::new(path, "a {}", vec![]);
      result.invalid_option_warnings.push(text.to_string());
      result
    })
    .collect();
  stylelint_results(&results)
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
  let output = JsonFormatter.format(&stylelint_results(&[LintResult::new(
    "a.css",
    source,
    vec![diag],
  )]));
  let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
  let warning = &parsed[0]["warnings"][0];
  assert_eq!(warning["column"], 33);
  assert_eq!(warning["endColumn"], 35);
}

#[test]
fn an_empty_span_at_a_line_end_ends_one_column_on() {
  // A problem at one index ends one column after it on the same line,
  // as PostCSS's `rangeBy` has it, even at a line break or the end of
  // the text, where no span could say so.
  let source = "a {}\nb {}";
  let at = |offset: usize, length: usize| {
    Diagnostic::new("r", "m")
      .severity(Severity::Error)
      .span(Span::new(offset, length))
  };
  let diags = vec![at(4, 0), at(source.len(), 0), at(0, 0)];
  let results = stylelint_results(&[LintResult::new("a.css", source, diags)]);
  let ends: Vec<_> = results[0]
    .warnings
    .iter()
    .map(|w| (w.line, w.column, w.end_line, w.end_column))
    .collect();
  assert!(ends.contains(&(1, 5, Some(1), Some(6))), "{ends:?}");
  assert!(ends.contains(&(2, 5, Some(2), Some(6))), "{ends:?}");
  // Elsewhere an empty span still ends where it starts.
  assert!(ends.contains(&(1, 1, Some(1), Some(1))), "{ends:?}");
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

fn comment_problem_results() -> Vec<StylelintResult> {
  let source = "/* stylelint-disable x */\na {}\n";
  let diag = Diagnostic::new("--report-needless-disables", "Needless disable for \"x\"")
    .severity(Severity::Error)
    .span(Span::new(0, 0));
  stylelint_results(&[LintResult::new("src/app.css", source, vec![diag])])
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
  let results = stylelint_results(&[LintResult::new("src/app.css", source, vec![diag])]);
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
  let coloured = "\x1b[4msrc/app.css\x1b[0m\n  \x1b[31m\u{2716}\x1b[39m  plain \x1b[1mbold\x1b[22m";
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
fn create_formatter_returns_correct_types() {
  let _ = create_formatter_with_color("text", true);
  let _ = create_formatter_with_color("json", true);
  let _ = create_formatter_with_color("compact", true);
  let _ = create_formatter_with_color("unknown", true);
}

#[test]
fn every_advertised_formatter_name_is_constructible() {
  for name in FORMATTER_NAMES {
    let out = create_formatter_with_color(name, true).format(&sample_results());
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
  let results = stylelint_results(&[LintResult::new("a.css", source, vec![hex, empty])]);

  assert_eq!(
    AgentFormatter.format(&results),
    "a.css:1:12: error: Expected \"#ffffff\" to be \"#fff\" [color-hex-length, fixable]\n\
       a.css:2:3: warning: Unexpected empty block [block-no-empty]\n\
       2 problems (1 error, 1 warning), 1 fixable with `gale --fix`\n"
  );
}

#[test]
fn agent_formatter_prints_nothing_for_a_clean_run() {
  let results = stylelint_results(&[LintResult::new("a.css", "a { color: red; }\n", vec![])]);
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
  let clean = stylelint_results(&[LintResult::new("src/ok.css", "a { color: red; }", vec![])]);
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
  let results = stylelint_results(&[LintResult::new("src/app.css", "a{}", vec![diag])]);
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

#[test]
fn message_drops_exactly_the_rule_suffix() {
  let warning = |rule: &str, text: &str| StylelintWarning {
    rule: rule.to_string(),
    text: text.to_string(),
    ..StylelintWarning::default()
  };
  assert_eq!(warning("a", "Bad (a)").message(), "Bad");
  assert_eq!(warning("a", "Bad (a) (a)").message(), "Bad (a)");
  assert_eq!(warning("a", "Bad(a)").message(), "Bad(a)");
  assert_eq!(warning("a", "Bad (b)").message(), "Bad (b)");
  assert_eq!(
    warning("--report-needless-disables", "Needless disable").message(),
    "Needless disable"
  );
}

#[test]
fn every_formatter_renders_the_json_round_trip_the_same() {
  let mut results = sample_results();
  results.extend(comment_problem_results());
  results.extend(results_with_invalid_option());
  let parsed: Vec<StylelintResult> = serde_json::from_str(&JsonFormatter.format(&results)).unwrap();
  for name in FORMATTER_NAMES {
    for color in [false, true] {
      let formatter = create_formatter_with_color(name, color);
      assert_eq!(
        formatter.format(&parsed),
        formatter.format(&results),
        "{name}"
      );
    }
  }
}

#[test]
fn results_read_from_json_may_leave_fields_out() {
  let input = r#"[{"source":"a.css","warnings":[
      {"line":1,"column":3,"rule":"r","severity":"error","text":"Say \"no\" (r)"}
    ]}]"#;
  let results: Vec<StylelintResult> = serde_json::from_str(input).unwrap();
  assert!(
    TapFormatter
      .format(&results)
      .contains("  message: \"Say \\\"no\\\"\"\n"),
  );
  // Positions the input did not give are not made up.
  let json = JsonFormatter.format(&results);
  assert!(!json.contains("endLine"), "{json}");
  assert!(json.contains(r#""invalidOptionWarnings":[]"#), "{json}");
}
