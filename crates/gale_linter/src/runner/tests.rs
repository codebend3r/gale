use gale_css_parser::postcss_tree::PostcssTree;

use super::*;
use crate::panic_guard::ISSUES_URL;

use crate::options::InvalidOption;
use crate::registry::RuleRegistry;

/// Implement [`Rule`] for `$rule`, a unit struct that is also its own
/// configured form: a test rule that takes no options and warns.
macro_rules! test_rule {
  ($rule:ident, $name:literal, $description:literal) => {
    impl Rule for $rule {
      fn meta(&self) -> RuleMeta {
        RuleMeta {
          name: $name,
          description: $description,
          default_severity: Severity::Warning,
          primary_option_array: false,
          primary_option_object: false,
        }
      }

      fn configure(
        &self,
        _setting: &Setting,
        _env: &LintEnv,
      ) -> Result<Box<dyn Check + '_>, Vec<InvalidOption>> {
        Ok(Box::new($rule))
      }
    }
  };
}

/// The spec of a set running `rules` with no options.
fn spec_for(rules: &[&str]) -> RuleSetSpec {
  RuleSetSpec {
    rules: rules.iter().map(|r| RuleSetting::new(*r)).collect(),
    ..RuleSetSpec::default()
  }
}

/// A set running `rules` with no options.
fn set_for(rules: &[&str]) -> RuleSet {
  RuleSet::of_rules(rules.iter().copied())
}

/// A set running just `rule`, configured with `options`.
fn set_with_options(rule: &str, options: serde_json::Value) -> RuleSet {
  RuleSet::new(RuleSetSpec {
    rules: vec![RuleSetting::new(rule).with_options(options)],
    ..RuleSetSpec::default()
  })
}

/// A set running `rules` with rules from `registry`, which is leaked so
/// the set can borrow its rules as it borrows the built-in ones.
fn set_in(registry: RuleRegistry, spec: RuleSetSpec) -> RuleSet {
  RuleSet::with_registry(Box::leak(Box::new(registry)), spec)
}

#[test]
fn lint_empty_block() {
  let set = set_for(&["block-no-empty"]);
  let result = set.lint("a { }", "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  assert_eq!(result.diagnostics[0].rule_name, "block-no-empty");
  assert_eq!(result.diagnostics[0].message, "Unexpected empty block");
}

#[test]
fn lint_non_empty_block() {
  let set = set_for(&["block-no-empty"]);
  let result = set.lint("a { color: red; }", "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty());
}

#[test]
fn disabled_rule_not_run() {
  let set = set_for(&[]);
  let result = set.lint("a { }", "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty());
}

// -- Invalid pattern options --

#[test]
fn a_lookahead_pattern_works() {
  let set = set_with_options(
    "selector-class-pattern",
    serde_json::json!("^(?!js-)[a-z-]+$"),
  );
  let result = set.lint(".card {}\n.js-card {}\n", "test.css", Syntax::Css);
  assert!(result.invalid_option_warnings.is_empty());
  assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
  assert!(result.diagnostics[0].message.contains("js-card"));
}

#[test]
fn a_pattern_that_does_not_compile_is_an_invalid_option_once_per_file() {
  let set = set_with_options("selector-class-pattern", serde_json::json!("^[a-z"));
  let result = set.lint(".a {}\n.b {}\n", "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
  assert_eq!(result.invalid_option_warnings.len(), 1);
  assert!(
    result.invalid_option_warnings[0]
      .starts_with("Invalid option value \"^[a-z\" for rule \"selector-class-pattern\": "),
    "{:?}",
    result.invalid_option_warnings
  );
  assert!(result.errored());
}

#[test]
fn a_broken_list_entry_is_an_invalid_option() {
  let set = set_with_options(
    "color-named",
    serde_json::json!(["never", { "ignoreProperties": ["/[broken/"] }]),
  );
  let result = set.lint("a { color: red; }\n", "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1, "the rule still runs");
  assert_eq!(
    result.invalid_option_warnings.len(),
    1,
    "{:?}",
    result.invalid_option_warnings
  );
  assert!(result.invalid_option_warnings[0].contains("\"/[broken/\""));
}

// -- Rules configured once per set --

thread_local! {
  /// How often [`FlagsRules`] has been configured on this test's thread.
  static FLAGS_CONFIGURED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A rule with typed options: it flags every style rule, with the word
/// its primary option names (`"a"` or `"b"`).
struct FlagsRules {
  word: &'static str,
}

impl FlagsRules {
  fn from_setting(
    setting: &Setting,
    _env: &LintEnv,
  ) -> Result<Self, Vec<crate::options::InvalidOption>> {
    FLAGS_CONFIGURED.set(FLAGS_CONFIGURED.get() + 1);
    let mut options = setting.options();
    let word = options.primary_one_of(&["a", "b"]).unwrap_or("a");
    options.flag("ignoreNothing");
    options.finish()?;
    Ok(Self { word })
  }
}

impl Check for FlagsRules {
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_rules()
      .map(|rule| Diagnostic::new("test/flags", self.word).span(Span::new(rule.start, 1)))
      .collect()
  }
}

crate::rule::declare_rule! {
  /// Flags every style rule.
  struct Flags => FlagsRules {
    name: "test/flags",
    description: "Flags every style rule",
    default_severity: Error,
  }
}

/// A set running [`Flags`] as `setting` says, with `default_severity`.
fn flags_set(setting: RuleSetting, default_severity: Option<Severity>) -> RuleSet {
  let mut registry = RuleRegistry::default();
  registry.register(Box::new(Flags));
  set_in(
    registry,
    RuleSetSpec {
      rules: vec![setting],
      default_severity,
      ..RuleSetSpec::default()
    },
  )
}

#[test]
fn a_rule_with_typed_options_is_configured_once_per_set() {
  let before = FLAGS_CONFIGURED.get();
  let set = flags_set(
    RuleSetting::new("test/flags").with_options(serde_json::json!("b")),
    None,
  );
  for _ in 0..3 {
    let result = set.lint("a {}\nb {}\n", "test.css", Syntax::Css);
    let words: Vec<&str> = result
      .diagnostics
      .iter()
      .map(|d| d.message.as_str())
      .collect();
    assert_eq!(words, ["b", "b"]);
  }
  assert_eq!(FLAGS_CONFIGURED.get() - before, 1);
  assert!(set.invalid_options().is_empty());
}

#[test]
fn the_runner_gives_problems_their_severity() {
  let severity_of = |setting: RuleSetting, default: Option<Severity>| {
    let result = flags_set(setting, default).lint("a {}\n", "test.css", Syntax::Css);
    result.diagnostics[0].severity
  };
  // The rule leaves its problems at `Diagnostic::new`'s warning; with no
  // severity configured it reports at its own default.
  assert_eq!(
    severity_of(RuleSetting::new("test/flags"), None),
    Severity::Error
  );
  assert_eq!(
    severity_of(RuleSetting::new("test/flags"), Some(Severity::Warning)),
    Severity::Warning,
    "defaultSeverity comes before the rule's default"
  );
  assert_eq!(
    severity_of(
      RuleSetting::new("test/flags").with_severity(Severity::Warning),
      Some(Severity::Error)
    ),
    Severity::Warning,
    "the rule's own severity comes first"
  );
}

#[test]
fn invalid_options_stop_the_rule_and_are_reported_on_every_file() {
  let set = flags_set(
    RuleSetting::new("test/flags").with_options(serde_json::json!(["c", { "ignoreAll": true }])),
    None,
  );
  let expected = [
    "Invalid option value \"c\" for rule \"test/flags\"",
    "Invalid option name \"ignoreAll\" for rule \"test/flags\"",
  ];
  assert_eq!(set.invalid_options(), expected);
  for (source, path) in [
    ("a {}\n", "a.css"),
    ("", "empty.css"),
    ("<template></template>\n", "nothing.vue"),
    ("<style>a {}</style>\n", "styled.vue"),
  ] {
    let result = set.lint(source, path, Syntax::Css);
    assert!(
      result.diagnostics.is_empty(),
      "{path}: {:?}",
      result.diagnostics
    );
    assert_eq!(result.invalid_option_warnings, expected, "{path}");
    assert!(result.errored(), "{path}");
  }
}

#[test]
fn every_rule_gets_the_common_secondary_options() {
  let set = flags_set(
    RuleSetting::new("test/flags").with_options(serde_json::json!([
      "a",
      { "message": "custom", "url": "https://example.test", "ignoreNothing": true }
    ])),
    None,
  );
  let result = set.lint("a {}\n", "test.css", Syntax::Css);
  assert!(result.invalid_option_warnings.is_empty());
  assert_eq!(result.diagnostics[0].message, "custom");
  assert_eq!(
    result.diagnostics[0].url.as_deref(),
    Some("https://example.test")
  );
}

// -- Unknown rule names --

#[test]
fn an_unknown_rule_is_reported_like_stylelint() {
  let set = set_for(&["block-no-emty", "block-no-empty"]);
  let result = set.lint("a {}\n", "test.css", Syntax::Css);
  let unknown: Vec<&Diagnostic> = result
    .diagnostics
    .iter()
    .filter(|d| d.rule_name == "block-no-emty")
    .collect();
  assert_eq!(unknown.len(), 1);
  let d = unknown[0];
  assert_eq!(
    d.message,
    "Unknown rule block-no-emty. Did you mean block-no-empty?"
  );
  assert_eq!(d.severity, Severity::Error);
  assert_eq!(d.span, Span::new(0, 1));
  assert!(
    result
      .diagnostics
      .iter()
      .any(|d| d.rule_name == "block-no-empty"),
    "the real rule still runs"
  );
}

#[test]
fn rules_gale_lacks_are_not_unknown() {
  // A Stylelint core rule gale has not implemented, and a plugin rule.
  let set = set_for(&["no-unknown-custom-properties", "acme/no-foo"]);
  let result = set.lint("a {}\n", "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn unknown_rules_ignore_disables_and_default_severity() {
  let set = RuleSet::new(RuleSetSpec {
    default_severity: Some(Severity::Warning),
    ..spec_for(&["not-a-rule"])
  });
  let result = set.lint("/* stylelint-disable */\na {}\n", "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
  assert_eq!(result.diagnostics[0].message, "Unknown rule not-a-rule.");
  assert_eq!(result.diagnostics[0].severity, Severity::Error);
}

// -- Panic isolation --

/// A rule that panics on any style rule, the way an out-of-bounds slice
/// would.
struct PanickingRule;

test_rule!(PanickingRule, "test/panics", "Panics on every style rule");

impl Check for PanickingRule {
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    for rule in tree.walk_rules() {
      let _ = &rule.selector()[..rule.selector().len() + 1];
    }
    vec![]
  }
}

/// A registry with the built-in rules plus [`PanickingRule`].
fn registry_with_panicking_rule() -> RuleRegistry {
  let mut registry = RuleRegistry::default();
  registry.register(Box::new(PanickingRule));
  registry
}

#[test]
fn a_panicking_rule_is_reported_once_and_other_rules_still_run() {
  let set = set_in(
    registry_with_panicking_rule(),
    spec_for(&["test/panics", "block-no-empty"]),
  );
  let result = set.lint("a {}\nb {}\n", "test.css", Syntax::Css);

  let crashes: Vec<&Diagnostic> = result
    .diagnostics
    .iter()
    .filter(|d| d.rule_name == "test/panics")
    .collect();
  assert_eq!(crashes.len(), 1, "{:?}", result.diagnostics);
  let crash = crashes[0];
  assert_eq!(crash.severity, Severity::Error);
  assert!(
    crash
      .message
      .starts_with("Internal error in rule \"test/panics\": "),
    "{}",
    crash.message
  );
  assert!(crash.message.contains("out of bounds"), "{}", crash.message);
  assert!(crash.message.contains(ISSUES_URL), "{}", crash.message);
  assert_eq!(crash.span.offset, 0, "reported at the start of the file");

  let empty_blocks = result
    .diagnostics
    .iter()
    .filter(|d| d.rule_name == "block-no-empty")
    .count();
  assert_eq!(empty_blocks, 2, "the other rule still reports everything");
}

#[test]
fn a_rule_crash_ignores_disables_and_severity_overrides() {
  let set = set_in(
    registry_with_panicking_rule(),
    RuleSetSpec {
      rules: vec![
        RuleSetting::new("test/panics")
          .with_options(serde_json::json!([true, { "message": "custom" }]))
          .with_severity(Severity::Warning),
      ],
      ..RuleSetSpec::default()
    },
  );
  let src = "/* stylelint-disable */\na { color: red; }\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
  assert_eq!(result.diagnostics[0].severity, Severity::Error);
  assert!(result.diagnostics[0].message.starts_with("Internal error"));
}

#[test]
fn host_files_are_linted_per_block_with_the_blocks_syntax() {
  let set = RuleSet::new(RuleSetSpec {
    rules: vec![
      RuleSetting::new("scss/dollar-variable-pattern").with_options(serde_json::json!("^[a-z]+$")),
    ],
    ..RuleSetSpec::default()
  });
  let source = "<template></template>\n<style lang=\"scss\">$Bad: 1;</style>\n";
  let result = set.lint(source, "a.vue", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
  assert_eq!(
    result.diagnostics[0].span.offset,
    source.find("$Bad").unwrap()
  );
  // The same text as a style sheet of its own is CSS, not SCSS.
  let as_css = set.lint("$Bad: 1;", "a.css", Syntax::Css);
  assert!(as_css.diagnostics.is_empty(), "{:?}", as_css.diagnostics);
}

#[test]
fn embedded_style_sheets_are_guarded_too() {
  let set = set_in(
    registry_with_panicking_rule(),
    spec_for(&["test/panics", "color-named"]),
  );
  let result = set.lint(
    "<template></template>\n<style>a { color: red; }</style>\n",
    "test.vue",
    Syntax::Css,
  );
  let rules: Vec<&str> = result
    .diagnostics
    .iter()
    .map(|d| d.rule_name.as_str())
    .collect();
  assert!(rules.contains(&"test/panics"), "{rules:?}");
  assert!(rules.contains(&"color-named"), "{rules:?}");
}

// -- Configured names, options and severities --

#[test]
fn problems_are_reported_under_the_name_the_config_used() {
  // `indentation` is the deprecated name of `@stylistic/indentation`.
  for name in ["indentation", "@stylistic/indentation"] {
    let set = RuleSet::new(RuleSetSpec {
      rules: vec![
        RuleSetting::new(name)
          .with_options(serde_json::json!([2, { "message": "indent it" }]))
          .with_severity(Severity::Warning),
      ],
      ..RuleSetSpec::default()
    });
    let result = set.lint("a {\n    top: 0;\n}\n", "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, name);
    assert_eq!(d.message, "indent it");
    assert_eq!(d.severity, Severity::Warning);
  }
}

#[test]
fn a_rule_named_twice_runs_once_under_its_own_name() {
  let set = set_for(&["indentation", "@stylistic/indentation"]);
  let result = set.lint("a {\n   top: 0;\n}\n", "test.css", Syntax::Css);
  let names: Vec<&str> = result
    .diagnostics
    .iter()
    .map(|d| d.rule_name.as_str())
    .collect();
  assert_eq!(names, ["@stylistic/indentation"]);
}

#[test]
fn the_default_severity_applies_to_rules_without_their_own() {
  let set = RuleSet::new(RuleSetSpec {
    rules: vec![
      RuleSetting::new("block-no-empty"),
      RuleSetting::new("color-named")
        .with_options(serde_json::json!("never"))
        .with_severity(Severity::Error),
    ],
    default_severity: Some(Severity::Warning),
    ..RuleSetSpec::default()
  });
  let result = set.lint("a {}\nb { color: red; }\n", "test.css", Syntax::Css);
  let severities: Vec<(&str, Severity)> = result
    .diagnostics
    .iter()
    .map(|d| (d.rule_name.as_str(), d.severity))
    .collect();
  assert_eq!(
    severities,
    [
      ("block-no-empty", Severity::Warning),
      ("color-named", Severity::Error)
    ]
  );
}

#[test]
fn rules_see_the_environment_of_their_set() {
  let src = "a { height: 100dvh; }";
  let modern = set_for(&["unit-no-unknown"]);
  assert!(
    modern
      .lint(src, "test.css", Syntax::Css)
      .diagnostics
      .is_empty()
  );

  let legacy = RuleSet::new(RuleSetSpec {
    env: LintEnv {
      stylelint_version: Some((14, 16, 1)),
    },
    ..spec_for(&["unit-no-unknown"])
  });
  let result = legacy.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
  assert_eq!(
    result.diagnostics[0].message,
    "Unexpected unknown unit \"dvh\""
  );
}

// -- Panics and the artifacts rules share --

/// A rule that reports every style rule in the shared statement tree, at
/// its start.
struct ReportsTreeRules;

test_rule!(
  ReportsTreeRules,
  "test/tree-rules",
  "Reports every style rule in the shared statement tree"
);

impl Check for ReportsTreeRules {
  /// Reports each style rule of the tree it is handed, which must be the
  /// tree of the context's text.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    assert!(
      std::ptr::eq(tree.source(), ctx.source),
      "the tree of the text"
    );
    tree
      .walk_rules()
      .map(|rule| Diagnostic::new("test/tree-rules", "rule").span(Span::new(rule.start, 1)))
      .collect()
  }
}

/// A rule that panics while it holds the shared tree.
struct PanicsHoldingTree;

test_rule!(
  PanicsHoldingTree,
  "test/panics-holding-tree",
  "Panics while holding the shared statement tree"
);

impl Check for PanicsHoldingTree {
  /// Panics holding the shared tree.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    panic!("crashed holding {} statements", tree.nodes.len());
  }
}

struct MustNotRunEmbedded;
test_rule!(
  MustNotRunEmbedded,
  "@stylistic/unicode-bom",
  "Must not execute in embedded roots"
);

impl Check for MustNotRunEmbedded {
  fn check(&self, _tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    panic!("excluded rule executed");
  }
}

#[test]
fn embedded_exclusions_are_applied_before_rule_execution() {
  let mut registry = RuleRegistry::new();
  registry.register(Box::new(MustNotRunEmbedded));
  let set = set_in(registry, spec_for(&["@stylistic/unicode-bom"]));
  assert!(
    set
      .lint("<style>a{}</style>", "x.vue", Syntax::Css)
      .diagnostics
      .is_empty()
  );
  assert_eq!(set.lint("a{}", "x.css", Syntax::Css).diagnostics.len(), 1);
}

/// A set running `rules`, in that order, with the test rules above
/// registered.
fn set_with_tree_rules(rules: &[&str]) -> RuleSet {
  let mut registry = RuleRegistry::default();
  registry.register(Box::new(ReportsTreeRules));
  registry.register(Box::new(PanicsHoldingTree));
  set_in(registry, spec_for(rules))
}

/// The messages of `rule`'s problems, by offset.
fn problems_of<'r>(result: &'r LintResult, rule: &str) -> Vec<(usize, &'r str)> {
  result
    .diagnostics
    .iter()
    .filter(|d| d.rule_name == rule)
    .map(|d| (d.span.offset, d.message.as_str()))
    .collect()
}

/// A rule that panics while holding the shared tree only drops its
/// reference; the other rules still get the tree.
#[test]
fn a_rule_that_panics_holding_the_shared_tree_leaves_it_to_the_others() {
  let set = set_with_tree_rules(&["test/panics-holding-tree", "test/tree-rules"]);
  let result = set.lint(
    "a {}
b {}
",
    "test.css",
    Syntax::Css,
  );

  let crashes = problems_of(&result, "test/panics-holding-tree");
  assert_eq!(crashes.len(), 1, "{:?}", result.diagnostics);
  assert!(
    crashes[0].1.contains("crashed holding 2 statements"),
    "{}",
    crashes[0].1
  );
  assert_eq!(
    problems_of(&result, "test/tree-rules"),
    vec![(0, "rule"), (5, "rule")]
  );
}

#[test]
fn a_panic_outside_any_rule_becomes_one_problem_on_the_file() {
  let result = guard_file("a {}", "test.css", || panic!("parser exploded"));
  assert_eq!(result.file_path, "test.css");
  assert_eq!(result.diagnostics.len(), 1);
  let d = &result.diagnostics[0];
  assert_eq!(d.rule_name, INTERNAL_ERROR_RULE);
  assert_eq!(d.severity, Severity::Error);
  assert!(d.message.contains("parser exploded"), "{}", d.message);
}

// -- Inline disable comment tests --

#[test]
fn embedded_disable_reports_share_document_scope() {
  let set = RuleSet::new(RuleSetSpec {
    rules: vec![
      RuleSetting::new("block-no-empty")
        .with_options(serde_json::json!([true, {"reportDisables":true}])),
    ],
    disables: DisableOptions {
      report_descriptionless: true,
      ..Default::default()
    },
    ..Default::default()
  });
  let source = "<style>/* stylelint-disable block-no-empty */</style>\n<style>a{} /* stylelint-enable block-no-empty */</style>";
  let result = set.lint(source, "x.vue", Syntax::Css);
  assert_eq!(
    result
      .diagnostics
      .iter()
      .filter(|d| d.rule_name == "reportDisables")
      .count(),
    1
  );
  assert_eq!(
    result
      .diagnostics
      .iter()
      .filter(|d| d.rule_name == "--report-descriptionless-disables")
      .count(),
    1
  );
  assert!(
    !result
      .diagnostics
      .iter()
      .any(|d| d.rule_name == "CssSyntaxError" || d.rule_name == "block-no-empty")
  );
}

#[test]
fn embedded_custom_messages_are_not_parsed_as_source_locations() {
  let message = "first used at line 123";
  let set = set_with_options(
    "no-duplicate-selectors",
    serde_json::json!([true, {"message":message}]),
  );
  let result = set.lint(
    "<template/>\n<style>\n.a{}\n.a{}\n</style>",
    "x.vue",
    Syntax::Css,
  );
  assert_eq!(result.diagnostics[0].message, message);
  assert_eq!(result.diagnostics[0].related.len(), 1);
}

#[test]
fn gale_disable_all() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* gale-disable */\na { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty());
}

#[test]
fn gale_disable_specific_rule() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* gale-disable block-no-empty */\na { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty());
}

#[test]
fn gale_disable_wrong_rule_still_reports() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* gale-disable some-other-rule */\na { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn gale_disable_enable_block() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* gale-disable */\na { }\n/* gale-enable */\nb { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  // First `a { }` is disabled, second `b { }` should be reported.
  assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn gale_disable_next_line() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* gale-disable-next-line */\na { }\nb { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  // Only `a { }` is disabled; `b { }` still reported.
  assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn gale_disable_next_line_specific_rule() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* gale-disable-next-line block-no-empty */\na { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty());
}

#[test]
fn stylelint_disable_compat() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* stylelint-disable */\na { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty());
}

#[test]
fn stylelint_disable_next_line_compat() {
  let set = set_for(&["block-no-empty"]);
  let src = "/* stylelint-disable-next-line */\na { }\nb { }";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
}

/// The lines `set` reports problems on in `src`.
fn problem_lines(set: &RuleSet, src: &str, path: &str, syntax: Syntax) -> Vec<usize> {
  let result = set.lint(src, path, syntax);
  let index = SourceLineIndex::build(src);
  result
    .diagnostics
    .iter()
    .map(|d| index.line(d.span.offset))
    .collect()
}

#[test]
fn disables_cover_whole_lines_like_stylelint() {
  let set = set_for(&["block-no-empty"]);
  let lines = |src: &str| problem_lines(&set, src, "test.css", Syntax::Css);
  // A disable covers its own line, code before it included, and the
  // enable's line is still off.
  assert_eq!(
    lines("a {} /* stylelint-disable */\nb {}\n/* stylelint-enable */ c {}\nd {}\n"),
    vec![4]
  );
  // `disable-next-line` reaches the line after the comment ends.
  assert_eq!(
    lines("/* stylelint-disable-next-line\n  block-no-empty */\na {}\nb {}\n"),
    vec![4]
  );
  // A command inside a selector or value counts; one inside a string
  // does not.
  assert_eq!(
    lines(
      "a, /* stylelint-disable-line */ b {}\nc { content: \"/* stylelint-disable */\" }\nd {}\n"
    ),
    vec![3]
  );
}

#[test]
fn a_plain_enable_turns_rules_disabled_by_name_back_on() {
  let set = set_for(&["color-no-invalid-hex"]);
  let src = "/* stylelint-disable color-no-invalid-hex */\na { color: #ab; }\n/* stylelint-enable */\na { color: #ab; }\n";
  assert_eq!(problem_lines(&set, src, "test.css", Syntax::Css), vec![4]);
  // And enabling one rule under a blanket disable turns on just that one.
  let set = set_for(&["color-no-invalid-hex", "block-no-empty"]);
  let src = "/* stylelint-disable */\na {}\n/* stylelint-enable color-no-invalid-hex */\nb { color: #ab; }\nc {}\n";
  assert_eq!(problem_lines(&set, src, "test.css", Syntax::Css), vec![4]);
}

#[test]
fn a_rejected_command_replaces_every_problem_like_stylelint() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      ignore: true,
      ..DisableOptions::default()
    },
    default_severity: Some(Severity::Warning),
    ..spec_for(&["block-no-empty", "not-a-rule"])
  });
  let src = "a {}\n/* stylelint-enable */\nb {}\n";
  for result in [
    set.lint(src, "test.css", Syntax::Css),
    set_for(&["block-no-empty"]).lint(src, "test.css", Syntax::Css),
  ] {
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "CssSyntaxError");
    assert_eq!(d.message, "No rules have been disabled");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(&src[d.span.offset..d.span.end()], "/* stylelint-enable */");
    assert_eq!(
      d.stylelint_text(),
      "No rules have been disabled (CssSyntaxError)"
    );
  }
}

#[test]
fn double_slash_commands_need_scss_or_less() {
  let set = set_for(&["block-no-empty"]);
  let src = "// stylelint-disable-next-line block-no-empty\na {}\nb {}\n";
  assert_eq!(problem_lines(&set, src, "t.scss", Syntax::Scss), vec![3]);
  assert_eq!(problem_lines(&set, src, "t.less", Syntax::Less), vec![3]);
  // In CSS the line is part of the next selector, not a comment.
  let hex = set_for(&["color-no-invalid-hex"]);
  let css = "a { color: #ab; } // stylelint-disable-line color-no-invalid-hex\nb {}\n";
  assert_eq!(problem_lines(&hex, css, "t.css", Syntax::Css), vec![1]);
  assert!(problem_lines(&hex, css, "t.scss", Syntax::Scss).is_empty());
  // A description on the next `//` line makes the command span both.
  let described = "// stylelint-disable-next-line block-no-empty\n// -- legacy\na {}\nb {}\n";
  assert_eq!(
    problem_lines(&set, described, "t.scss", Syntax::Scss),
    vec![4]
  );
}

#[test]
fn sass_disables_count_the_lines_of_the_sass() {
  // The converted SCSS gains a `}` line before `color`, which must not
  // push the problem off the line the command names.
  let set = set_with_options("color-hex-case", serde_json::json!("lower"));
  let src =
    ".a\n  .b\n    // stylelint-disable-next-line color-hex-case\n  color: #FFF\n  top: #FFF\n";
  assert_eq!(problem_lines(&set, src, "test.sass", Syntax::Sass), vec![5]);
}

#[test]
fn sass_is_linted_as_the_scss_it_converts_to() {
  let set = set_for(&["color-named"]);
  let result = set.lint(".foo\n  color: red", "test.sass", Syntax::Sass);
  assert_eq!(result.file_path, "test.sass");
  assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
  assert_eq!(result.diagnostics[0].span.offset, 14);
}

#[test]
fn sass_spans_point_into_the_original_file() {
  // Multibyte text before the problem used to push spans (computed on the
  // converted SCSS) into the middle of characters of the Sass source.
  let src =
    "// héllo wörld ünïcödé\n.foo\n  content: \"→ ✓ ★\"\n  color: red\n\n.baz\n  color: #FFF\n";
  let set = set_with_options("color-hex-case", serde_json::json!("lower"));
  let result = set.lint(src, "test.sass", Syntax::Sass);

  assert_eq!(result.source, src, "results carry the Sass, not the SCSS");
  assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
  let d = &result.diagnostics[0];
  assert_eq!(d.rule_name, "color-hex-case");
  assert_eq!(d.span.offset, src.find("#FFF").unwrap());
  assert_eq!(&src[d.span.offset..d.span.end()], "#FFF");
  assert!(
    d.fix.is_none(),
    "fixes target the SCSS, so they are dropped"
  );
}

#[test]
fn sass_with_multibyte_text_lints_with_every_rule() {
  let src = "// héllo wörld ünïcödé\n.foo\n  content: \"→ ✓ ★\"\n  color: red\n  .bar\n    margin: 0 0 0 0\n    font-family: Ärial\n\n.baz\n  color: #FFF\n";
  let set = RuleSet::of_rules(RuleRegistry::global().all().iter().map(|r| r.name()));
  let result = set.lint(src, "test.sass", Syntax::Sass);
  for d in &result.diagnostics {
    assert!(!d.message.starts_with("Internal error"), "{}", d.message);
    assert!(src.is_char_boundary(d.span.offset), "{d:?}");
    assert!(src.is_char_boundary(d.span.end()), "{d:?}");
  }
}

#[test]
fn malformed_scss_is_linted_as_far_as_it_parses() {
  // The tree recovers from the unclosed string the way
  // `postcss-safe-parser` does, so the rules still see the file.
  let set = set_for(&["block-no-empty"]);
  let result = set.lint(
    "a {}\n.foo { content: \"unclosed; }",
    "test.scss",
    Syntax::Scss,
  );
  assert_eq!(result.file_path, "test.scss");
  let rules: Vec<&str> = result
    .diagnostics
    .iter()
    .map(|d| d.rule_name.as_str())
    .collect();
  assert_eq!(rules, ["block-no-empty"]);
}

// -- Secondary options every rule accepts --

#[test]
fn message_option_replaces_the_warning_text() {
  let set = set_with_options(
    "block-no-empty",
    serde_json::json!([true, { "message": "No empty blocks please" }]),
  );
  let result = set.lint("a {}", "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  assert_eq!(result.diagnostics[0].message, "No empty blocks please");
  assert_eq!(result.diagnostics[0].rule_name, "block-no-empty");
}

#[test]
fn url_option_is_attached_to_every_warning_of_the_rule() {
  let set = set_with_options(
    "block-no-empty",
    serde_json::json!([true, { "url": "https://example.com/empty" }]),
  );
  let result = set.lint("a {}\nb {}", "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 2);
  for d in &result.diagnostics {
    assert_eq!(d.url.as_deref(), Some("https://example.com/empty"));
  }
}

#[test]
fn disable_fix_option_keeps_the_report_but_drops_the_fix() {
  let fixable = set_with_options("color-hex-case", serde_json::json!(["lower"]));
  let with_fix = fixable.lint("a { color: #FFF; }", "test.css", Syntax::Css);
  assert!(
    with_fix.diagnostics[0].fix.is_some(),
    "precondition: rule is fixable"
  );

  let set = set_with_options(
    "color-hex-case",
    serde_json::json!(["lower", { "disableFix": true }]),
  );
  let result = set.lint("a { color: #FFF; }", "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  assert!(result.diagnostics[0].fix.is_none());
}

#[test]
fn secondary_options_apply_in_embedded_style_sheets() {
  let set = set_with_options(
    "block-no-empty",
    serde_json::json!([true, { "message": "Custom", "url": "https://x.y" }]),
  );
  let result = set.lint("<style>a {}</style>", "test.html", Syntax::Css);
  assert_eq!(result.diagnostics[0].message, "Custom");
  assert_eq!(result.diagnostics[0].url.as_deref(), Some("https://x.y"));
}

// -- Disable-comment reports --

#[test]
fn invalid_scope_disable_is_reported_for_unconfigured_rule() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_invalid_scope: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable color-named */\na { color: red; }\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  let d = &result.diagnostics[0];
  assert_eq!(d.rule_name, "--report-invalid-scope-disables");
  assert_eq!(d.message, "Rule \"color-named\" isn't enabled");
  assert_eq!(d.severity, Severity::Error);
  // From the comment's first character to its last, as Stylelint's
  // warning start and end.
  assert_eq!(d.span, Span::from_range(0, 34));
}

#[test]
fn disable_reports_point_at_the_declaration_holding_the_comment() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_invalid_scope: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  let src = "a {\n  color: /* stylelint-disable-line color-named */ red;\n}\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  let span = result.diagnostics[0].span;
  assert_eq!(
    &src[span.offset..=span.end()],
    "color: /* stylelint-disable-line color-named */ red;"
  );
}

#[test]
fn invalid_scope_ignores_configured_and_blanket_disables() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_invalid_scope: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  for src in [
    "/* stylelint-disable block-no-empty */\na { color: red; }\n",
    "/* stylelint-disable */\na { color: red; }\n",
  ] {
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty(), "{src}");
  }
}

#[test]
fn invalid_scope_counts_plugin_rules_from_the_config() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_invalid_scope: true,
      ..DisableOptions::default()
    },
    configured_rules: vec!["some-plugin/rule".to_string()],
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable some-plugin/rule */\na { color: red; }\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert!(result.diagnostics.is_empty());
}

#[test]
fn invalid_scope_reports_once_per_comment_and_rule() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_invalid_scope: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  // An inline disable creates two ranges for the same comment.
  let src = "a { color: red; } /* stylelint-disable color-named */\nb { color: red; }\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn descriptionless_disable_is_reported_once_per_comment() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_descriptionless: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable block-no-empty, color-named */\na {}\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  let d = &result.diagnostics[0];
  assert_eq!(d.rule_name, "--report-descriptionless-disables");
  assert_eq!(
    d.message,
    "Disable for \"block-no-empty\" is missing a description"
  );
  assert_eq!(d.span.offset, 0);
}

#[test]
fn descriptionless_blanket_disable_is_named_all() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_descriptionless: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable */\na {}\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  assert_eq!(
    result.diagnostics[0].message,
    "Disable for \"all\" is missing a description"
  );
}

#[test]
fn described_disables_are_not_reported() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_descriptionless: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  for src in [
    "/* stylelint-disable block-no-empty -- legacy markup */\na {}\n",
    "/* stylelint-disable -- legacy markup */\na {}\n",
    "/* stylelint-disable-next-line block-no-empty -- why */\na {}\n",
  ] {
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty(), "{src}");
  }
}

#[test]
fn descriptionless_next_line_disable_is_reported() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_descriptionless: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable-next-line block-no-empty */\na {}\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  assert_eq!(
    result.diagnostics[0].rule_name,
    "--report-descriptionless-disables"
  );
}

#[test]
fn unscoped_disable_is_reported_once_per_comment() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_unscoped: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  // Inline placement creates two ranges for one comment.
  let src = "a { color: red; } /* stylelint-disable */\nb {}\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics.len(), 1);
  let d = &result.diagnostics[0];
  assert_eq!(d.rule_name, "--report-unscoped-disables");
  assert_eq!(d.message, "Configuration comment must be scoped");
  assert_eq!(d.severity, Severity::Error);
  assert_eq!(d.span.offset, 18);
}

#[test]
fn scoped_disables_are_not_unscoped() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_unscoped: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  for src in [
    "/* stylelint-disable block-no-empty */\na {}\n",
    "/* stylelint-disable-next-line block-no-empty */\na {}\n",
  ] {
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty(), "{src}");
  }
}

#[test]
fn report_disables_option_flags_a_disable_of_that_rule() {
  let set = set_with_options(
    "block-no-empty",
    serde_json::json!([true, { "reportDisables": true }]),
  );
  for src in [
    "/* stylelint-disable block-no-empty */\na {}\n",
    "/* stylelint-disable-next-line block-no-empty */\na {}\n",
  ] {
    let result = set.lint(src, "test.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1, "{src}");
    let d = &result.diagnostics[0];
    assert_eq!(d.rule_name, "reportDisables");
    assert_eq!(d.message, "Rule \"block-no-empty\" may not be disabled");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.span.offset, 0);
  }
}

#[test]
fn report_disables_ignores_other_rules_and_blanket_disables() {
  let set = set_with_options(
    "block-no-empty",
    serde_json::json!([true, { "reportDisables": true }]),
  );
  for src in [
    "/* stylelint-disable color-named */\na { color: red; }\n",
    "/* stylelint-disable */\na {}\n",
  ] {
    let result = set.lint(src, "test.css", Syntax::Css);
    assert!(result.diagnostics.is_empty(), "{src}");
  }
}

#[test]
fn disable_reports_use_the_default_severity() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_descriptionless: true,
      ..DisableOptions::default()
    },
    default_severity: Some(Severity::Warning),
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable block-no-empty */\na {}\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  assert_eq!(result.diagnostics[0].severity, Severity::Warning);
}

#[test]
fn reports_still_run_under_ignore_disables() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      ignore: true,
      report_descriptionless: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable block-no-empty */\na {}\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  let rules: Vec<&str> = result
    .diagnostics
    .iter()
    .map(|d| d.rule_name.as_str())
    .collect();
  assert_eq!(
    rules,
    vec!["--report-descriptionless-disables", "block-no-empty"]
  );
}

/// Only a rule that says it never reports has needless disables: a comment
/// disabling `material/no-prefixes` disables nothing, while one disabling a
/// rule gale runs might be needed for problems gale does not report.
#[test]
fn needless_disables_are_reported_for_rules_that_never_report() {
  let set = RuleSet::new(RuleSetSpec {
    disables: DisableOptions {
      report_needless: true,
      ..DisableOptions::default()
    },
    ..spec_for(&["block-no-empty"])
  });
  let src = "/* stylelint-disable material/no-prefixes */\n/* stylelint-disable color-named */\na { color: red; }\n";
  let result = set.lint(src, "test.css", Syntax::Css);
  let needless: Vec<&str> = result
    .diagnostics
    .iter()
    .filter(|d| d.rule_name == gale_diagnostics::NEEDLESS_DISABLES)
    .map(|d| d.message.as_str())
    .collect();
  assert_eq!(needless, ["Needless disable for \"material/no-prefixes\""]);
}
