//! The colons of declarations, media features and Sass variables, found as
//! the plugin's `declarationColonSpaceChecker` and
//! `mediaFeatureColonSpaceChecker` and stylelint-scss's
//! `variableColonSpaceChecker` find them.

use std::ops::Range;

use gale_css_parser::postcss_tree::{DeclRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Edit};

use super::{
  Text, declaration_string, declaration_value_index, index_in, space_after, space_before,
};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::whitespace::{
  ALWAYS_NEVER, At, Expectation, Messages, SearchOptions, Spacing, SpacingRule, Target,
  is_horizontal_space, is_js_space, is_single_line, newline, replace, space_run_after,
  space_run_before, style_search,
};

/// A declaration's colon as the colon rules see it.
struct Colon<'a> {
  /// `propPlusColon`: the declaration's string up to a little past the
  /// colon, which is what the checker reads.
  text: &'a str,
  /// Where the colon is in `text`.
  index: usize,
  /// The declaration.
  decl: DeclRef<'a>,
}

impl Colon<'_> {
  /// `raws.between`: what separates the property from the value.
  fn between(&self) -> Range<usize> {
    self.decl.name_span.end..self.decl.value_start()
  }

  /// Where the colon is in the source.
  fn offset(&self) -> usize {
    self.decl.prop_start() + self.index
  }
}

/// The text the colon rules check: the declaration's string up to
/// `declarationValueIndex + between.length - 1`, which reaches past the
/// colon by the length of `raws.between`.
fn prop_plus_colon(decl: DeclRef, string: &str) -> String {
  let between = decl.value_start() - decl.name_span.end;
  let mut end = (declaration_value_index(decl) + between)
    .saturating_sub(1)
    .min(string.len());
  while !string.is_char_boundary(end) {
    end -= 1;
  }
  string[..end].to_string()
}

/// The plugin's `declarationColonSpaceChecker`: the first colon of every
/// standard declaration, in a text padded so that there is always
/// something after it.
fn declaration_colons(tree: &PostcssTree, mut each: impl FnMut(&Colon)) {
  for decl in tree.walk_decls() {
    if !decl.is_standard_syntax() {
      continue;
    }
    let string = declaration_string(decl);
    let mut text = prop_plus_colon(decl, &string);
    // PostCSS keeps a custom property's whitespace-only value in the value,
    // not in `raws.between`.
    if decl.is_custom_property() && !decl.value().is_empty() && decl.value().trim().is_empty() {
      text.push_str(decl.value());
      text.push('x');
    } else {
      text.push_str("xxx");
    }
    if let Some(index) = text.find(':') {
      each(&Colon {
        text: &text,
        index,
        decl,
      });
    }
  }
}

/// The colon fix of the `declaration-colon-space-*` rules: rewrite the
/// whitespace on one side of the colon within `raws.between`.
fn between_fix(colon: &Colon, after: bool, with: &str) -> Vec<Edit> {
  let source = colon.decl.tree().source();
  let between = colon.between();
  let at = colon.offset();
  if !between.contains(&at) {
    return vec![];
  }
  let custom_without_value =
    colon.decl.is_custom_property() && colon.decl.raw_value().trim_matches(is_js_space).is_empty();
  if !after {
    let run = space_run_before(source, at);
    return vec![replace(0, run.start.max(between.start)..run.end, with)];
  }
  // Whitespace that starts the value is the value's: the plugin's fix
  // puts a space in front of it and leaves the problem, every time.
  let value = colon.decl.raw_value();
  if !with.is_empty() && !custom_without_value && value.starts_with(is_js_space) {
    return vec![];
  }
  let mut run = space_run_after(&source[..between.end], at + 1);
  // https://github.com/stylelint-stylistic/stylelint-stylistic/issues/50:
  // a custom property's whitespace-only value goes with it, in the same
  // edit so that both land.
  if custom_without_value {
    run.end = colon.decl.value_span.end.max(run.end);
  }
  vec![replace(0, run, with)]
}

const DECLARATION_COLON_SPACE_AFTER: &str = "@stylistic/declaration-colon-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the colon of
  /// declarations.
  ///
  /// Equivalent to `@stylistic/declaration-colon-space-after`.
  pub struct StylisticDeclarationColonSpaceAfter => Spacing<StylisticDeclarationColonSpaceAfter> {
    name: DECLARATION_COLON_SPACE_AFTER,
    description: "Require or disallow a space after the colon in declarations",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationColonSpaceAfter {
  const OPTIONS: &'static [&'static str] = &["always", "never", "always-single-line"];
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \":\""),
    rejected_after: Some("Unexpected whitespace after \":\""),
    expected_after_single_line: Some(
      "Expected single space after \":\" with a single-line declaration",
    ),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    declaration_colons(tree, |colon| {
      let at = At::new(colon.text, colon.index).line_check(colon.decl.value());
      let Some(message) = rule.checker.after(at) else {
        return;
      };
      let with = match rule.expectation() {
        e if e.is_always() => " ",
        Expectation::Never => "",
        _ => return,
      };
      let prop = colon.decl.prop();
      let report = index_in(tree.source(), colon.decl.start, prop, prop.len() + 1);
      let edits = between_fix(colon, true, with);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    });
    problems
  }
}

const DECLARATION_COLON_SPACE_BEFORE: &str = "@stylistic/declaration-colon-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the colon of
  /// declarations.
  ///
  /// Equivalent to `@stylistic/declaration-colon-space-before`.
  pub struct StylisticDeclarationColonSpaceBefore => Spacing<StylisticDeclarationColonSpaceBefore> {
    name: DECLARATION_COLON_SPACE_BEFORE,
    description: "Require or disallow a space before the colon in declarations",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationColonSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \":\""),
    rejected_before: Some("Unexpected whitespace before \":\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    declaration_colons(tree, |colon| {
      let at = At::new(colon.text, colon.index).line_check(colon.decl.value());
      let Some(message) = rule.checker.before(at) else {
        return;
      };
      let with = if rule.expectation().is_always() {
        " "
      } else {
        ""
      };
      let prop = colon.decl.prop();
      let report = index_in(tree.source(), colon.decl.start, prop, prop.len() + 1);
      let edits = between_fix(colon, false, with);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    });
    problems
  }
}

const DECLARATION_COLON_NEWLINE_AFTER: &str = "@stylistic/declaration-colon-newline-after";

declare_rule! {
  /// Require a newline after the colon of declarations, or of declarations
  /// with a multi-line value.  A comment after the colon is allowed before
  /// the newline.
  ///
  /// Equivalent to `@stylistic/declaration-colon-newline-after`.
  pub struct StylisticDeclarationColonNewlineAfter => Spacing<StylisticDeclarationColonNewlineAfter> {
    name: DECLARATION_COLON_NEWLINE_AFTER,
    description: "Require a newline after the colon if the value is multi-line",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticDeclarationColonNewlineAfter {
  const OPTIONS: &'static [&'static str] = &["always", "always-multi-line"];
  const DEFAULT: &'static str = "always-multi-line";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \":\""),
    expected_after_multi_line: Some("Expected newline after \":\" with a multi-line declaration"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let source = tree.source();
    let mut problems = Vec::new();
    for decl in tree.walk_decls() {
      if !decl.is_standard_syntax() {
        continue;
      }
      let string = declaration_string(decl);
      let text = prop_plus_colon(decl, &string) + "xxx";
      // Unlike the space rules, every colon of the text is checked.
      for (i, _) in text.match_indices(':') {
        let rest = &text[i + 1..];
        let index = if rest
          .trim_start_matches(is_horizontal_space)
          .starts_with("/*")
        {
          text[i..].find("*/").map_or(0, |at| i + at + 1)
        } else {
          i
        };
        let at = At::new(&text, index).line_check(decl.value());
        let Some(message) = rule.checker.after_one_only(at) else {
          continue;
        };
        // Within `raws.between`: drop the spaces before a line break that
        // is there already, or add one.  A colon in the value, which the
        // text reaches once `raws.between` holds a line break, is reported
        // but not fixed: the plugin's fix would add a line break to
        // `raws.between` every time.
        let between = decl.name_span.end..decl.value_start();
        let split = decl.prop_start() + index + 1;
        // Whitespace that starts a custom property's value stays the
        // value's, and so does a line break added before it: the problem
        // stays.  Stylelint adds the line break once; gale adds it unless
        // the value starts with one already.
        let into_value = split == between.end
          && decl.raw_value().starts_with(is_js_space)
          && decl.raw_value().starts_with(['\n', '\r']);
        if !between.contains(&(decl.prop_start() + i)) || split > between.end || into_value {
          problems.push(rule.problem(source, message, index_in(source, decl.start, &text, index)));
          continue;
        }
        let after = &source[split..between.end];
        let run = space_run_after(after, 0);
        let edits = if after[run].contains('\n') {
          let horizontal = after.len() - after.trim_start_matches(is_horizontal_space).len();
          vec![replace(split, 0..horizontal, "")]
        } else {
          vec![replace(split, 0..0, newline(tree))]
        };
        let report = index_in(source, decl.start, &text, index);
        problems.push(rule.problem_fixed_by(source, message, report, edits));
      }
    }
    problems
  }
}

/// The plugin's `mediaFeatureColonSpaceChecker`: the colons of every
/// `@media` at-rule's params as written.
fn media_feature_colons(tree: &PostcssTree, mut each: impl FnMut(&Text, usize)) {
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
    style_search(&params, &SearchOptions::new(":"), |m| each(&text, m.start));
  }
}

const MEDIA_FEATURE_COLON_SPACE_AFTER: &str = "@stylistic/media-feature-colon-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the colon in media
  /// features.
  ///
  /// Equivalent to `@stylistic/media-feature-colon-space-after`.
  pub struct StylisticMediaFeatureColonSpaceAfter => Spacing<StylisticMediaFeatureColonSpaceAfter> {
    name: MEDIA_FEATURE_COLON_SPACE_AFTER,
    description: "Require or disallow a space after the colon in media features",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaFeatureColonSpaceAfter {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \":\""),
    rejected_after: Some("Unexpected whitespace after \":\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    media_feature_colons(tree, |text, index| {
      problems.extend(space_after(rule, tree.source(), text, index));
    });
    problems
  }
}

const MEDIA_FEATURE_COLON_SPACE_BEFORE: &str = "@stylistic/media-feature-colon-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the colon in media
  /// features.
  ///
  /// Equivalent to `@stylistic/media-feature-colon-space-before`.
  pub struct StylisticMediaFeatureColonSpaceBefore => Spacing<StylisticMediaFeatureColonSpaceBefore> {
    name: MEDIA_FEATURE_COLON_SPACE_BEFORE,
    description: "Require or disallow a space before the colon in media features",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticMediaFeatureColonSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \":\""),
    rejected_before: Some("Unexpected whitespace before \":\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    media_feature_colons(tree, |text, index| {
      problems.extend(space_before(rule, tree.source(), text, index));
    });
    problems
  }
}

/// stylelint-scss's `variableColonSpaceChecker`: the first colon of every
/// declaration of a `$variable`, in the declaration's string up to
/// `declarationValueIndex + between.length - 1`, with nothing added after
/// it.
fn variable_colons(tree: &PostcssTree, mut each: impl FnMut(&Colon)) {
  for decl in tree.walk_decls() {
    if !decl.prop().starts_with('$') {
      continue;
    }
    let string = declaration_string(decl);
    let text = prop_plus_colon(decl, &string);
    if let Some(index) = text.find(':') {
      each(&Colon {
        text: &text,
        index,
        decl,
      });
    }
  }
}

/// stylelint-scss's fix: the first match of `pattern` in `raws.between`
/// replaced, `/\s*:/` before the colon and `/:\s*/` (or `/:(?! )/` for
/// `at-least-one-space`) after it.
fn variable_colon_fix(colon: &Colon, expectation: Expectation, after: bool) -> Vec<Edit> {
  if expectation == Expectation::AlwaysSingleLine && !is_single_line(colon.decl.value()) {
    return vec![];
  }
  let source = colon.decl.tree().source();
  let between = colon.between();
  let Some(at) = source[between.clone()]
    .find(':')
    .map(|at| between.start + at)
  else {
    return vec![];
  };
  let never = expectation == Expectation::Never;
  if after {
    if expectation == Expectation::AtLeastOneSpace {
      // `/:(?! )/`: the first colon without a space after it.
      let text = &source[between.clone()];
      let Some(bare) = text
        .match_indices(':')
        .map(|(i, _)| between.start + i)
        .find(|&i| source.as_bytes().get(i + 1) != Some(&b' ') || i + 1 >= between.end)
      else {
        return vec![];
      };
      return vec![replace(0, bare..bare + 1, ": ")];
    }
    let run = space_run_after(&source[..between.end], at + 1);
    vec![replace(0, at..run.end, if never { ":" } else { ": " })]
  } else {
    let run = space_run_before(source, at);
    let run = run.start.max(between.start)..at + 1;
    vec![replace(0, run, if never { ":" } else { " :" })]
  }
}

const DOLLAR_VARIABLE_COLON_SPACE_AFTER: &str = "scss/dollar-variable-colon-space-after";

declare_rule! {
  /// Require a single space or disallow whitespace after the colon in
  /// `$variable` declarations.
  ///
  /// ```scss
  /// // Good (always)
  /// $var: value;
  /// // Bad (always)
  /// $var:value;
  /// $var:  value;
  /// ```
  ///
  /// Equivalent to `scss/dollar-variable-colon-space-after`.
  pub struct ScssDollarVariableColonSpaceAfter => Spacing<ScssDollarVariableColonSpaceAfter> {
    name: DOLLAR_VARIABLE_COLON_SPACE_AFTER,
    description: "Require or disallow a space after the colon in $variable declarations",
    default_severity: Warning,
  }
}

impl SpacingRule for ScssDollarVariableColonSpaceAfter {
  const OPTIONS: &'static [&'static str] = &[
    "always",
    "never",
    "always-single-line",
    "at-least-one-space",
  ];
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const SCSS: bool = true;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after \":\""),
    rejected_after: Some("Unexpected whitespace after \":\""),
    expected_after_single_line: Some("Expected single space after \":\" with a single-line value"),
    expected_after_at_least: Some("Expected at least one space after \":\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    variable_colons(tree, |colon| {
      let at = At::new(colon.text, colon.index).line_check(colon.decl.value());
      if let Some(message) = rule.checker.after(at) {
        let edits = variable_colon_fix(colon, rule.expectation(), true);
        let report = index_in(tree.source(), colon.decl.start, colon.text, colon.index);
        problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
      }
    });
    problems
  }
}

const DOLLAR_VARIABLE_COLON_SPACE_BEFORE: &str = "scss/dollar-variable-colon-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the colon in
  /// `$variable` declarations.
  ///
  /// ```scss
  /// // Good (never)
  /// $var: value;
  /// // Bad (never)
  /// $var : value;
  /// ```
  ///
  /// Equivalent to `scss/dollar-variable-colon-space-before`.
  pub struct ScssDollarVariableColonSpaceBefore => Spacing<ScssDollarVariableColonSpaceBefore> {
    name: DOLLAR_VARIABLE_COLON_SPACE_BEFORE,
    description: "Require or disallow a space before the colon in $variable declarations",
    default_severity: Warning,
  }
}

impl SpacingRule for ScssDollarVariableColonSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const SCSS: bool = true;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \":\""),
    rejected_before: Some("Unexpected whitespace before \":\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    variable_colons(tree, |colon| {
      let at = At::new(colon.text, colon.index).line_check(colon.decl.value());
      if let Some(message) = rule.checker.before(at) {
        let edits = variable_colon_fix(colon, rule.expectation(), false);
        let report = index_in(tree.source(), colon.decl.start, colon.text, colon.index);
        problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
      }
    });
    problems
  }
}

#[cfg(test)]
#[path = "cases/colon.rs"]
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
  fn declaration_colons_report_after_the_property() {
    let after = "@stylistic/declaration-colon-space-after";
    assert_eq!(
      problems(after, "always", "a { color:red; top :  0; }", Syntax::Css),
      [
        (1, 11, "Expected single space after \":\"".into()),
        (1, 20, "Expected single space after \":\"".into()),
      ]
    );
    assert_eq!(
      fix(
        after,
        json!("always"),
        "a { color:red; top :  0; }",
        Syntax::Css
      ),
      "a { color: red; top : 0; }"
    );
    assert_eq!(
      fix(after, json!("never"), "a { --x: ; }", Syntax::Css),
      "a { --x:; }"
    );
    let before = "@stylistic/declaration-colon-space-before";
    assert_eq!(
      fix(
        before,
        json!("always"),
        "a { color:red; top  : 0; }",
        Syntax::Css
      ),
      "a { color :red; top : 0; }"
    );
  }

  #[test]
  fn declaration_colon_newline_checks_multi_line_values() {
    let rule = "@stylistic/declaration-colon-newline-after";
    let source = "a { b: c,\n d; e:\n f; g: /* x */\n h,\n i; }";
    assert_eq!(
      problems(rule, "always-multi-line", source, Syntax::Css),
      [(
        1,
        6,
        "Expected newline after \":\" with a multi-line declaration".into()
      )]
    );
    assert_eq!(
      fix(rule, json!("always-multi-line"), source, Syntax::Css),
      "a { b:\n c,\n d; e:\n f; g: /* x */\n h,\n i; }"
    );
  }

  #[test]
  fn media_feature_colons_are_in_media_params() {
    let rule = "@stylistic/media-feature-colon-space-after";
    assert_eq!(
      fix(
        rule,
        json!("always"),
        "@media (min-width:1px) and (max-width:  2px) {}",
        Syntax::Css
      ),
      "@media (min-width: 1px) and (max-width: 2px) {}"
    );
    let before = "@stylistic/media-feature-colon-space-before";
    assert_eq!(
      problems(
        before,
        "never",
        "@media (a :1) {}\n@supports (a :1) {}",
        Syntax::Css
      ),
      [(1, 11, "Unexpected whitespace before \":\"".into())]
    );
  }

  #[test]
  fn variable_colons_follow_stylelint_scss() {
    let after = "scss/dollar-variable-colon-space-after";
    assert_eq!(
      problems(
        after,
        "always",
        "$a:1;\n$b:  2;\n$c: 3;\na { $d:4; }",
        Syntax::Scss
      ),
      [
        (1, 3, "Expected single space after \":\"".into()),
        (2, 3, "Expected single space after \":\"".into()),
        (4, 7, "Expected single space after \":\"".into()),
      ]
    );
    assert_eq!(
      fix(after, json!("always"), "$a:1;\n$b:  2;", Syntax::Scss),
      "$a: 1;\n$b: 2;"
    );
    assert_eq!(
      problems(after, "at-least-one-space", "$a:   1;", Syntax::Scss),
      []
    );
    let before = "scss/dollar-variable-colon-space-before";
    assert_eq!(
      fix(before, json!("never"), "$a : 1;\n$b\t:2;", Syntax::Scss),
      "$a: 1;\n$b:2;"
    );
  }

  /// Where the plugin's fix cannot settle, gale differs from one Stylelint
  /// run on purpose: Stylelint fixes once, gale fixes until nothing
  /// changes, so it leaves out a fix that would repeat on every pass.
  #[test]
  fn fixes_that_cannot_settle() {
    use crate::rules::spacing::harness::{bad, check};
    // The whitespace is the value's, so the space the plugin adds after
    // the colon leaves the problem; Stylelint makes it `margin:      ;`.
    check(
      "@stylistic/declaration-colon-space-after",
      json!("always"),
      Syntax::Css,
      &[bad("a { margin:     ; }")
        .at(1, 12, "Expected single space after \":\"")
        .unfixed()],
    );
    // A line break added before a custom property's whitespace-only value
    // is the value's too; gale adds it once, as Stylelint's one run does.
    check(
      "@stylistic/declaration-colon-newline-after",
      json!("always"),
      Syntax::Css,
      &[bad("a { --x: ; }")
        .at(1, 8, "Expected newline after \":\"")
        .fixed("a { --x:\n ; }")],
    );
  }
}
