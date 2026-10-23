//! The whitespace just inside parentheses and brackets: of functions in
//! declaration values, of media features, of pseudo-class arguments and of
//! attribute selectors.  The plugin's rules for these do not use the
//! checker: each compares the whitespace after the opening and before the
//! closing character itself, so each keeps its own comparison here, over
//! shared finders.

use std::borrow::Cow;
use std::ops::Range;

use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_css_parser::selector::postcss::{self as selector_parser, Kind};
use gale_css_parser::value_parser::{self, NodeKind, ValueNode};
use gale_diagnostics::{Diagnostic, Edit};

use super::{declaration_value_index, index_in, selector_node_string};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_function;
use crate::whitespace::{
  ALWAYS_NEVER, Expectation, Messages, SPACE_SINGLE_LINE, SearchOptions, Spacing, SpacingRule,
  Target, is_single_line, newline, replace, space_run_after, space_run_before, style_search,
};

/// The messages of the rules about both sides inside parentheses.
struct InsideMessages {
  expected_opening: &'static str,
  rejected_opening: &'static str,
  expected_closing: &'static str,
  rejected_closing: &'static str,
}

/// A function in a declaration's value, with where it sits.
struct Function<'v, 'a> {
  decl: DeclRef<'a>,
  node: &'a ValueNode<'v>,
  /// The declaration's value, as the plugin parses it.
  value: &'a str,
  /// `valueParser.stringify(node)`.
  string: String,
  /// Where the value starts in the source, when the value is as written.
  fix_base: Option<usize>,
}

impl Function<'_, '_> {
  /// The source offset of an index into the declaration's value.
  fn report(&self, offset: usize) -> usize {
    let start = self.decl.start + declaration_value_index(self.decl);
    index_in(self.decl.tree().source(), start, self.value, offset)
  }

  /// Where `valueNode.before`, the whitespace after the `(`, is in the
  /// value.
  fn before(&self) -> Range<usize> {
    let start = self.node.source_index + self.node.value.len() + 1;
    start..start + self.node.before.len()
  }

  /// Where `valueNode.after`, the whitespace before the `)`, is in the
  /// value.
  fn after(&self) -> Range<usize> {
    let close = self.node.source_index + self.string.len() - usize::from(!self.node.unclosed);
    close - self.node.after.len()..close
  }

  /// An edit of the value.
  fn edit(&self, range: Range<usize>, with: &str) -> Vec<Edit> {
    self
      .fix_base
      .map(|base| vec![replace(base, range, with)])
      .unwrap_or_default()
  }
}

/// The standard functions in the values of every declaration whose value
/// holds a `(`, as the plugin's `function-parentheses-*` rules walk them.
fn functions(tree: &PostcssTree, mut each: impl FnMut(&Function)) {
  for decl in tree.walk_decls() {
    if !decl.value().contains('(') {
      continue;
    }
    let value = decl.raw_value();
    let fix_base = matches!(value, Cow::Borrowed(_)).then(|| decl.value_start());
    let nodes = value_parser::parse(&value);
    value_parser::walk(&nodes, &mut |node| {
      if node.kind == NodeKind::Function && is_standard_syntax_function(node) {
        each(&Function {
          decl,
          node,
          value: &value,
          string: node.to_css(),
          fix_base,
        });
      }
      true
    });
  }
}

const FUNCTION_PARENTHESES_SPACE_INSIDE: &str = "@stylistic/function-parentheses-space-inside";

declare_rule! {
  /// Require a single space or disallow whitespace on the inside of the
  /// parentheses of functions.
  ///
  /// Equivalent to `@stylistic/function-parentheses-space-inside`.
  pub struct StylisticFunctionParenthesesSpaceInside => Spacing<StylisticFunctionParenthesesSpaceInside> {
    name: FUNCTION_PARENTHESES_SPACE_INSIDE,
    description: "Require or disallow a space inside the parentheses of functions",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticFunctionParenthesesSpaceInside {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages::NONE;
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The whitespace after the `(` and before the `)` must be exactly one
  /// space, or nothing; a single-line expectation applies to functions on
  /// one line.  Functions without arguments are left alone.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let single = |line: &'static str, plain: &'static str, single_line: bool| {
      if single_line { line } else { plain }
    };
    let mut problems = Vec::new();
    functions(tree, |function| {
      if function.node.nodes.is_empty() {
        return;
      }
      let single_line = is_single_line(&function.string);
      let (want, single_line_option) = match rule.expectation() {
        Expectation::Always => (" ", false),
        Expectation::Never => ("", false),
        Expectation::AlwaysSingleLine if single_line => (" ", true),
        Expectation::NeverSingleLine if single_line => ("", true),
        _ => return,
      };
      let always = want == " ";
      let opening = function.node.source_index + function.node.value.len() + 1;
      if function.node.before != want {
        let message = match always {
          true => single(
            "Expected single space after \"(\" in a single-line function",
            "Expected single space after \"(\"",
            single_line_option,
          ),
          false => single(
            "Unexpected whitespace after \"(\" in a single-line function",
            "Unexpected whitespace after \"(\"",
            single_line_option,
          ),
        };
        let edits = function.edit(function.before(), want);
        problems.push(rule.problem_fixed_by(
          tree.source(),
          message.into(),
          function.report(opening),
          edits,
        ));
      }
      let closing = (function.node.source_index + function.string.len()).saturating_sub(2);
      if function.node.after != want {
        let message = match always {
          true => single(
            "Expected single space before \")\" in a single-line function",
            "Expected single space before \")\"",
            single_line_option,
          ),
          false => single(
            "Unexpected whitespace before \")\" in a single-line function",
            "Unexpected whitespace before \")\"",
            single_line_option,
          ),
        };
        let edits = function.edit(function.after(), want);
        problems.push(rule.problem_fixed_by(
          tree.source(),
          message.into(),
          function.report(closing),
          edits,
        ));
      }
    });
    problems
  }
}

const FUNCTION_PARENTHESES_NEWLINE_INSIDE: &str = "@stylistic/function-parentheses-newline-inside";

declare_rule! {
  /// Require a newline or disallow whitespace on the inside of the
  /// parentheses of functions.
  ///
  /// Equivalent to `@stylistic/function-parentheses-newline-inside`.
  pub struct StylisticFunctionParenthesesNewlineInside => Spacing<StylisticFunctionParenthesesNewlineInside> {
    name: FUNCTION_PARENTHESES_NEWLINE_INSIDE,
    description: "Require or disallow a newline inside the parentheses of functions",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticFunctionParenthesesNewlineInside {
  const OPTIONS: &'static [&'static str] = &["always", "always-multi-line", "never-multi-line"];
  const DEFAULT: &'static str = "always-multi-line";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages::NONE;
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The whitespace after the `(` (with any space nodes after comments
  /// that start the arguments) must hold a newline, or be empty; likewise
  /// before the `)`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let expectation = rule.expectation();
    let mut problems = Vec::new();
    functions(tree, |function| {
      let multi_line = !is_single_line(&function.string);
      let node = function.node;
      // `getCheckBefore` / `getCheckAfter`: the space nodes at either end
      // of the arguments, past comments.
      let leading = || {
        node
          .nodes
          .iter()
          .filter(|n| n.kind != NodeKind::Comment)
          .take_while(|n| n.kind == NodeKind::Space)
      };
      let trailing = || {
        node
          .nodes
          .iter()
          .rev()
          .filter(|n| n.kind != NodeKind::Comment)
          .take_while(|n| n.kind == NodeKind::Space)
      };
      let check_before: String = std::iter::once(node.before)
        .chain(leading().map(|n| n.value))
        .collect();
      let mut check_after: Vec<&str> = trailing().map(|n| n.value).collect();
      check_after.reverse();
      let check_after = format!("{}{}", check_after.concat(), node.after);
      let newline = newline(tree);
      let opening = node.source_index + node.value.len() + 1;
      let closing = (node.source_index + function.string.len()).saturating_sub(2);
      let space = |n: &ValueNode| n.source_index..n.source_end_index;

      // The opening parenthesis.
      let opening_problem = match expectation {
        Expectation::Always if !check_before.contains('\n') => Some("Expected newline after \"(\""),
        Expectation::AlwaysMultiLine if multi_line && !check_before.contains('\n') => {
          Some("Expected newline after \"(\" in a multi-line function")
        }
        Expectation::NeverMultiLine if multi_line && !check_before.is_empty() => {
          Some("Unexpected whitespace after \"(\" in a multi-line function")
        }
        _ => None,
      };
      if let Some(message) = opening_problem {
        let edits = if expectation.is_always() {
          // `fixBeforeForAlways`: a newline before the last leading space
          // node, or before `valueNode.before`.
          match leading().last() {
            Some(last) => function.edit(last.source_index..last.source_index, newline),
            None => function.edit(function.before().start..function.before().start, newline),
          }
        } else {
          // `fixBeforeForNever`: no `before`, and no leading space nodes.
          let mut edits = function.edit(function.before(), "");
          for n in leading() {
            edits.extend(function.edit(space(n), ""));
          }
          edits
        };
        problems.push(rule.problem_fixed_by(
          tree.source(),
          message.into(),
          function.report(opening),
          edits,
        ));
      }

      // The closing parenthesis.
      let closing_problem = match expectation {
        Expectation::Always if !check_after.contains('\n') => Some("Expected newline before \")\""),
        Expectation::AlwaysMultiLine if multi_line && !check_after.contains('\n') => {
          Some("Expected newline before \")\" in a multi-line function")
        }
        Expectation::NeverMultiLine if multi_line && !check_after.is_empty() => {
          Some("Unexpected whitespace before \")\" in a multi-line function")
        }
        _ => None,
      };
      if let Some(message) = closing_problem {
        let edits = if expectation.is_always() {
          // `fixAfterForAlways`: a newline in front of `valueNode.after`.
          // In a function without arguments the newline goes to
          // `valueNode.before` instead and the problem stays: Stylelint
          // adds it once, and gale adds it unless one is there.
          if node.nodes.is_empty() && check_before.contains('\n') {
            vec![]
          } else {
            function.edit(function.after().start..function.after().start, newline)
          }
        } else {
          let mut edits = function.edit(function.after(), "");
          for n in trailing() {
            edits.extend(function.edit(space(n), ""));
          }
          edits
        };
        problems.push(rule.problem_fixed_by(
          tree.source(),
          message.into(),
          function.report(closing),
          edits,
        ));
      }
    });
    problems
  }
}

const MEDIA_FEATURE_PARENTHESES_SPACE_INSIDE: &str =
  "@stylistic/media-feature-parentheses-space-inside";

declare_rule! {
  /// Require a single space or disallow whitespace on the inside of the
  /// parentheses of media features.
  ///
  /// Equivalent to `@stylistic/media-feature-parentheses-space-inside`.
  pub struct StylisticMediaFeatureParenthesesSpaceInside => Spacing<StylisticMediaFeatureParenthesesSpaceInside> {
    name: MEDIA_FEATURE_PARENTHESES_SPACE_INSIDE,
    description: "Require or disallow a space inside the parentheses of media features",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaFeatureParenthesesSpaceInside {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages::NONE;
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// Every parenthesis of a `@media` at-rule's params, as the value parser
  /// reads them: `never` rejects a space or tab inside, `always` wants
  /// something there.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    const MESSAGES: InsideMessages = InsideMessages {
      expected_opening: "Expected single space after \"(\"",
      rejected_opening: "Unexpected whitespace after \"(\"",
      expected_closing: "Expected single space before \")\"",
      rejected_closing: "Unexpected whitespace before \")\"",
    };
    let always = rule.expectation() == Expectation::Always;
    let mut problems = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.name().eq_ignore_ascii_case("media") {
        continue;
      }
      let params = at.raw_params();
      let boost = at.start + at.param_index();
      let fix_base = at.params_fixable().then(|| at.params_start());
      let nodes = value_parser::parse(&params);
      value_parser::walk(&nodes, &mut |node| {
        if node.kind != NodeKind::Function {
          return true;
        }
        let len = node.to_css().len();
        let open = node.source_index + node.value.len() + 1;
        let before = open..open + node.before.len();
        let close = node.source_index + len - usize::from(!node.unclosed);
        let after = close - node.after.len()..close;
        let has_blank = |text: &str| text.contains([' ', '\t']);
        let edit = |range: Range<usize>, with: &str| {
          fix_base
            .map(|base| vec![replace(base, range, with)])
            .unwrap_or_default()
        };
        let mut push = |message: &str, index: usize, edits: Vec<Edit>| {
          problems.push(rule.problem_fixed_by(
            tree.source(),
            message.into(),
            index_in(tree.source(), boost, &params, index),
            edits,
          ));
        };
        if always {
          if node.before.is_empty() {
            push(
              MESSAGES.expected_opening,
              node.source_index + 1,
              edit(before, " "),
            );
          }
          if node.after.is_empty() {
            push(
              MESSAGES.expected_closing,
              (node.source_index + len).saturating_sub(2),
              edit(after, " "),
            );
          }
        } else {
          if has_blank(node.before) {
            push(
              MESSAGES.rejected_opening,
              node.source_index + 1,
              edit(before, ""),
            );
          }
          if has_blank(node.after) {
            push(
              MESSAGES.rejected_closing,
              (node.source_index + len).saturating_sub(2),
              edit(after, ""),
            );
          }
        }
        true
      });
    }
    problems
  }
}

const ATTRIBUTE_BRACKETS_SPACE_INSIDE: &str = "@stylistic/selector-attribute-brackets-space-inside";

declare_rule! {
  /// Require a single space or disallow whitespace on the inside of the
  /// brackets of attribute selectors.
  ///
  /// Equivalent to `@stylistic/selector-attribute-brackets-space-inside`.
  pub struct StylisticSelectorAttributeBracketsSpaceInside => Spacing<StylisticSelectorAttributeBracketsSpaceInside> {
    name: ATTRIBUTE_BRACKETS_SPACE_INSIDE,
    description: "Require or disallow a space inside the brackets of attribute selectors",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorAttributeBracketsSpaceInside {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages::NONE;
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The character after every `[` and before every `]` of each attribute
  /// selector (strings aside): a space for `always`, anything else for
  /// `never`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    const MESSAGES: InsideMessages = InsideMessages {
      expected_opening: "Expected single space after \"[\"",
      rejected_opening: "Unexpected whitespace after \"[\"",
      expected_closing: "Expected single space before \"]\"",
      rejected_closing: "Unexpected whitespace before \"]\"",
    };
    let always = rule.expectation() == Expectation::Always;
    let with = if always { " " } else { "" };
    let mut problems = Vec::new();
    for node in tree.walk_rules() {
      if !node.is_standard_syntax() || !node.selector().contains('[') {
        continue;
      }
      let selector = node.raw_selector();
      let Some(selectors) = selector_parser::parse(&selector, 0) else {
        continue;
      };
      let fixable = node.selector_fixable();
      selector_parser::walk(&selectors, &mut |visit| {
        let attribute = visit.node;
        if attribute.kind != Kind::Attribute {
          return;
        }
        // Indexes count in `toString()`, from `sourceIndex`.
        let (start, text) = selector_node_string(&selector, visit);
        let base = node.selector_start() + start;
        let edit = |range: Range<usize>| {
          if fixable {
            vec![replace(base, range, with)]
          } else {
            vec![]
          }
        };
        let report = |index: usize| index_in(tree.source(), node.start, &selector, index);
        style_search(text, &SearchOptions::new("["), |m| {
          let space = text.as_bytes().get(m.start + 1) == Some(&b' ');
          let message = match (space, always) {
            (true, false) => MESSAGES.rejected_opening,
            (false, true) => MESSAGES.expected_opening,
            _ => return,
          };
          // `attributeNode.spaces.attribute.before`.
          let edits = edit(space_run_after(text, m.start + 1));
          problems.push(rule.problem_fixed_by(
            tree.source(),
            message.into(),
            report(attribute.start + m.start + 1),
            edits,
          ));
        });
        style_search(text, &SearchOptions::new("]"), |m| {
          let space = m.start > 0 && text.as_bytes()[m.start - 1] == b' ';
          let message = match (space, always) {
            (true, false) => MESSAGES.rejected_closing,
            (false, true) => MESSAGES.expected_closing,
            _ => return,
          };
          // The `after` of the attribute's last part.
          let edits = edit(space_run_before(text, m.start));
          let index = report((attribute.start + m.start).saturating_sub(1));
          problems.push(rule.problem_fixed_by(tree.source(), message.into(), index, edits));
        });
      });
    }
    problems
  }
}

const PSEUDO_CLASS_PARENTHESES_SPACE_INSIDE: &str =
  "@stylistic/selector-pseudo-class-parentheses-space-inside";

declare_rule! {
  /// Require a single space or disallow whitespace on the inside of the
  /// parentheses of pseudo-class selectors.
  ///
  /// Equivalent to `@stylistic/selector-pseudo-class-parentheses-space-inside`.
  pub struct StylisticSelectorPseudoClassParenthesesSpaceInside => Spacing<StylisticSelectorPseudoClassParenthesesSpaceInside> {
    name: PSEUDO_CLASS_PARENTHESES_SPACE_INSIDE,
    description: "Require or disallow a space inside the parentheses of pseudo-class selectors",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticSelectorPseudoClassParenthesesSpaceInside {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages::NONE;
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The argument of every pseudo with one: whether it starts and ends with
  /// a space.  `never` leaves a space before a `)` alone when the argument
  /// spans lines.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    const MESSAGES: InsideMessages = InsideMessages {
      expected_opening: "Expected single space after \"(\"",
      rejected_opening: "Unexpected whitespace after \"(\"",
      expected_closing: "Expected single space before \")\"",
      rejected_closing: "Unexpected whitespace before \")\"",
    };
    let always = rule.expectation() == Expectation::Always;
    let with = if always { " " } else { "" };
    let mut problems = Vec::new();
    for node in tree.walk_rules() {
      if !node.is_standard_syntax() || !node.selector().contains('(') {
        continue;
      }
      let selector = node.raw_selector();
      let Some(selectors) = selector_parser::parse(&selector, 0) else {
        continue;
      };
      let fixable = node.selector_fixable();
      selector_parser::walk(&selectors, &mut |visit| {
        let pseudo = visit.node;
        let (Kind::Pseudo, Some(args)) = (pseudo.kind, pseudo.args.as_ref()) else {
          return;
        };
        let inner = &selector[args.open + 1..args.close];
        let multi_line = inner.contains('\n');
        let open = pseudo.start + pseudo.value.len() + 1;
        let close = (open + inner.len()).saturating_sub(1);
        let edit = |range: Range<usize>| {
          if fixable {
            vec![replace(node.selector_start() + args.open + 1, range, with)]
          } else {
            vec![]
          }
        };
        let starts = inner.starts_with(' ');
        let ends = inner.ends_with(' ');
        let opening = match (starts, always) {
          (true, false) => Some(MESSAGES.rejected_opening),
          (false, true) => Some(MESSAGES.expected_opening),
          _ => None,
        };
        if let Some(message) = opening {
          // The first node's `spaces.before`.
          let edits = edit(space_run_after(inner, 0));
          problems.push(rule.problem_fixed_by(
            tree.source(),
            message.into(),
            index_in(tree.source(), node.start, &selector, open),
            edits,
          ));
        }
        let closing = match (ends, always) {
          (true, false) if !multi_line => Some(MESSAGES.rejected_closing),
          (false, true) => Some(MESSAGES.expected_closing),
          _ => None,
        };
        if let Some(message) = closing {
          // The last node's `spaces.after`.
          let edits = edit(space_run_before(inner, inner.len()));
          problems.push(rule.problem_fixed_by(
            tree.source(),
            message.into(),
            index_in(tree.source(), node.start, &selector, close),
            edits,
          ));
        }
      });
    }
    problems
  }
}

#[cfg(test)]
#[path = "cases/inside.rs"]
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
  fn function_parentheses() {
    let rule = "@stylistic/function-parentheses-space-inside";
    let source = "a { color: rgb( 0, 0, 0 ); top: calc(1px + 2px); left: f(); }";
    assert_eq!(
      problems(rule, "never", source),
      [
        (1, 16, "Unexpected whitespace after \"(\"".into()),
        (1, 24, "Unexpected whitespace before \")\"".into()),
      ]
    );
    assert_eq!(
      fix(rule, json!("always"), source, Syntax::Css),
      "a { color: rgb( 0, 0, 0 ); top: calc( 1px + 2px ); left: f(); }"
    );
    let newline = "@stylistic/function-parentheses-newline-inside";
    assert_eq!(
      fix(
        newline,
        json!("always-multi-line"),
        "a { b: f(1,\n  2); }",
        Syntax::Css
      ),
      "a { b: f(\n1,\n  2\n); }"
    );
  }

  #[test]
  fn media_feature_parentheses() {
    let rule = "@stylistic/media-feature-parentheses-space-inside";
    let source = "@media ( min-width: 1px) and (max-width: 2px ) {}";
    assert_eq!(
      fix(rule, json!("never"), source, Syntax::Css),
      "@media (min-width: 1px) and (max-width: 2px) {}"
    );
    assert_eq!(
      problems(rule, "always", source),
      [
        (1, 23, "Expected single space before \")\"".into()),
        (1, 31, "Expected single space after \"(\"".into()),
      ]
    );
  }

  #[test]
  fn selector_brackets_and_pseudo_parentheses() {
    let brackets = "@stylistic/selector-attribute-brackets-space-inside";
    assert_eq!(
      fix(brackets, json!("never"), "[ a=b ], [c] {}", Syntax::Css),
      "[a=b], [c] {}"
    );
    assert_eq!(
      fix(brackets, json!("always"), "[a=b] {}", Syntax::Css),
      "[ a=b ] {}"
    );
    let pseudo = "@stylistic/selector-pseudo-class-parentheses-space-inside";
    assert_eq!(
      problems(pseudo, "never", "a:not( b ), :is(\n  c ) {}"),
      [
        (1, 7, "Unexpected whitespace after \"(\"".into()),
        (1, 9, "Unexpected whitespace before \")\"".into()),
      ]
    );
    assert_eq!(
      fix(pseudo, json!("always"), "a:not(b) {}", Syntax::Css),
      "a:not( b ) {}"
    );
  }

  /// In a function without arguments the newline the plugin adds before
  /// the `)` goes to `before` and the problem stays: gale adds it once, as
  /// Stylelint's one run does.
  #[test]
  fn the_empty_function_fix_settles() {
    use crate::rules::spacing::harness::{bad, check};
    check(
      "@stylistic/function-parentheses-newline-inside",
      json!("always"),
      Syntax::Css,
      &[bad("a { width: calc(); }")
        .at(1, 16, "Expected newline before \")\"")
        .at(1, 17, "Expected newline after \"(\"")
        .fixed("a { width: calc(\n\n); }")],
    );
  }
}
