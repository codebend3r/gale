//! The whitespace around an at-rule's name and its `;`, as the plugin's
//! `atRuleNameSpaceChecker`, `at-rule-semicolon-space-before` and
//! `at-rule-semicolon-newline-after` check it, and before the parentheses
//! of Sass's `@else if`, `@function` and `@mixin`, as stylelint-scss's
//! `at-*-parentheses-space-before` rules do.

use gale_css_parser::postcss_tree::{AtRuleRef, NodeKind, PostcssTree};
use gale_diagnostics::Diagnostic;

use super::{from_end, index_in, is_standard_at_rule, node_string};
use crate::options::Options;
use crate::rule::{RuleContext, declare_rule};
use crate::whitespace::{
  ALWAYS_NEVER, At, Messages, Spacing, SpacingRule, Target, is_js_space, newline, replace,
  space_run_after,
};

const AT_RULE_NAME_SPACE_AFTER: &str = "@stylistic/at-rule-name-space-after";

declare_rule! {
  /// Require a single space after at-rule names.
  ///
  /// Equivalent to `@stylistic/at-rule-name-space-after`.
  pub struct StylisticAtRuleNameSpaceAfter => Spacing<StylisticAtRuleNameSpaceAfter> {
    name: AT_RULE_NAME_SPACE_AFTER,
    description: "Require a space after at-rule names",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticAtRuleNameSpaceAfter {
  const OPTIONS: &'static [&'static str] = &["always", "always-single-line"];
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected single space after at-rule name \"%s\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin's `atRuleNameSpaceChecker`: the character after the name
  /// in `@name` + `raws.afterName` + `params`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for at in tree.walk_at_rules() {
      if !is_standard_at_rule(at) {
        continue;
      }
      let name = format!("@{}", at.name());
      let text = format!("{name}{}{}", at.after_name, at.params());
      let check = At::new(&text, at.name().len()).err_target(&name);
      let Some(message) = rule.checker.after(check) else {
        continue;
      };
      // `raws.afterName.replace(/^\s*/, ' ')`.
      let after_name = at.name_start() + at.name().len();
      let run = space_run_after(&at.after_name, 0);
      let edits = vec![replace(after_name, run, " ")];
      let report = index_in(tree.source(), at.start, at.name(), at.name().len());
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

/// The at-rules without a block that the plugin's semicolon rules check.
fn blockless_at_rules<'t>(tree: &'t PostcssTree) -> impl Iterator<Item = AtRuleRef<'t>> {
  tree
    .walk_at_rules()
    .filter(|at| !at.has_block() && is_standard_at_rule(*at))
}

const AT_RULE_SEMICOLON_SPACE_BEFORE: &str = "@stylistic/at-rule-semicolon-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the semicolons of
  /// at-rules.
  ///
  /// Equivalent to `@stylistic/at-rule-semicolon-space-before`, which does
  /// not fix.
  pub struct StylisticAtRuleSemicolonSpaceBefore => Spacing<StylisticAtRuleSemicolonSpaceBefore> {
    name: AT_RULE_SEMICOLON_SPACE_BEFORE,
    description: "Require or disallow a space before at-rule semicolons",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticAtRuleSemicolonSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const MESSAGES: &'static Messages = &Messages {
    expected_before: Some("Expected single space before \";\""),
    rejected_before: Some("Unexpected whitespace before \";\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks the end of `raws.before` + `toString()`, where the
  /// `;` would follow, and reports that many characters from the at-rule's
  /// start: past the `;` when `raws.before` is longer than one character.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for at in blockless_at_rules(tree) {
      let text = format!("{}{}", at.before(), node_string(at.node()));
      if let Some(message) = rule.checker.before(At::new(&text, text.len())) {
        let report = from_end(tree.source(), at.start, &text, 1);
        problems.push(rule.problem(tree.source(), message, report));
      }
    }
    problems
  }
}

const AT_RULE_SEMICOLON_NEWLINE_AFTER: &str = "@stylistic/at-rule-semicolon-newline-after";

declare_rule! {
  /// Require a newline after the semicolons of at-rules.  A comment after
  /// the semicolon is allowed before the newline.
  ///
  /// Equivalent to `@stylistic/at-rule-semicolon-newline-after`.
  pub struct StylisticAtRuleSemicolonNewlineAfter => Spacing<StylisticAtRuleSemicolonNewlineAfter> {
    name: AT_RULE_SEMICOLON_NEWLINE_AFTER,
    description: "Require a newline after the semicolon of at-rules",
    default_severity: Warning,
  }
}

impl SpacingRule for StylisticAtRuleSemicolonNewlineAfter {
  const OPTIONS: &'static [&'static str] = &["always"];
  const DEFAULT: &'static str = "always";
  const TARGET: Target = Target::Newline;
  const MESSAGES: &'static Messages = &Messages {
    expected_after: Some("Expected newline after \";\""),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  /// The plugin checks what starts the next node that is not a comment
  /// (`raws.before`, or the node itself), and reports just past the `;`.
  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    for at in blockless_at_rules(tree) {
      let Some(next) = at.node().next().and_then(|next| {
        std::iter::successors(Some(next), |n| n.next()).find(|n| n.kind != NodeKind::Comment)
      }) else {
        continue;
      };
      let text = format!("{}{}", next.before(), next.source_text());
      let Some(message) = rule.checker.after_one_only(At::start_of(&text)) else {
        continue;
      };
      let edits = vec![replace(next.before.start, 0..0, newline(tree))];
      let string = node_string(at.node());
      let report = index_in(tree.source(), at.start, &string, string.len() + 1);
      problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
    }
    problems
  }
}

/// What a stylelint-scss `at-*-parentheses-space-before` rule checks: the
/// `(` in the params of the at-rules named `name`, whose params match
/// `head` (the start of the params up to the `(`, as a regex would).
fn parentheses_before<R: SpacingRule>(
  rule: &Spacing<R>,
  tree: &PostcssTree,
  name: &str,
  head: fn(&str) -> Option<usize>,
  needs_head: bool,
) -> Vec<Diagnostic> {
  let mut problems = Vec::new();
  for at in tree.walk_at_rules() {
    if at.name() != name {
      continue;
    }
    let params = at.params();
    let matched = head(params);
    if needs_head && matched.is_none() {
      continue;
    }
    // `params.indexOf('(')`, which is -1 without one: then nothing is
    // checked.
    let check = match params.find('(') {
      Some(index) => At::new(params, index),
      None => At::start_of(params),
    };
    let Some(message) = rule.checker.before(check) else {
      continue;
    };
    let index = params.find('(').unwrap_or(0);
    // The fix rewrites the matched start of the params, `name\s*(`, with
    // one space or none before the `(`.
    let edits = match (matched, at.raw_params() == params) {
      (Some(open), true) => {
        let word = params[..open].trim_end_matches(is_js_space).len();
        let with = if rule.expectation().is_always() {
          " "
        } else {
          ""
        };
        vec![replace(at.params_start(), word..open, with)]
      }
      _ => vec![],
    };
    let report = index_in(tree.source(), at.start + at.param_index(), params, index);
    problems.push(rule.problem_fixed_by(tree.source(), message, report, edits));
  }
  problems
}

/// `/^if\s*\(/`: the offset of the `(`.
fn if_head(params: &str) -> Option<usize> {
  let rest = params.strip_prefix("if")?;
  let after = rest.trim_start_matches(is_js_space);
  after.starts_with('(').then(|| params.len() - after.len())
}

/// `/^([\w-]+)\s*\(/`: the offset of the `(`.
fn name_head(params: &str) -> Option<usize> {
  let word = params
    .bytes()
    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    .count();
  if word == 0 {
    return None;
  }
  let after = params[word..].trim_start_matches(is_js_space);
  after.starts_with('(').then(|| params.len() - after.len())
}

const AT_ELSE_IF_PARENTHESES_SPACE_BEFORE: &str = "scss/at-else-if-parentheses-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the parentheses
  /// of `@else if`.
  ///
  /// ```scss
  /// // Good (never)
  /// @else if($condition) {}
  /// // Good (always)
  /// @else if ($condition) {}
  /// ```
  ///
  /// This is distinct from `scss/at-rule-conditional-no-parentheses`, which
  /// disallows the parentheses.  Equivalent to
  /// `scss/at-else-if-parentheses-space-before`.
  pub struct ScssAtElseIfParenthesesSpaceBefore => Spacing<ScssAtElseIfParenthesesSpaceBefore> {
    name: AT_ELSE_IF_PARENTHESES_SPACE_BEFORE,
    description: "Require or disallow a space before @else if parentheses",
    default_severity: Warning,
  }
}

impl SpacingRule for ScssAtElseIfParenthesesSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const SCSS: bool = true;
  const MESSAGES: &'static Messages = &Messages {
    rejected_before: Some("Unexpected whitespace before parentheses in else-if declaration"),
    expected_before: Some("Expected a single space before parentheses in else-if declaration"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    parentheses_before(rule, tree, "else", if_head, true)
  }
}

const AT_FUNCTION_PARENTHESES_SPACE_BEFORE: &str = "scss/at-function-parentheses-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the parentheses
  /// of `@function`.
  ///
  /// ```scss
  /// // Good (never)
  /// @function foo($x) { @return $x; }
  /// // Good (always)
  /// @function foo ($x) { @return $x; }
  /// ```
  ///
  /// Equivalent to `scss/at-function-parentheses-space-before`.
  pub struct ScssAtFunctionParenthesesSpaceBefore => Spacing<ScssAtFunctionParenthesesSpaceBefore> {
    name: AT_FUNCTION_PARENTHESES_SPACE_BEFORE,
    description: "Require or disallow a space before @function parentheses",
    default_severity: Warning,
  }
}

impl SpacingRule for ScssAtFunctionParenthesesSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const SCSS: bool = true;
  const MESSAGES: &'static Messages = &Messages {
    rejected_before: Some("Unexpected whitespace before parentheses in function declaration"),
    expected_before: Some("Expected a single space before parentheses in function declaration"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    parentheses_before(rule, tree, "function", name_head, false)
  }
}

const AT_MIXIN_PARENTHESES_SPACE_BEFORE: &str = "scss/at-mixin-parentheses-space-before";

declare_rule! {
  /// Require a single space or disallow whitespace before the parentheses
  /// of `@mixin`.
  ///
  /// ```scss
  /// // Good (never)
  /// @mixin foo($x) {}
  /// // Good (always)
  /// @mixin foo ($x) {}
  /// ```
  ///
  /// Equivalent to `scss/at-mixin-parentheses-space-before`.
  pub struct ScssAtMixinParenthesesSpaceBefore => Spacing<ScssAtMixinParenthesesSpaceBefore> {
    name: AT_MIXIN_PARENTHESES_SPACE_BEFORE,
    description: "Require or disallow a space before @mixin parentheses",
    default_severity: Warning,
  }
}

impl SpacingRule for ScssAtMixinParenthesesSpaceBefore {
  const OPTIONS: &'static [&'static str] = ALWAYS_NEVER;
  const DEFAULT: &'static str = "never";
  const TARGET: Target = Target::Space;
  const SCSS: bool = true;
  const MESSAGES: &'static Messages = &Messages {
    rejected_before: Some("Unexpected whitespace before parentheses in mixin declaration"),
    expected_before: Some("Expected a single space before parentheses in mixin declaration"),
    ..Messages::NONE
  };
  type Secondary = ();

  fn secondary(_: &mut Options) {}

  fn check(rule: &Spacing<Self>, tree: &PostcssTree, _: &RuleContext) -> Vec<Diagnostic> {
    parentheses_before(rule, tree, "mixin", name_head, false)
  }
}

#[cfg(test)]
#[path = "cases/at_rule.rs"]
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
  fn at_rule_names_want_one_space() {
    let rule = "@stylistic/at-rule-name-space-after";
    let source = "@media(min-width: 1px){}\n@import  \"a\";\n@charset\"x\";\n@include\nfoo;";
    assert_eq!(
      problems(rule, "always", source, Syntax::Css),
      [
        (
          1,
          6,
          "Expected single space after at-rule name \"@media\"".into()
        ),
        (
          2,
          7,
          "Expected single space after at-rule name \"@import\"".into()
        ),
        (
          3,
          8,
          "Expected single space after at-rule name \"@charset\"".into()
        ),
        (
          4,
          8,
          "Expected single space after at-rule name \"@include\"".into()
        ),
      ]
    );
    assert_eq!(
      fix(rule, json!("always"), source, Syntax::Css),
      "@media (min-width: 1px){}\n@import \"a\";\n@charset \"x\";\n@include foo;"
    );
  }

  #[test]
  fn at_rule_semicolons() {
    let before = "@stylistic/at-rule-semicolon-space-before";
    assert_eq!(
      problems(
        before,
        "never",
        "@import \"a\" ;\n  @import \"b\" ;",
        Syntax::Css
      ),
      [
        (1, 12, "Unexpected whitespace before \";\"".into()),
        // Counted from the at-rule's start past its `raws.before`.
        (2, 17, "Unexpected whitespace before \";\"".into()),
      ]
    );
    let after = "@stylistic/at-rule-semicolon-newline-after";
    let source = "@import \"a\"; @import \"b\"; /* c */\na {}";
    assert_eq!(
      problems(after, "always", source, Syntax::Css),
      [(1, 13, "Expected newline after \";\"".into())]
    );
    assert_eq!(
      fix(after, json!("always"), source, Syntax::Css),
      "@import \"a\";\n @import \"b\"; /* c */\na {}"
    );
  }

  #[test]
  fn sass_parentheses_follow_stylelint_scss() {
    let mixin = "scss/at-mixin-parentheses-space-before";
    let source = "@mixin a ($b) {}\n@mixin c($d) {}\n@mixin e {}";
    assert_eq!(
      problems(mixin, "never", source, Syntax::Scss),
      [(
        1,
        10,
        "Unexpected whitespace before parentheses in mixin declaration".into()
      )]
    );
    assert_eq!(
      fix(mixin, json!("always"), source, Syntax::Scss),
      "@mixin a ($b) {}\n@mixin c ($d) {}\n@mixin e {}"
    );
    let else_if = "scss/at-else-if-parentheses-space-before";
    assert_eq!(
      fix(
        else_if,
        json!("never"),
        "@if a {} @else if  ($b) {}",
        Syntax::Scss
      ),
      "@if a {} @else if($b) {}"
    );
  }

  /// stylelint-scss's fix rewrites the params as PostCSS stores them, which
  /// drops a `//` comment in them; gale reports but does not fix there.
  #[test]
  fn fixes_that_would_drop_comments_are_left_out() {
    use crate::rules::spacing::harness::{bad, check};
    check(
      "scss/at-function-parentheses-space-before",
      json!("always"),
      Syntax::Scss,
      &[bad("@function a( // x\n $b) {}")
        .at(
          1,
          12,
          "Expected a single space before parentheses in function declaration",
        )
        .unfixed()],
    );
  }
}
