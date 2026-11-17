use super::*;
use serde_json::json;

/// A rule whose primary option is a plain value.
const RULE: RuleMeta = RuleMeta {
  name: "test-rule",
  description: "",
  default_severity: gale_diagnostics::Severity::Warning,
  primary_option_array: false,
  primary_option_object: false,
};

/// A rule whose primary option can be an array.
const ARRAY_RULE: RuleMeta = RuleMeta {
  primary_option_array: true,
  ..RULE
};

/// A rule whose primary option is an object.
const OBJECT_RULE: RuleMeta = RuleMeta {
  primary_option_object: true,
  ..RULE
};

fn setting(options: Value) -> Setting {
  Setting::new(&RULE, Some(&options))
}

fn array_setting(options: Value) -> Setting {
  Setting::new(&ARRAY_RULE, Some(&options))
}

fn object_setting(options: Value) -> Setting {
  Setting::new(&OBJECT_RULE, Some(&options))
}

fn texts(result: Result<(), Vec<InvalidOption>>) -> Vec<String> {
  result
    .err()
    .unwrap_or_default()
    .into_iter()
    .map(InvalidOption::into_text)
    .collect()
}

#[test]
fn values_are_quoted_as_stylelint_quotes_them() {
  assert_eq!(stringify(&json!("sideways")), "\"sideways\"");
  assert_eq!(stringify(&json!(5)), "\"5\"");
  assert_eq!(stringify(&json!(2.0)), "\"2\"");
  assert_eq!(stringify(&json!(1.5)), "\"1.5\"");
  assert_eq!(stringify(&json!(true)), "\"true\"");
  assert_eq!(stringify(&json!(null)), "\"null\"");
  assert_eq!(stringify(&json!(["a", 1])), "\"[\"a\",1]\"");
  assert_eq!(
    stringify(&json!({ "b": [1], "a": "x" })),
    "\"{\"b\":[1],\"a\":\"x\"}\""
  );
  assert_eq!(stringify(&json!("/^a\\d/")), "\"/^a\\d/\"");
}

#[test]
fn settings_normalise_like_stylelint() {
  let plain = setting(json!("always"));
  assert_eq!(plain.primary(), Some(&json!("always")));
  assert_eq!(plain.secondary(), None);

  let pair = setting(json!(["always", { "ignore": ["x"] }]));
  assert_eq!(pair.primary(), Some(&json!("always")));
  assert_eq!(pair.secondary(), Some(&json!({ "ignore": ["x"] })));

  // Gale reads `[true, {...}]` and a bare `{...}` as the secondary
  // options of a rule run with its default primary option.
  for written in [
    json!([true, { "ignore": ["x"] }]),
    json!({ "ignore": ["x"] }),
  ] {
    for defaults in [setting(written.clone()), array_setting(written)] {
      assert_eq!(defaults.primary(), None);
      assert_eq!(defaults.secondary(), Some(&json!({ "ignore": ["x"] })));
    }
  }
  let enabled = array_setting(json!([true]));
  assert_eq!((enabled.primary(), enabled.secondary()), (None, None));
  // What follows `true` is the setting.
  assert_eq!(setting(json!([true, "long"])), setting(json!("long")));
  assert_eq!(
    array_setting(json!([true, ["a", { "b": 1 }]])),
    array_setting(json!(["a", { "b": 1 }]))
  );

  let none = Setting::new(&RULE, None);
  assert_eq!((none.primary(), none.secondary()), (None, None));

  for meta in [RULE, ARRAY_RULE, OBJECT_RULE] {
    let off = Setting::new(&meta, Some(&json!([null, { "a": 1 }])));
    assert_eq!((off.primary(), off.secondary()), (None, None));
  }
}

#[test]
fn array_primaries_normalise_like_stylelint() {
  let list = array_setting(json!(["a", "b"]));
  assert_eq!(list.primary(), Some(&json!(["a", "b"])));
  assert_eq!(list.secondary(), None);

  let wrapped = array_setting(json!([["a", "b"]]));
  assert_eq!(wrapped.primary(), Some(&json!(["a", "b"])));

  let with_secondary = array_setting(json!([["a"], { "severity": "warning" }]));
  assert_eq!(with_secondary.primary(), Some(&json!(["a"])));
  assert_eq!(
    with_secondary.secondary(),
    Some(&json!({ "severity": "warning" }))
  );

  let string_then_object = array_setting(json!(["a", { "ignore": [] }]));
  assert_eq!(string_then_object.primary(), Some(&json!("a")));
  assert!(string_then_object.secondary().is_some());

  let objects = array_setting(json!([{ "type": "rule" }, { "type": "at-rule" }]));
  assert_eq!(
    objects.primary(),
    Some(&json!([{ "type": "rule" }, { "type": "at-rule" }]))
  );
  assert_eq!(objects.secondary(), None);

  let three = array_setting(json!(["a", "b", { "x": 1 }]));
  assert_eq!(three.primary(), Some(&json!(["a", "b", { "x": 1 }])));

  // Not an array rule: the second entry is the secondary options.
  let scalar = setting(json!(["a", "b"]));
  assert_eq!(scalar.primary(), Some(&json!("a")));
  assert_eq!(scalar.secondary(), Some(&json!("b")));
}

#[test]
fn an_invalid_primary_is_reported_with_stylelints_text() {
  let s = setting(json!("sideways"));
  let mut options = s.options();
  assert_eq!(options.primary_one_of(&["short", "long"]), None);
  assert_eq!(
    texts(options.finish()),
    ["Invalid option value \"sideways\" for rule \"test-rule\""]
  );

  let s = setting(json!("long"));
  let mut options = s.options();
  assert_eq!(options.primary_one_of(&["short", "long"]), Some("long"));
  assert!(options.finish().is_ok());

  let none = Setting::new(&RULE, None);
  let mut options = none.options();
  assert_eq!(options.primary_one_of(&["short", "long"]), None);
  assert!(options.finish().is_ok(), "no primary means the default");
}

#[test]
fn each_bad_list_entry_is_reported() {
  let s = array_setting(json!(["color", 5, true]));
  let mut options = s.options();
  assert_eq!(options.primary_strings(), None);
  assert_eq!(
    texts(options.finish()),
    [
      "Invalid option value \"5\" for rule \"test-rule\"",
      "Invalid option value \"true\" for rule \"test-rule\""
    ]
  );

  let s = array_setting(json!(5));
  let mut options = s.options();
  options.primary_strings();
  assert_eq!(
    texts(options.finish()),
    ["Invalid option value \"5\" for rule \"test-rule\""]
  );
}

#[test]
fn boolean_rules_word_their_problems_by_how_stylelint_validates_them() {
  let s = setting(json!("yes"));
  let mut options = s.options();
  options.primary_true();
  assert_eq!(
    texts(options.finish()),
    ["Invalid option value \"yes\" for rule \"test-rule\""]
  );

  let mut options = s.options();
  options.no_primary();
  assert_eq!(
    texts(options.finish()),
    ["Unexpected option value \"yes\" for rule \"test-rule\""]
  );

  for ok in [json!(true), json!({})] {
    let s = setting(ok);
    let mut options = s.options();
    options.primary_true();
    options.no_primary();
    assert!(options.finish().is_ok());
  }
}

#[test]
fn false_asks_whether_the_rule_was_meant_to_be_off() {
  let s = setting(json!([false]));
  let mut options = s.options();
  options.primary_one_of(&["a"]);
  assert_eq!(
    texts(options.finish()),
    [
      "Invalid option value \"false\" for rule \"test-rule\". Are you trying to disable this rule? If so use \"null\" instead"
    ]
  );
}

#[test]
fn numbers_and_tests() {
  let s = setting(json!(2.0));
  let mut options = s.options();
  assert_eq!(
    options.primary_number(&[Accept::NonNegativeInteger]),
    Some(2.0)
  );
  assert!(options.finish().is_ok());

  let s = setting(json!(-1));
  let mut options = s.options();
  assert_eq!(options.primary_number(&[Accept::NonNegativeInteger]), None);
  assert_eq!(
    texts(options.finish()),
    ["Invalid option value \"-1\" for rule \"test-rule\""]
  );

  let mut options = s.options();
  options.primary_test(|v| v.as_f64().is_some_and(|n| n >= 0.0));
  assert_eq!(
    texts(options.finish()),
    ["Invalid option \"-1\" for rule \"test-rule\""]
  );
}

#[test]
fn patterns_compile_once_and_report_why_they_do_not() {
  let s = setting(json!("^[a-z]+$"));
  let mut options = s.options();
  let pattern = options.primary_pattern().unwrap();
  assert!(pattern.is_match("abc"));
  assert_eq!(pattern.source, "^[a-z]+$");
  assert!(options.finish().is_ok());

  let s = setting(json!("^[a-z"));
  let mut options = s.options();
  assert!(options.primary_pattern().is_none());
  let problems = texts(options.finish());
  assert_eq!(problems.len(), 1);
  assert!(
    problems[0].starts_with("Invalid option value \"^[a-z\" for rule \"test-rule\": "),
    "{problems:?}"
  );
}

#[test]
fn secondary_options_are_validated_and_unknown_ones_named() {
  let s = setting(json!([
    "never",
    {
      "ignore": ["inside-function", "outside"],
      "ignoreProperties": ["fill", "/^my-/"],
      "ignoreEverything": true,
      "severity": "warning",
      "message": "x",
      "url": "y",
      "disableFix": true,
      "reportDisables": true
    }
  ]));
  let mut options = s.options();
  assert_eq!(options.primary_one_of(&["never"]), Some("never"));
  assert!(options.keywords("ignore", &["inside-function"]).is_empty());
  let properties = options.matchers("ignoreProperties");
  assert!(properties.matches("fill"));
  assert!(properties.matches("my-color"));
  assert!(!properties.matches("color"));
  assert!(!options.flag("ignoreNothing"));
  assert_eq!(
    texts(options.finish()),
    [
      "Invalid value \"outside\" for option \"ignore\" of rule \"test-rule\"",
      "Invalid option name \"ignoreEverything\" for rule \"test-rule\""
    ]
  );
}

#[test]
fn a_rule_that_reads_no_secondary_options_ignores_them() {
  let s = setting(json!(["short", { "ignoreEverything": true }]));
  let mut options = s.options();
  options.primary_one_of(&["short"]);
  assert!(options.finish().is_ok());
}

#[test]
fn secondary_options_must_be_an_object() {
  let s = setting(json!(["always", "sometimes"]));
  let mut options = s.options();
  options.primary_one_of(&["always"]);
  options.flag("ignoreThings");
  assert_eq!(
    texts(options.finish()),
    ["Invalid option value \"sometimes\" for rule \"test-rule\": should be an object"]
  );
}

#[test]
fn a_bare_object_is_the_primary_of_an_object_rule() {
  for written in [
    json!({ "color": ["red"] }),
    json!([true, { "color": ["red"] }]),
  ] {
    let s = object_setting(written);
    assert_eq!(s.primary(), Some(&json!({ "color": ["red"] })));
    assert_eq!(s.secondary(), None);
  }

  let s = object_setting(json!({ "color": ["red"] }));
  let mut options = s.options();
  let map = options.primary_object().unwrap();
  assert!(map.contains_key("color"));
  assert!(options.strings("ignore").is_empty());
  assert!(
    options.finish().is_ok(),
    "its keys are not secondary options"
  );

  let s = object_setting(json!([{ "color": ["red"] }, { "ignore": ["x"] }]));
  let mut options = s.options();
  assert!(options.primary_object().is_some());
  assert_eq!(options.strings("ignore"), ["x"]);
  assert!(options.finish().is_ok());
}

#[test]
fn matchers_match_names_exactly_and_regexes_by_search() {
  let list = Matchers::new(&["color", "/^border/i", "/[broken/"]);
  assert!(list.matches("color"));
  assert!(!list.matches("Color"));
  assert!(list.matches("BORDER-top"));
  assert!(!list.matches("[broken"));
  assert!(Matchers::default().is_empty());

  let any_case = Matchers::ignoring_case(&["My-Fn", "/^theme-/"]);
  assert!(any_case.matches("my-fn"));
  assert!(any_case.matches("MY-FN"));
  assert!(any_case.matches("theme-x"));
  assert!(!any_case.matches("Theme-x"), "a regex matches as written");
}

#[test]
fn object_validators_follow_stylelint() {
  assert!(is_object_of_string_lists(&json!({ "a": "x", "b": ["y"] })));
  assert!(!is_object_of_string_lists(&json!({ "a": [1] })));
  assert!(!is_object_of_string_lists(&json!(["a"])));
  assert!(is_object_of_strings(&json!({ "a": "x" })));
  assert!(!is_object_of_strings(&json!({ "a": ["x"] })));
  assert!(is_object_of_non_negative_integers(
    &json!({ "a": 0, "b": 2 })
  ));
  assert!(!is_object_of_non_negative_integers(&json!({ "a": 1.5 })));
  assert!(is_truthy(&json!("x")) && is_truthy(&json!([])) && !is_truthy(&json!(0)));
}

#[test]
fn lenient_strings_leave_other_keys_alone() {
  let s = setting(json!([true, { "names": ["a", 1], "other": true }]));
  let mut options = s.options();
  assert_eq!(options.lenient_strings("names"), ["a"]);
  assert_eq!(
    texts(options.finish()),
    ["Invalid value \"1\" for option \"names\" of rule \"test-rule\""]
  );
}

#[test]
fn a_rule_that_validates_disable_fix_reports_a_bad_value() {
  let s = setting(json!(["a", { "disableFix": "yes" }]));
  let mut options = s.options();
  options.disable_fix();
  assert_eq!(
    texts(options.finish()),
    ["Invalid value \"yes\" for option \"disableFix\" of rule \"test-rule\""]
  );

  let s = setting(json!(["a", { "disableFix": true, "other": 1 }]));
  let mut options = s.options();
  options.disable_fix();
  assert_eq!(
    texts(options.finish()),
    ["Invalid option name \"other\" for rule \"test-rule\""]
  );
}
