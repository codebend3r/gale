//! The combinators of selectors (`>`, `+`, `~`, ...), found as the
//! plugin's `selectorCombinatorSpaceChecker` finds them: those between two
//! compounds of a standard rule's selector, outside pseudo-class arguments,
//! with the descendant combinator (whitespace) left out.

use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{self as selector_parser, Kind};
use gale_diagnostics::Diagnostic;

use super::index_in;
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::whitespace::{ALWAYS_NEVER, At, Messages, Spacing, SpacingRule, Target, replace};

/// Which side of the combinator a rule checks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
  Before,
  After,
}

/// Check the whitespace on `side` of every combinator, fixing it to one
/// space or none where it is only whitespace.
fn check_combinators<R: SpacingRule>(
  rule: &Spacing<R>,
  tree: &PostcssTree,
  side: Side,
) -> Vec<Diagnostic> {
  let mut problems = Vec::new();
  for node in tree.walk_rules() {
    if !node.is_standard_syntax() {
      continue;
    }
    let selector = node.raw_selector();
    let Some(selectors) = selector_parser::parse(&selector, 0) else {
      continue;
    };
    let fix_base = node.selector_fixable().then(|| node.selector_start());
    selector_parser::walk(&selectors, &mut |visit| {
      let combinator = visit.node;
      let value = combinator.value.as_str();
      // `isStandardSyntaxCombinator`: no `/deep/`-like combinator, and none
      // that starts or ends its selector; then no descendant combinator and
      // none inside a pseudo-class's argument.
      if combinator.kind != Kind::Combinator
        || value.starts_with('/')
        || value.ends_with('/')
        || visit.index == 0
        || visit.index + 1 == visit.siblings.len()
        || value.chars().any(char::is_whitespace)
        || visit.parent_pseudo.is_some()
      {
        return;
      }
      let index = if side == Side::Before && value.len() > 1 {
        combinator.start
      } else {
        combinator.start + value.len() - 1
      };
      let at = At::new(&selector, index).err_target(value);
      let message = match side {
        Side::Before => rule.checker.before(at),
        Side::After => rule.checker.after(at),
      };
      let Some(message) = message else {
        return;
      };
      // `combinator.spaces.before/after`, which only applies where they are
      // whitespace: a comment there is kept in `raws.spaces`.
      let space = match side {
        Side::Before => visit.prev().map_or(0, |prev| prev.end)..combinator.start,
        Side::After => combinator.end..visit.next().map_or(combinator.end, |next| next.start),
      };
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let edits = match fix_base {
        Some(base) if selector[space.clone()].chars().all(char::is_whitespace) => {
          vec![replace(base, space, with)]
        }
        _ => vec![],
      };
      let report = index_in(tree.source(), node.start, &selector, combinator.start);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    });
  }
  problems
}

const COMBINATOR_SPACE_AFTER: &str = "@stylistic/selector-combinator-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the combinators of
  /// selectors.
  ///
  /// Equivalent to `@stylistic/selector-combinator-space-after`.
  pub struct StylisticSelectorCombinatorSpaceAfter => Spacing<StylisticSelectorCombinatorSpaceAfter> {
    name: COMBINATOR_SPACE_AFTER,
    description: "Require or disallow a space after combinators of selectors",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorCombinatorSpaceAfter {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \"%s\""),
    rejected_after: Some("Unexpected whitespace after \"%s\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    check_combinators(rule, tree, Side::After)
  }
}

const COMBINATOR_SPACE_BEFORE: &str = "@stylistic/selector-combinator-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the combinators
  /// of selectors.
  ///
  /// Equivalent to `@stylistic/selector-combinator-space-before`.
  pub struct StylisticSelectorCombinatorSpaceBefore => Spacing<StylisticSelectorCombinatorSpaceBefore> {
    name: COMBINATOR_SPACE_BEFORE,
    description: "Require or disallow a space before combinators of selectors",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorCombinatorSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \"%s\""),
    rejected_before: Some("Unexpected whitespace before \"%s\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    check_combinators(rule, tree, Side::Before)
  }
}

#[cfg(test)]
#[path = "cases/combinator.rs"]
mod cases;

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  /// `(line, column, message)` of each problem.
  fn problems(rule: &str, option: &str, source: &str) -> Vec<(usize, usize, String)> {
    let index = gale_diagnostics::SourceLineIndex::build(source);
    let mut found: Vec<_> = lint(rule, json!(option), source, Syntax::Css)
      .into_iter()
      .map(|d| {
        let (line, column) = index.offset_to_location(d.span.offset);
        (line, column, d.message)
      })
      .collect();
    found.sort();
    found
  }

  #[test]
  fn combinators_between_compounds_are_checked() {
    let after = "@stylistic/selector-combinator-space-after";
    let source = "a>b, c +  d, :is(e>f), > g, h i {}";
    assert_eq!(
      problems(after, "always", source),
      [
        (1, 2, "Expected single space after \">\"".into()),
        (1, 8, "Expected single space after \"+\"".into()),
      ]
    );
    assert_eq!(
      fix(after, json!("always"), source, Syntax::Css),
      "a> b, c + d, :is(e>f), > g, h i {}"
    );
    let before = "@stylistic/selector-combinator-space-before";
    assert_eq!(
      fix(before, json!("never"), "a > b, c\n~ d {}", Syntax::Css),
      "a> b, c~ d {}"
    );
  }
}
