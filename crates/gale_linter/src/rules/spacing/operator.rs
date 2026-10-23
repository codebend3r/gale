//! The range operators of media features (`>=`, `<`, ...) and the
//! operators of attribute selectors (`=`, `~=`, ...), found as the plugin's
//! `findMediaOperator` and `selectorAttributeOperatorSpaceChecker` find
//! them.

use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{self as selector_parser, Kind};
use gale_diagnostics::Diagnostic;

use super::{Text, index_in, selector_node_string, space_after, space_before};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::whitespace::{
  ALWAYS_NEVER, At, Messages, SearchOptions, Spacing, SpacingRule, Target, replace,
  space_run_after, space_run_before, style_search,
};

/// The range operators, longest first so that `>=` is not read as `>`.
const RANGE_OPERATORS: &[&str] = &[">=", "<=", ">", "<", "="];

/// The plugin's `findMediaOperator`: every range operator in every `@media`
/// at-rule's params as written, but for the `=` of `>=` and `<=`.  Gives
/// the start of the operator and its last character.
fn media_range_operators(tree: &PostcssTree, mut each: impl FnMut(&Text, usize, usize)) {
  for at in tree.walk_at_rules() {
    if !at.name().eq_ignore_ascii_case("media") {
      continue;
    }
    let params = at.raw_params();
    let text = Text {
      text: &params,
      report_base: at.start + at.param_index(),
      fix_base: at.params_fixable().then(|| at.params_start()),
    };
    style_search(&params, &SearchOptions::any(RANGE_OPERATORS), |m| {
      let before = m.start.checked_sub(1).map(|i| params.as_bytes()[i]);
      if !matches!(before, Some(b'>' | b'<')) {
        each(&text, m.start, m.end - 1);
      }
    });
  }
}

const RANGE_OPERATOR_SPACE_AFTER: &str = "@stylistic/media-feature-range-operator-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the range
  /// operators of media features.
  ///
  /// Equivalent to `@stylistic/media-feature-range-operator-space-after`.
  pub struct StylisticMediaFeatureRangeOperatorSpaceAfter => Spacing<StylisticMediaFeatureRangeOperatorSpaceAfter> {
    name: RANGE_OPERATOR_SPACE_AFTER,
    description: "Require or disallow a space after the range operator in media features",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaFeatureRangeOperatorSpaceAfter {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after range operator"),
    rejected_after: Some("Unexpected whitespace after range operator"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// Reported just past the operator.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    media_range_operators(tree, |text, _, last| {
      let text = Text {
        report_base: text.report_base + 1,
        ..*text
      };
      let found = space_after(rule, tree.source(), &text, last);
      problems.extend(found);
    });
    problems
  }
}

const RANGE_OPERATOR_SPACE_BEFORE: &str = "@stylistic/media-feature-range-operator-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the range
  /// operators of media features.
  ///
  /// Equivalent to `@stylistic/media-feature-range-operator-space-before`.
  pub struct StylisticMediaFeatureRangeOperatorSpaceBefore => Spacing<StylisticMediaFeatureRangeOperatorSpaceBefore> {
    name: RANGE_OPERATOR_SPACE_BEFORE,
    description: "Require or disallow a space before the range operator in media features",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaFeatureRangeOperatorSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before range operator"),
    rejected_before: Some("Unexpected whitespace before range operator"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// Reported just before the operator.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    media_range_operators(tree, |text, start, _| {
      let text = Text {
        report_base: text.report_base.wrapping_sub(1),
        ..*text
      };
      problems.extend(space_before(rule, tree.source(), &text, start));
    });
    problems
  }
}

/// An attribute selector's operator, as the plugin's
/// `selectorAttributeOperatorSpaceChecker` checks it.
struct Operator<'a> {
  /// The attribute selector, `[...]`, as written.
  attribute: &'a str,
  /// The operator.
  operator: &'a str,
  /// Where the operator starts in `attribute`.
  start: usize,
  /// The rule's selector as written, its start in the source and the
  /// attribute's `sourceIndex` in it, which problems are reported from.
  selector: &'a str,
  rule_start: usize,
  source_index: usize,
  /// The source offset of the attribute selector, when the selector is the
  /// source as written.
  fix_base: Option<usize>,
}

impl Operator<'_> {
  /// Where the plugin reports index `index` of the attribute's string.
  fn report(&self, source: &str, index: usize) -> usize {
    index_in(
      source,
      self.rule_start,
      self.selector,
      self.source_index + index,
    )
  }
}

/// The operators of the attribute selectors of every standard rule with a
/// `[` and an `=`.
fn attribute_operators(tree: &PostcssTree, mut each: impl FnMut(&Operator)) {
  for rule in tree.walk_rules() {
    let cleaned = rule.selector();
    if !rule.is_standard_syntax() || !cleaned.contains('[') || !cleaned.contains('=') {
      continue;
    }
    let selector = rule.raw_selector();
    let Some(selectors) = selector_parser::parse(&selector, 0) else {
      continue;
    };
    let fixable = rule.selector_fixable();
    selector_parser::walk(&selectors, &mut |visit| {
      let node = visit.node;
      let Some(operator) = node.attr.as_ref().and_then(|attr| attr.operator.as_deref()) else {
        return;
      };
      if node.kind != Kind::Attribute {
        return;
      }
      // Indexes count in `toString()`, from `sourceIndex`.
      let (start, attribute) = selector_node_string(&selector, visit);
      style_search(attribute, &SearchOptions::new(operator), |m| {
        each(&Operator {
          attribute,
          operator,
          start: m.start,
          selector: &selector,
          rule_start: rule.start,
          source_index: node.start,
          fix_base: fixable.then(|| rule.selector_start() + start),
        });
      });
    });
  }
}

const ATTRIBUTE_OPERATOR_SPACE_AFTER: &str = "@stylistic/selector-attribute-operator-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the operators of
  /// attribute selectors.
  ///
  /// Equivalent to `@stylistic/selector-attribute-operator-space-after`.
  pub struct StylisticSelectorAttributeOperatorSpaceAfter => Spacing<StylisticSelectorAttributeOperatorSpaceAfter> {
    name: ATTRIBUTE_OPERATOR_SPACE_AFTER,
    description: "Require or disallow a space after the operator within attribute selectors",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorAttributeOperatorSpaceAfter {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \"%s\""),
    rejected_after: Some("Unexpected whitespace after \"%s\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks after the operator's last character and words the
  /// problem with the whole operator.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    attribute_operators(tree, |op| {
      let index = op.start + op.operator.len() - 1;
      let at = At::new(op.attribute, index).err_target(op.operator);
      let Some(message) = rule.checker.after(at) else {
        return;
      };
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let run = space_run_after(&op.attribute[..op.attribute.len() - 1], index + 1);
      let edits = op
        .fix_base
        .map(|base| vec![replace(base, run, with)])
        .unwrap_or_default();
      problems.push(rule.problem_fixed_by(
        tree.source(),
        message,
        op.report(tree.source(), index),
        edits,
      ));
    });
    problems
  }
}

const ATTRIBUTE_OPERATOR_SPACE_BEFORE: &str = "@stylistic/selector-attribute-operator-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the operators of
  /// attribute selectors.
  ///
  /// Equivalent to `@stylistic/selector-attribute-operator-space-before`.
  pub struct StylisticSelectorAttributeOperatorSpaceBefore => Spacing<StylisticSelectorAttributeOperatorSpaceBefore> {
    name: ATTRIBUTE_OPERATOR_SPACE_BEFORE,
    description: "Require or disallow a space before the operator within attribute selectors",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorAttributeOperatorSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \"%s\""),
    rejected_before: Some("Unexpected whitespace before \"%s\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks before the operator's first character and words
  /// the problem with the whole operator.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    attribute_operators(tree, |op| {
      let at = At::new(op.attribute, op.start).err_target(op.operator);
      let Some(message) = rule.checker.before(at) else {
        return;
      };
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let run = space_run_before(op.attribute, op.start);
      let bracket = op.attribute.find('[').map_or(0, |at| at + 1);
      let run = run.start.max(bracket)..run.end;
      let edits = op
        .fix_base
        .map(|base| vec![replace(base, run, with)])
        .unwrap_or_default();
      problems.push(rule.problem_fixed_by(
        tree.source(),
        message,
        op.report(tree.source(), op.start),
        edits,
      ));
    });
    problems
  }
}

#[cfg(test)]
#[path = "cases/operator.rs"]
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
  fn media_range_operators() {
    let after = "@stylistic/media-feature-range-operator-space-after";
    let source = "@media (width>=600px) and (400px <  width) {}";
    assert_eq!(
      problems(after, "always", source),
      [
        (1, 16, "Expected single space after range operator".into()),
        (1, 35, "Expected single space after range operator".into()),
      ]
    );
    assert_eq!(
      fix(after, json!("always"), source, Syntax::Css),
      "@media (width>= 600px) and (400px < width) {}"
    );
    let before = "@stylistic/media-feature-range-operator-space-before";
    assert_eq!(
      fix(before, json!("never"), source, Syntax::Css),
      "@media (width>=600px) and (400px<  width) {}"
    );
  }

  #[test]
  fn attribute_operators() {
    let after = "@stylistic/selector-attribute-operator-space-after";
    let source = "[a~= b], [c=\"=\"] {}";
    assert_eq!(
      problems(after, "never", source),
      [(1, 4, "Unexpected whitespace after \"~=\"".into())]
    );
    assert_eq!(
      fix(after, json!("never"), source, Syntax::Css),
      "[a~=b], [c=\"=\"] {}"
    );
    let before = "@stylistic/selector-attribute-operator-space-before";
    assert_eq!(
      fix(before, json!("always"), source, Syntax::Css),
      "[a ~= b], [c =\"=\"] {}"
    );
  }
}
