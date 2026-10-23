//! The `!` of declarations (`!important`, `!default`), found as the
//! plugin's `declarationBangSpaceChecker` finds it.

use std::borrow::Cow;

use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Edit};

use super::{declaration_string, declaration_value_index, index_in};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::whitespace::{
  ALWAYS_NEVER, At, Messages, SearchOptions, Spacing, SpacingRule, Target, is_js_space, replace,
  style_search,
};

/// The plugin's `declarationBangSpaceChecker`: the `!`s after the start of
/// a declaration's value, outside comments, strings and function names,
/// as indexes into the declaration's string, for each declaration with one.
fn bangs(tree: &PostcssTree, mut each: impl FnMut(DeclRef, &str, &[usize])) {
  for decl in tree.walk_decls() {
    let string = declaration_string(decl);
    let offset = declaration_value_index(decl);
    let Some(value) = string.get(offset..) else {
      continue;
    };
    if !value.contains('!') {
      continue;
    }
    let mut found = Vec::new();
    style_search(value, &SearchOptions::new("!"), |m| {
      found.push(m.start + offset)
    });
    each(decl, &string, &found);
  }
}

/// Which side of the `!` a fix rewrites, and with what.
#[derive(Clone, Copy)]
enum Side {
  After(&'static str),
  Before(&'static str),
}

/// The fixes of one declaration's problems, as the plugin applies them: one
/// after the other, each at the index it was reported at, in the value as
/// the fixes before it left it (so a fix after the first can land a
/// character off), or in `raws.important` past the value's end.  The
/// result replaces the value and `!important` as written.
fn fix_declaration(decl: DeclRef, indexes: &[usize], side: Side) -> Vec<Edit> {
  let source = decl.tree().source();
  let value_index = declaration_value_index(decl);
  let raw_value = decl.raw_value();
  let fixable_value = matches!(raw_value, Cow::Borrowed(_));
  let mut value = raw_value.into_owned();
  let important_span = decl.value_span.end..decl.semicolon().unwrap_or(decl.end);
  let mut important = source[important_span.clone()].to_string();
  let (original_value, original_important) = (value.clone(), important.clone());
  for &index in indexes {
    let mut bang = index - value_index;
    let target = if bang < value.len() {
      &mut value
    } else if decl.important() {
      bang -= value.len();
      &mut important
    } else {
      continue;
    };
    let split = match side {
      Side::After(_) => bang + 1,
      Side::Before(_) => bang,
    };
    if !target.is_char_boundary(split) {
      continue;
    }
    let (head, tail) = target.split_at(split);
    let fixed = match side {
      Side::After(with) => format!("{head}{with}{}", tail.trim_start_matches(is_js_space)),
      Side::Before(with) => format!("{}{with}{tail}", head.trim_end_matches(is_js_space)),
    };
    *target = fixed;
  }
  let mut edits = Vec::new();
  if value != original_value && fixable_value {
    edits.push(replace(0, decl.value_span.clone(), value));
  }
  if important != original_important {
    edits.push(replace(0, important_span, important));
  }
  edits
}

/// Check every `!` with `check` and report each problem at its index, the
/// first problem of a declaration carrying the fixes of all of them.
fn check_bangs<R: SpacingRule>(
  rule: &Spacing<R>,
  tree: &PostcssTree,
  check: impl Fn(At) -> Option<String>,
  side: Side,
) -> Vec<Diagnostic> {
  let mut problems = Vec::new();
  bangs(tree, |decl, string, found| {
    let reported: Vec<(usize, String)> = found
      .iter()
      .filter_map(|&index| Some((index, check(At::new(string, index))?)))
      .collect();
    let indexes: Vec<usize> = reported.iter().map(|(index, _)| *index).collect();
    let mut edits = fix_declaration(decl, &indexes, side);
    for (index, message) in reported {
      let report = index_in(tree.source(), decl.start, string, index);
      problems.push(rule.problem_fixed_by(
        tree.source(),
        message,
        report,
        std::mem::take(&mut edits),
      ));
    }
  });
  problems
}

const DECLARATION_BANG_SPACE_AFTER: &str = "@stylistic/declaration-bang-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the bang (`!`) of
  /// declarations.
  ///
  /// Equivalent to `@stylistic/declaration-bang-space-after`.
  pub struct StylisticDeclarationBangSpaceAfter => Spacing<StylisticDeclarationBangSpaceAfter> {
    name: DECLARATION_BANG_SPACE_AFTER,
    description: "Require or disallow a space after the bang of declarations",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationBangSpaceAfter {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \"!\""),
    rejected_after: Some("Unexpected whitespace after \"!\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let with = if rule.expectation().is_always() {
      " "
    } else {
      ""
    };
    check_bangs(rule, tree, |at| rule.checker.after(at), Side::After(with))
  }
}

const DECLARATION_BANG_SPACE_BEFORE: &str = "@stylistic/declaration-bang-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the bang (`!`) of
  /// declarations.
  ///
  /// Equivalent to `@stylistic/declaration-bang-space-before`.
  pub struct StylisticDeclarationBangSpaceBefore => Spacing<StylisticDeclarationBangSpaceBefore> {
    name: DECLARATION_BANG_SPACE_BEFORE,
    description: "Require or disallow a space before the bang of declarations",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationBangSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \"!\""),
    rejected_before: Some("Unexpected whitespace before \"!\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let with = if rule.expectation().is_always() {
      " "
    } else {
      ""
    };
    check_bangs(rule, tree, |at| rule.checker.before(at), Side::Before(with))
  }
}

#[cfg(test)]
#[path = "cases/bang.rs"]
mod cases;

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  /// `(line, column, message)` of each problem.
  fn problems(
    rule: &str,
    option: &str,
    source: &str,
    syntax: Syntax,
  ) -> Vec<(usize, usize, String)> {
    let index = gale_diagnostics::SourceLineIndex::build(source);
    let mut found: Vec<_> = lint(rule, json!(option), source, syntax)
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
  fn bangs_in_values_and_important_are_checked() {
    let after = "@stylistic/declaration-bang-space-after";
    assert_eq!(
      problems(
        after,
        "never",
        "a { color: red ! important; }\n$a: 1 ! default;",
        Syntax::Scss
      ),
      [
        (1, 16, "Unexpected whitespace after \"!\"".into()),
        (2, 7, "Unexpected whitespace after \"!\"".into()),
      ]
    );
    assert_eq!(
      fix(
        after,
        json!("never"),
        "a { color: red !  important; }",
        Syntax::Css
      ),
      "a { color: red !important; }"
    );
    // The plugin fixes the second `!` at the index it was reported at, one
    // character off once the first fix added a space; the next pass fixes
    // it, as a second Stylelint run would.
    assert_eq!(
      fix(
        after,
        json!("always"),
        "$a: b !default !global;",
        Syntax::Scss
      ),
      "$a: b ! default  ! global;"
    );
    let before = "@stylistic/declaration-bang-space-before";
    assert_eq!(
      fix(
        before,
        json!("always"),
        "a { color: red!important; content: 'a!'; }",
        Syntax::Css
      ),
      "a { color: red !important; content: 'a!'; }"
    );
    assert_eq!(
      fix(before, json!("never"), "$a: 1  !default;", Syntax::Scss),
      "$a: 1!default;"
    );
  }
}
