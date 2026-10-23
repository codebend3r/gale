//! The semicolons that end the declarations of a block, as the plugin's
//! `declaration-block-semicolon-*` rules find them: after every
//! declaration in a rule or at-rule, except the last one when the block
//! has no trailing semicolon.

use std::borrow::Cow;

use gale_css_parser::postcss_tree::{DeclRef, NodeKind, NodeRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Edit};

use super::{block_string, from_end, index_in, node_string};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::whitespace::{
  At, Expectation, Messages, SPACE_SINGLE_LINE, Spacing, SpacingRule, Target, is_js_space, newline,
  replace,
};

/// A declaration followed by a semicolon of its block, with the block.
struct Semicolon<'t> {
  decl: DeclRef<'t>,
  /// The rule or at-rule whose block holds the declaration.
  parent: NodeRef<'t>,
}

/// The declarations of rules and at-rules that end with a semicolon of the
/// block: all but a last one the block leaves without (`raws.semicolon`).
fn semicolons<'t>(tree: &'t PostcssTree<'_>) -> impl Iterator<Item = Semicolon<'t>> {
  tree.walk_decls().filter_map(|decl| {
    let parent = decl.parent()?;
    if !matches!(parent.kind, NodeKind::Rule | NodeKind::AtRule) {
      return None;
    }
    let last = parent.children().next_back() == Some(decl.node());
    if last && !parent.semicolon {
      return None;
    }
    Some(Semicolon { decl, parent })
  })
}

/// `nextNonCommentNode`: the first node from `node` on that is not a
/// comment.
fn next_non_comment(node: Option<NodeRef>) -> Option<NodeRef> {
  std::iter::successors(node, |n| n.next()).find(|n| n.kind != NodeKind::Comment)
}

/// `rawNodeString`: `raws.before` and the node's string.
fn raw_node_string(node: NodeRef) -> String {
  format!("{}{}", node.before(), node_string(node))
}

/// What the `-before` rules' fixes rewrite: the end of `!important`
/// (`raws.important`, when it is not exactly ` !important`), or else the
/// end of the value, as the source has them.
struct Tail<'t> {
  decl: DeclRef<'t>,
}

impl Tail<'_> {
  /// The span and text of `!important` as written, if the declaration has
  /// one PostCSS keeps in `raws.important`.
  fn important(&self) -> Option<(std::ops::Range<usize>, &str)> {
    let span = self.decl.value_span.end..self.decl.semicolon().unwrap_or(self.decl.end);
    let text = &self.decl.tree().source()[span.clone()];
    (self.decl.important() && text != " !important").then_some((span, text))
  }

  /// Replace the whitespace that ends `text` at `span` with `with`
  /// (`.replace(/\s*$/, with)`).
  fn trim_end(span: std::ops::Range<usize>, text: &str, with: &str) -> Vec<Edit> {
    let kept = text.trim_end_matches(is_js_space).len();
    vec![replace(span.start, kept..text.len(), with)]
  }

  /// The value's end, when the value is the source as written and ends
  /// at the declaration's `;`.
  fn value(&self, with: &str) -> Vec<Edit> {
    // Without its own `;`, a declaration is followed by a comment that
    // takes the whitespace after the value: what the plugin's fix adds
    // there is the comment's too, and the problem stays.
    if self.decl.semicolon().is_none() {
      return vec![];
    }
    match self.decl.raw_value() {
      Cow::Borrowed(value) => Self::trim_end(self.decl.value_span.clone(), value, with),
      Cow::Owned(_) => vec![],
    }
  }
}

const SEMICOLON_SPACE_AFTER: &str = "@stylistic/declaration-block-semicolon-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the semicolons of
  /// declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-space-after`.
  pub struct StylisticDeclarationBlockSemicolonSpaceAfter => Spacing<StylisticDeclarationBlockSemicolonSpaceAfter> {
    name: SEMICOLON_SPACE_AFTER,
    description: "Require or disallow a space after the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationBlockSemicolonSpaceAfter {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "always-single-line";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \";\""),
    rejected_after: Some("Unexpected whitespace after \";\""),
    expected_after_single_line: Some(
      "Expected single space after \";\" in a single-line declaration block",
    ),
    rejected_after_single_line: Some(
      "Unexpected whitespace after \";\" in a single-line declaration block",
    ),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks what starts the next node (`raws.before`, or the
  /// node itself) and reports just past the `;`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for Semicolon { decl, parent } in semicolons(tree) {
      let Some(next) = decl.next() else {
        continue;
      };
      let text = raw_node_string(next);
      let block = block_string(parent);
      let at = At::start_of(&text).line_check(&block);
      let Some(message) = rule.checker.after(at) else {
        continue;
      };
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let edits = vec![replace(0, next.before.clone(), with)];
      let string = node_string(decl.node());
      let report = index_in(tree.source(), decl.start, &string, string.len() + 1);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

const SEMICOLON_SPACE_BEFORE: &str = "@stylistic/declaration-block-semicolon-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the semicolons of
  /// declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-space-before`.
  pub struct StylisticDeclarationBlockSemicolonSpaceBefore => Spacing<StylisticDeclarationBlockSemicolonSpaceBefore> {
    name: SEMICOLON_SPACE_BEFORE,
    description: "Require or disallow a space before the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationBlockSemicolonSpaceBefore {
  const OPTIONS: &'static [&'static str] = SPACE_SINGLE_LINE;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \";\""),
    rejected_before: Some("Unexpected whitespace before \";\""),
    expected_before_single_line: Some(
      "Expected single space before \";\" in a single-line declaration block",
    ),
    rejected_before_single_line: Some(
      "Unexpected whitespace before \";\" in a single-line declaration block",
    ),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks the end of the declaration's string, where the `;`
  /// follows, and reports at its last character.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for Semicolon { decl, parent } in semicolons(tree) {
      let value = decl.raw_value();
      let only_spaces = decl.is_custom_property() && is_whitespace_only(&value);
      // https://github.com/stylelint-stylistic/stylelint-stylistic/issues/50
      if decl.is_custom_property() && rule.expectation().is_never() && value == " " {
        continue;
      }
      let text = node_string(decl.node());
      let block = block_string(parent);
      let at = At::new(&text, text.len()).line_check(&block);
      let Some(message) = rule.checker.before(at) else {
        continue;
      };
      let tail = Tail { decl };
      let edits = if rule.expectation().is_always() {
        if decl.important() {
          let span = decl.value_span.end..decl.semicolon().unwrap_or(decl.end);
          vec![replace(0, span, " !important ")]
        } else {
          tail.value(" ")
        }
      } else if let Some((span, text)) = tail.important() {
        Tail::trim_end(span, text, "")
      } else if only_spaces {
        vec![replace(0, decl.value_span.clone(), " ")]
      } else {
        tail.value("")
      };
      let report = from_end(tree.source(), decl.start, &text, 1);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

/// `/^\s+$/`.
fn is_whitespace_only(text: &str) -> bool {
  !text.is_empty() && text.chars().all(is_js_space)
}

const SEMICOLON_NEWLINE_AFTER: &str = "@stylistic/declaration-block-semicolon-newline-after";

declare_rule! {
  /// Require a newline or disallow whitespace after the semicolons of
  /// declaration blocks.  A comment after the semicolon is allowed before
  /// the newline.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-newline-after`.
  pub struct StylisticDeclarationBlockSemicolonNewlineAfter => Spacing<StylisticDeclarationBlockSemicolonNewlineAfter> {
    name: SEMICOLON_NEWLINE_AFTER,
    description: "Require a newline after the semicolon of declaration blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationBlockSemicolonNewlineAfter {
  const OPTIONS: &'static [&'static str] = &["always", "always-multi-line", "never-multi-line"];
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \";\""),
    expected_after_multi_line: Some(
      "Expected newline after \";\" in a multi-line declaration block",
    ),
    rejected_after_multi_line: Some(
      "Unexpected newline after \";\" in a multi-line declaration block",
    ),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks what starts the next node that is not a comment and
  /// reports just past the `;`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for Semicolon { decl, parent } in semicolons(tree) {
      let Some(next) = next_non_comment(decl.next()) else {
        continue;
      };
      let text = raw_node_string(next);
      let block = block_string(parent);
      let at = At::start_of(&text).line_check(&block);
      let Some(message) = rule.checker.after_one_only(at) else {
        continue;
      };
      let before = next.before();
      let edits = match rule.expectation() {
        e if e.is_always() => match before.find('\n') {
          // `before.slice(before.search(/\r?\n/))`
          Some(at) => {
            let at = if at > 0 && before.as_bytes()[at - 1] == b'\r' {
              at - 1
            } else {
              at
            };
            vec![replace(next.before.start, 0..at, "")]
          }
          None => vec![replace(next.before.start, 0..0, newline(tree))],
        },
        Expectation::NeverMultiLine => vec![replace(0, next.before.clone(), "")],
        _ => vec![],
      };
      let string = node_string(decl.node());
      let report = index_in(tree.source(), decl.start, &string, string.len() + 1);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

const SEMICOLON_NEWLINE_BEFORE: &str = "@stylistic/declaration-block-semicolon-newline-before";

declare_rule! {
  /// Require a newline or disallow whitespace before the semicolons of
  /// declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-newline-before`;
  /// gale also takes `"never"`, no line break before the semicolon.
  pub struct StylisticDeclarationBlockSemicolonNewlineBefore => Spacing<StylisticDeclarationBlockSemicolonNewlineBefore> {
    name: SEMICOLON_NEWLINE_BEFORE,
    description: "Require a newline or disallow whitespace before the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationBlockSemicolonNewlineBefore {
  const OPTIONS: &'static [&'static str] =
    &["always", "always-multi-line", "never-multi-line", "never"];
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected newline before \";\""),
    rejected_before: Some("Unexpected whitespace before \";\""),
    expected_before_multi_line: Some(
      "Expected newline before \";\" in a multi-line declaration block",
    ),
    rejected_before_multi_line: Some(
      "Unexpected whitespace before \";\" in a multi-line declaration block",
    ),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks the end of the declaration's string, allowing
  /// indentation after the newline, and reports at its last character.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for Semicolon { decl, parent } in semicolons(tree) {
      let value = decl.raw_value();
      let horizontal_only = decl.is_custom_property()
        && !value.is_empty()
        && value.bytes().all(|b| matches!(b, b' ' | b'\t'));
      // https://github.com/stylelint-stylistic/stylelint-stylistic/issues/50
      if rule.expectation().is_never() && value == " " {
        continue;
      }
      let text = node_string(decl.node());
      let block = block_string(parent);
      let at = At::new(&text, text.len()).line_check(&block);
      let Some(message) = rule.checker.before_allowing_indentation(at) else {
        continue;
      };
      let tail = Tail { decl };
      let edits = match rule.expectation() {
        e if e.is_always() => match tail.important() {
          Some((span, text)) => Tail::trim_end(span, text, newline(tree)),
          None => tail.value(newline(tree)),
        },
        Expectation::NeverMultiLine => match tail.important() {
          Some((span, text)) => Tail::trim_end(span, text, ""),
          None if horizontal_only => vec![replace(0, decl.value_span.clone(), " ")],
          None => tail.value(""),
        },
        _ => vec![],
      };
      let report = from_end(tree.source(), decl.start, &text, 1);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

#[cfg(test)]
#[path = "cases/semicolon.rs"]
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
  fn space_after_looks_at_the_next_node() {
    let rule = "@stylistic/declaration-block-semicolon-space-after";
    assert_eq!(
      problems(rule, "always", "a { color: red;top: 0;  left: 0; }"),
      [
        (1, 16, "Expected single space after \";\"".into()),
        (1, 23, "Expected single space after \";\"".into()),
      ]
    );
    assert_eq!(
      fix(
        rule,
        json!("always"),
        "a { color: red;top: 0;  left: 0; }",
        Syntax::Css
      ),
      "a { color: red; top: 0; left: 0; }"
    );
    assert_eq!(
      problems(rule, "always-single-line", "a {\n  color: red;top: 0;\n}"),
      [],
      "a multi-line block"
    );
  }

  #[test]
  fn space_before_fixes_the_value_or_important() {
    let rule = "@stylistic/declaration-block-semicolon-space-before";
    assert_eq!(
      fix(
        rule,
        json!("never"),
        "a { color: red ; top: 0 !important ; }",
        Syntax::Css
      ),
      "a { color: red; top: 0 !important; }"
    );
    assert_eq!(
      fix(
        rule,
        json!("always"),
        "a { color: red; top: 0 !important; }",
        Syntax::Css
      ),
      "a { color: red ; top: 0 !important ; }"
    );
  }

  #[test]
  fn newline_after_allows_comments() {
    let rule = "@stylistic/declaration-block-semicolon-newline-after";
    let source = "a { color: red; /* c */\n top: 0; left: 0; }";
    assert_eq!(
      problems(rule, "always", source),
      [(2, 9, "Expected newline after \";\"".into())]
    );
    assert_eq!(
      fix(rule, json!("always"), source, Syntax::Css),
      "a { color: red; /* c */\n top: 0;\n left: 0; }"
    );
  }

  #[test]
  fn newline_before_allows_indentation() {
    let rule = "@stylistic/declaration-block-semicolon-newline-before";
    assert_eq!(problems(rule, "always", "a { color: red\n  ; }"), []);
    assert_eq!(
      fix(rule, json!("always"), "a { color: red ; }", Syntax::Css),
      "a { color: red\n; }"
    );
    assert_eq!(
      problems(rule, "never", "a { color: red\n  ; top: 0 ; }"),
      [(2, 2, "Unexpected whitespace before \";\"".into())]
    );
  }

  /// A declaration without its own `;` before a comment: the whitespace
  /// after the value is the comment's, so the space the plugin adds stays
  /// a problem (Stylelint makes it `0  /* c */`).  Gale leaves it out.
  #[test]
  fn fixes_that_cannot_settle() {
    use crate::rules::spacing::harness::{bad, check};
    let message = "Expected single space before \";\"";
    check(
      "@stylistic/declaration-block-semicolon-space-before",
      json!("always"),
      Syntax::Css,
      &[bad("a { top: 0; left: 0 /* c */\n}")
        .at(1, 10, message)
        .at(1, 19, message)
        .fixed("a { top: 0 ; left: 0 /* c */\n}")],
    );
  }
}
