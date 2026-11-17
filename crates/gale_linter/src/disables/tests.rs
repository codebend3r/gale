use super::*;
use gale_diagnostics::SourceLineIndex;

/// The ranges of `text`, parsed as `syntax`, as `(rule, start, end)`
/// line triples in list order, and the first rejected comment.
/// A disabled range as `(rule, start line, end line)`.
type RangeTuple = (String, usize, Option<usize>);

fn ranges_of(text: &str, syntax: Syntax) -> (Vec<RangeTuple>, Option<String>) {
  let index = SourceLineIndex::build(text);
  let line_of = |offset: usize| index.line(offset);
  let mut collector = Collector::new(&line_of);
  collector.scan(text, syntax, 0);
  let (ranges, error) = collector.finish();
  let triples = ranges
    .rules
    .iter()
    .flat_map(|(rule, list)| list.iter().map(move |r| (rule.clone(), r.start, r.end)))
    .collect();
  (triples, error.map(|e| e.message))
}

/// `(rule, start, end)` with the end line given.
fn closed(rule: &str, start: usize, end: usize) -> (String, usize, Option<usize>) {
  (rule.to_string(), start, Some(end))
}

/// `(rule, start, None)`: open to the end of the file.
fn open(rule: &str, start: usize) -> (String, usize, Option<usize>) {
  (rule.to_string(), start, None)
}

#[test]
fn commands_are_the_first_word() {
  assert_eq!(command("stylelint-disable a"), Some((Command::Disable, 17)));
  assert_eq!(
    command("gale-disable-next-line"),
    Some((Command::DisableNextLine, 22))
  );
  assert_eq!(command("stylelint-enable\na"), Some((Command::Enable, 16)));
  assert_eq!(command("stylelint-disables"), None);
  assert_eq!(command("see stylelint-disable"), None);
}

#[test]
fn rules_stop_at_the_description() {
  let rules = |text: &str| command_rules(text, command(text).unwrap().1);
  assert_eq!(rules("stylelint-disable"), vec!["all"]);
  assert_eq!(rules("stylelint-disable -- why"), vec!["all"]);
  assert_eq!(rules("stylelint-disable a, b -- c, d"), vec!["a", "b"]);
  assert_eq!(rules("stylelint-disable a,,b"), vec!["a", "b"]);
  assert_eq!(rules("stylelint-disable a\n-- why"), vec!["a"]);
  // Without whitespace around the dashes there is no description.
  assert_eq!(rules("stylelint-disable a --why"), vec!["a --why"]);
  assert!(has_description("stylelint-disable a --why"));
  assert!(!has_description("stylelint-disable a --  "));
  assert!(!has_description("stylelint-disable a"));
}

#[test]
fn a_disable_covers_its_own_line_and_the_enable_line() {
  let (ranges, _) = ranges_of(
    "a {} /* stylelint-disable */\nb {}\n/* stylelint-enable */ c {}\nd {}",
    Syntax::Css,
  );
  assert_eq!(ranges, vec![closed("all", 1, 3)]);
}

#[test]
fn line_commands_cover_one_line() {
  let (ranges, _) = ranges_of(
    "a {} /* stylelint-disable-line a */\n/* stylelint-disable-next-line\n  b */\nc {}",
    Syntax::Css,
  );
  assert_eq!(ranges, vec![closed("a", 1, 1), closed("b", 4, 4)]);
}

#[test]
fn comments_inside_nodes_count_but_strings_do_not() {
  let (ranges, _) = ranges_of(
    "a {\n  color: /* stylelint-disable-line a */ red;\n  content: \"/* stylelint-disable */\";\n}\n\
       b, /* stylelint-disable-line b */\nc {}\n@media /* stylelint-disable-line c */ x {}",
    Syntax::Css,
  );
  assert_eq!(
    ranges,
    vec![closed("a", 2, 2), closed("b", 5, 5), closed("c", 7, 7)]
  );
}

#[test]
fn double_slash_comments_count_in_scss_and_less_only() {
  let source = "// stylelint-disable-next-line a\nx {}\n";
  assert_eq!(ranges_of(source, Syntax::Css).0, vec![]);
  assert_eq!(ranges_of(source, Syntax::Scss).0, vec![closed("a", 2, 2)]);
  assert_eq!(ranges_of(source, Syntax::Less).0, vec![closed("a", 2, 2)]);
  // Inside a value, postcss-scss reads them as block comments.
  let value = "a {\n  b: c, // stylelint-disable-line a\n    d;\n}\n";
  assert_eq!(ranges_of(value, Syntax::Scss).0, vec![closed("a", 2, 2)]);
  assert_eq!(ranges_of(value, Syntax::Less).0, vec![]);
}

#[test]
fn a_description_on_the_next_lines_extends_the_comment() {
  for source in [
    "// stylelint-disable-next-line a --\n// because\nx {}\n",
    "// stylelint-disable-next-line a\n// -- because\nx {}\n",
    "// stylelint-disable-next-line a -- why\n// more\n// and more\nx {}\nx {}\n",
  ] {
    let lines = source.lines().count();
    let target = source.lines().position(|l| l == "x {}").unwrap() + 1;
    assert!(target <= lines);
    assert_eq!(
      ranges_of(source, Syntax::Scss).0,
      vec![closed("a", target, target)],
      "{source}"
    );
  }
  // A gap ends the run.
  assert_eq!(
    ranges_of(
      "// stylelint-disable-next-line a -- why\n\n// more\nx {}\n",
      Syntax::Scss
    )
    .0,
    vec![closed("a", 2, 2)]
  );
  // Dashes with nothing after them are part of the rule name, as in
  // Stylelint, once nothing merges to follow them.
  assert_eq!(
    ranges_of(
      "// stylelint-disable-next-line a --\n\n// why\nx {}\n",
      Syntax::Scss
    )
    .0,
    vec![closed("a --", 2, 2)]
  );
  // Without a description nothing merges.
  assert_eq!(
    ranges_of(
      "// stylelint-disable-next-line a\n// why\nx {}\n",
      Syntax::Scss
    )
    .0,
    vec![closed("a", 2, 2)]
  );
}

#[test]
fn named_rules_start_from_the_blanket_ranges() {
  let (ranges, _) = ranges_of(
    "/* stylelint-disable */\n/* stylelint-enable */\n/* stylelint-disable a */\n",
    Syntax::Css,
  );
  assert_eq!(
    ranges,
    vec![closed("all", 1, 2), closed("a", 1, 2), open("a", 3)]
  );
}

#[test]
fn a_plain_enable_ends_named_disables_too() {
  let (ranges, error) = ranges_of(
    "/* stylelint-disable a */\nx {}\n/* stylelint-enable */\nx {}\n",
    Syntax::Css,
  );
  assert_eq!(ranges, vec![closed("a", 1, 3)]);
  assert_eq!(error, None);
}

#[test]
fn enabling_one_rule_under_a_blanket_disable_leaves_the_rest_off() {
  let (ranges, error) = ranges_of(
    "/* stylelint-disable */\nx {}\n/* stylelint-enable a */\nx {}\n",
    Syntax::Css,
  );
  assert_eq!(ranges, vec![open("all", 1), closed("a", 1, 3)]);
  assert_eq!(error, None);
}

#[test]
fn a_plain_enable_ends_only_the_last_range_of_each_rule() {
  // `a` keeps its first range open, as in Stylelint.
  let (ranges, _) = ranges_of(
    "/* stylelint-disable a */\n/* stylelint-disable */\n/* stylelint-enable */\n",
    Syntax::Css,
  );
  assert_eq!(
    ranges,
    vec![closed("all", 2, 3), open("a", 1), closed("a", 2, 3)]
  );
}

#[test]
fn a_blanket_disable_opens_a_range_for_every_named_rule() {
  let (ranges, _) = ranges_of(
    "/* stylelint-disable a */\n/* stylelint-disable */\n",
    Syntax::Css,
  );
  assert_eq!(ranges, vec![open("all", 2), open("a", 1), open("a", 2)]);
}

/// The nodes the ranges of `text` point at, as the text each spans.
fn node_texts(text: &str, syntax: Syntax) -> Vec<String> {
  let index = SourceLineIndex::build(text);
  let line_of = |offset: usize| index.line(offset);
  let mut collector = Collector::new(&line_of);
  collector.scan(text, syntax, 0);
  let (ranges, _) = collector.finish();
  ranges
    .nodes
    .iter()
    .map(|node| {
      assert_eq!(text[node.last..node.end].chars().count(), 1);
      text[node.start..node.end].to_string()
    })
    .collect()
}

#[test]
fn ranges_point_at_the_node_that_holds_the_comment() {
  assert_eq!(
    node_texts(
      "/* stylelint-disable a */\nb {\n  c: /* stylelint-disable-line d */ e;\n}\n",
      Syntax::Css
    ),
    vec![
      "/* stylelint-disable a */",
      "c: /* stylelint-disable-line d */ e;"
    ]
  );
  // Merged `//` comments are one node.
  assert_eq!(
    node_texts(
      "// stylelint-disable-next-line a\n// -- why\nb {}\n",
      Syntax::Scss
    ),
    vec!["// stylelint-disable-next-line a\n// -- why"]
  );
}

#[test]
fn rejected_comments_are_recorded() {
  for (source, message) in [
    (
      "/* stylelint-disable */\n/* stylelint-disable */",
      "All rules have already been disabled",
    ),
    (
      "/* stylelint-disable a */\n/* stylelint-disable a */",
      "\"a\" has already been disabled",
    ),
    (
      "/* stylelint-disable */\n/* stylelint-disable-line a */",
      "All rules have already been disabled",
    ),
    ("/* stylelint-enable a */", "\"a\" has not been disabled"),
    ("/* stylelint-enable */", "No rules have been disabled"),
  ] {
    let (_, error) = ranges_of(source, Syntax::Css);
    assert_eq!(error.as_deref(), Some(message), "{source}");
  }
  // A line disabled twice is fine: each range closes on its line.
  let (_, error) = ranges_of(
    "/* stylelint-disable-line a */ /* stylelint-disable-line a */",
    Syntax::Css,
  );
  assert_eq!(error, None);
}

#[test]
fn problems_check_their_rule_list_first() {
  let text = "/* stylelint-disable */\n/* stylelint-enable */\n/* stylelint-disable a */\nx {}\n";
  let index = SourceLineIndex::build(text);
  let line_of = |offset: usize| index.line(offset);
  let mut collector = Collector::new(&line_of);
  collector.scan(text, Syntax::Css, 0);
  let (ranges, _) = collector.finish();
  assert!(ranges.covers("a", 4));
  assert!(ranges.covers("b", 1));
  assert!(!ranges.covers("b", 4));
  // An alias names the same rule.
  let text = "/* stylelint-disable color-hex-case */\n";
  let index = SourceLineIndex::build(text);
  let line_of = |offset: usize| index.line(offset);
  let mut collector = Collector::new(&line_of);
  collector.scan(text, Syntax::Css, 0);
  let (ranges, _) = collector.finish();
  assert!(ranges.covers("@stylistic/color-hex-case", 1));
}
