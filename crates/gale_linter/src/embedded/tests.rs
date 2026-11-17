use super::*;
use gale_diagnostics::{Edit, Fix, Severity, Span, apply_fixes};

fn lint_host(
  source: &str,
  file_path: &str,
  host: HostLanguage,
  options: DisablePolicy,
  lint_css: impl FnMut(&str, Syntax, &EmbeddedRoot) -> LintResult,
) -> LintResult {
  super::lint_host(source, file_path, host, &options, lint_css)
}

/// A stand-in linter: one `fake` problem at every `!`, with a fix
/// replacing it by `?`, plus whatever `extra` adds.
fn fake_lint(text: &str, _syntax: Syntax, _root: &EmbeddedRoot) -> LintResult {
  let diagnostics = text
    .match_indices('!')
    .map(|(at, _)| {
      Diagnostic::new("fake", "bang")
        .severity(Severity::Error)
        .span(Span::new(at, 1))
        .fix(Fix::new("unbang", vec![Edit::new(Span::new(at, 1), "?")]))
    })
    .collect();
  LintResult::new("x.vue", text, diagnostics)
}

#[test]
fn problems_land_at_host_offsets() {
  let source = "<template><p>!</p></template>\n<style>\n.a { b: c! }\n</style>\n<style lang=\"scss\">!</style>";
  let result = lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    fake_lint,
  );
  let offsets: Vec<usize> = result.diagnostics.iter().map(|d| d.span.offset).collect();
  let bangs: Vec<usize> = source
    .match_indices('!')
    .map(|(at, _)| at)
    .skip(1)
    .collect();
  assert_eq!(offsets, bangs);
  assert_eq!(result.source, source);
  assert_eq!(result.file_path, "x.vue");
}

#[test]
fn fixes_rewrite_only_the_style_blocks() {
  let source = "<template><p>!</p></template>\n<style>.a{b:c!}</style><div style=\"x:y!\"></div>";
  let result = lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    fake_lint,
  );
  let (fixed, count) = apply_fixes(source, &result.diagnostics);
  assert_eq!(count, 2);
  assert_eq!(
    fixed,
    "<template><p>!</p></template>\n<style>.a{b:c?}</style><div style=\"x:y?\"></div>"
  );
}

#[test]
fn fixes_that_leave_the_block_are_dropped() {
  let source = "<style>a!</style>";
  let result = lint_host(
    source,
    "x.html",
    HostLanguage::Html,
    DisablePolicy::default(),
    |text, _, _| {
      let reach = Diagnostic::new("reach", "too far")
        .span(Span::new(0, 1))
        .fix(Fix::new(
          "eat",
          vec![Edit::new(Span::new(0, text.len() + 3), "")],
        ));
      LintResult::new("x.html", text, vec![reach])
    },
  );
  assert!(result.diagnostics[0].fix.is_none());
}

#[test]
fn blocks_get_their_own_syntax_and_unsupported_ones_are_skipped() {
  let source = "<style>a</style><style lang=\"less\">b</style><style lang=\"sass\">c</style>\
                  <style lang=\"stylus\">d</style><p style=\"e: f\"></p>";
  let mut seen = Vec::new();
  lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    |text, syntax, _| {
      seen.push((text.to_string(), syntax));
      LintResult::new("x.vue", text, Vec::new())
    },
  );
  assert_eq!(
    seen,
    vec![
      ("a".to_string(), Syntax::Css),
      ("b".to_string(), Syntax::Less),
      ("c".to_string(), Syntax::Sass),
      ("e: f".to_string(), Syntax::Scss),
    ]
  );
  assert_eq!(
    unsupported_blocks(source, HostLanguage::Vue),
    vec!["stylus"]
  );
}

#[test]
fn template_expressions_become_placeholders_of_the_same_length() {
  let source = "<div style=\"width: {size}px; color: {c ? 'é' : 'b'}\"></div>";
  let mut seen = String::new();
  lint_host(
    source,
    "x.svelte",
    HostLanguage::Svelte,
    DisablePolicy::default(),
    |text, _, _| {
      seen = text.to_string();
      LintResult::new("x.svelte", text, Vec::new())
    },
  );
  assert_eq!(seen, "width: $_____px; color: $_______________");
  assert_eq!(seen.len(), "width: {size}px; color: {c ? 'é' : 'b'}".len());
}

#[test]
fn some_rules_skip_embedded_roots() {
  let source = "<style>a</style><p style=\"b\"></p>";
  let rules = [
    "@stylistic/unicode-bom",
    "unicode-bom",
    "nesting-selector-no-missing-scoping-root",
    "@stylistic/no-empty-first-line",
    "no-missing-end-of-source-newline",
    "no-invalid-position-declaration",
    "color-named",
  ];
  let result = lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    |text, _, root| {
      let diagnostics = rules
        .iter()
        .filter(|r| root.allows(r))
        .map(|r| Diagnostic::new(*r, "x"))
        .collect();
      LintResult::new("x.vue", text, diagnostics)
    },
  );
  let mut reported: Vec<(usize, &str)> = result
    .diagnostics
    .iter()
    .map(|d| (d.span.offset, d.rule_name.as_str()))
    .collect();
  reported.sort();
  assert_eq!(
    reported,
    vec![
      (7, "@stylistic/no-empty-first-line"),
      (7, "color-named"),
      (7, "no-invalid-position-declaration"),
      (7, "no-missing-end-of-source-newline"),
      (26, "color-named"),
    ]
  );
}

#[test]
fn duplicate_selector_messages_name_the_host_line() {
  let source = "<template/>\n<style>\n.a {}\n.a {}\n</style>";
  let result =
    crate::RuleSet::of_rules(["no-duplicate-selectors"]).lint(source, "x.vue", Syntax::Css);
  assert_eq!(
    result.diagnostics[0].message,
    "Unexpected duplicate selector \".a\", first used at line 3"
  );
  let first = result.diagnostics[0].related[0].span;
  assert_eq!(&source[first.offset..first.end()], ".a");
}

#[test]
fn open_disables_carry_into_later_blocks() {
  let source = "<style>\n/* stylelint-disable fake */\na{!}\n</style>\n\
                  <style>\na{!}\n/* stylelint-enable fake */\na{!}\n</style>\n\
                  <style>/* stylelint-disable */</style><style>a{!}</style>";
  // Lines 2 to 7 are off for `fake`, and everything from the last line,
  // the line of the blanket disable, on.
  let result = lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    fake_lint,
  );
  let offsets: Vec<usize> = result.diagnostics.iter().map(|d| d.span.offset).collect();
  let bangs: Vec<usize> = source.match_indices('!').map(|(at, _)| at).collect();
  assert_eq!(offsets, vec![bangs[2]]);

  let ignored = lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy {
      options: crate::DisableOptions {
        ignore: true,
        ..Default::default()
      },
      ..Default::default()
    },
    fake_lint,
  );
  assert_eq!(ignored.diagnostics.len(), 4);
}

#[test]
fn a_rejected_command_anywhere_replaces_the_files_problems() {
  let source = "<style>a{!}</style>\n<style>\n/* stylelint-enable fake */\n</style>";
  for ignore_disables in [false, true] {
    let result = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      DisablePolicy {
        options: crate::DisableOptions {
          ignore: ignore_disables,
          ..Default::default()
        },
        ..Default::default()
      },
      fake_lint,
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "CssSyntaxError");
    assert_eq!(d.message, "\"fake\" has not been disabled");
    assert_eq!(
      &source[d.span.offset..d.span.end()],
      "/* stylelint-enable fake */"
    );
  }
  // An enable for a disable in an earlier block is fine.
  let source = "<style>\n/* stylelint-disable fake */\n</style>\n\
                  <style>\n/* stylelint-enable fake */\na{!}\n</style>";
  let result = lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    fake_lint,
  );
  let rules: Vec<&str> = result
    .diagnostics
    .iter()
    .map(|d| d.rule_name.as_str())
    .collect();
  assert_eq!(rules, vec!["fake"]);
}

#[test]
fn invalid_options_are_reported_once() {
  let source = "<style>a</style><style>b</style>";
  let result = lint_host(
    source,
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    |text, _, _| {
      let mut r = LintResult::new("x.vue", text, Vec::new());
      r.invalid_option_warnings.push("bad option".to_string());
      r
    },
  );
  assert_eq!(result.invalid_option_warnings, vec!["bad option"]);
}

#[test]
fn a_file_without_styles_has_no_problems() {
  let result = lint_host(
    "<template><div/></template>",
    "x.vue",
    HostLanguage::Vue,
    DisablePolicy::default(),
    fake_lint,
  );
  assert!(result.diagnostics.is_empty());
}

/// The indentation context seen while linting each root of `source`.
fn indentation_seen(
  source: &str,
  space: Option<usize>,
  base: Option<i64>,
) -> Vec<Option<EmbeddedIndentation>> {
  let mut seen = Vec::new();
  lint_host(
    source,
    "x.html",
    HostLanguage::Html,
    DisablePolicy::default(),
    |text, _, root| {
      seen.push(Some(indentation_context(root, text, space, base)));
      LintResult::new("x.html", text, Vec::new())
    },
  );
  seen
}

#[test]
fn indentation_base_level_is_inferred_from_the_markup() {
  let html = "<html>\n  <head>\n    <style>\n      a {\n        color: red;\n      }\n    </style>\n  </head>\n</html>\n";
  // The shallowest line is 6 spaces: three 2-space levels.
  assert_eq!(
    indentation_seen(html, Some(2), None),
    vec![Some(EmbeddedIndentation {
      base_level: 3,
      check_first_line: true,
    })]
  );
  // With `baseIndentLevel: 1`, one level past the `<style>` tag's two.
  assert_eq!(
    indentation_seen(html, Some(2), Some(1))[0]
      .unwrap()
      .base_level,
    3
  );
  assert_eq!(
    indentation_seen(html, Some(2), Some(0))[0]
      .unwrap()
      .base_level,
    2
  );

  // Unindented Vue-style blocks sit at level 0.
  let vue = "<template>\n  <div />\n</template>\n\n<style>\n.a {\n  color: red;\n}\n</style>\n";
  assert_eq!(
    indentation_seen(vue, Some(2), None)[0].unwrap().base_level,
    0
  );

  // A sheet on the tag's own line does not check (or count) that line.
  let inline = indentation_seen("<style>  a {}\n  b {}</style>", Some(2), None);
  assert_eq!(
    inline,
    vec![Some(EmbeddedIndentation {
      base_level: 1,
      check_first_line: false,
    })]
  );

  // Context is supplied per invocation, not installed as ambient state.
}

#[test]
fn doc_indent_size_votes_on_indent_steps() {
  assert_eq!(infer_doc_indent_size("a\n  b\n    c\n  d\n"), Some(2));
  assert_eq!(infer_doc_indent_size("a\n    b\n        c\n"), Some(4));
  assert_eq!(infer_doc_indent_size("a\nb\n"), None);
  assert_eq!(infer_doc_indent_size(" a\n"), Some(1));
  assert_eq!(infer_doc_indent_size("\ta\n\t\tb\n"), None);
}
