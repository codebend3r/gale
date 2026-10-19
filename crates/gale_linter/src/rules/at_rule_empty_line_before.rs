use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::empty_lines::{empty_line_report, has_empty_line};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "at-rule-empty-line-before";

declare_rule! {
  /// Require or disallow an empty line before at-rules.
  ///
  /// Equivalent to Stylelint's `at-rule-empty-line-before` rule, autofix
  /// included.  Primary option `"always"` or `"never"`; secondary options:
  ///   - `except`: `after-same-name`, `inside-block`,
  ///     `blockless-after-same-name-blockless`, `blockless-after-blockless`,
  ///     `first-nested`
  ///   - `ignore`: `after-comment`, `first-nested`, `inside-block`,
  ///     `blockless-after-same-name-blockless`, `blockless-after-blockless`
  ///   - `ignoreAtRules`: names or `/regex/`es
  ///
  /// Works on the [`PostcssTree`] of the source, so at-rules the CSS parser
  /// drops (`@mixin` in plain CSS, `@import` after a rule) and comments count
  /// as Stylelint counts them.
  pub struct AtRuleEmptyLineBefore => AtRuleEmptyLineBeforeCheck {
    name: NAME,
    description: "Require or disallow an empty line before at-rules",
    default_severity: Warning,
  }
}

/// `at-rule-empty-line-before` as configured.
pub struct AtRuleEmptyLineBeforeCheck {
  /// `"always"` rather than `"never"`.
  always: bool,
  /// The `except` keywords.
  except: Vec<&'static str>,
  /// The `ignore` keywords.
  ignore: Vec<&'static str>,
  /// `ignoreAtRules`.
  ignore_at_rules: Matchers,
}

impl AtRuleEmptyLineBeforeCheck {
  /// Stylelint's options: `"always"` or `"never"`, then `except`, `ignore`
  /// and `ignoreAtRules`.  Gale reads no option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options.primary_one_of(&["always", "never"]);
    let except = options.keywords(
      "except",
      &[
        "after-same-name",
        "inside-block",
        "blockless-after-same-name-blockless",
        "blockless-after-blockless",
        "first-nested",
      ],
    );
    let ignore = options.keywords(
      "ignore",
      &[
        "after-comment",
        "first-nested",
        "inside-block",
        "blockless-after-same-name-blockless",
        "blockless-after-blockless",
      ],
    );
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self {
      always: expectation != Some("never"),
      except,
      ignore,
      ignore_at_rules,
    })
  }
}

impl Check for AtRuleEmptyLineBeforeCheck {
  /// Checks every at-rule in the document, in source order.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let always = self.always;
    let except = |name: &str| self.except.contains(&name);
    let ignore = |name: &str| self.ignore.contains(&name);
    let tree = ctx.postcss_tree();

    let mut diags = Vec::new();
    for i in 0..tree.nodes.len() {
      if tree.nodes[i].kind != NodeKind::AtRule
        || tree.is_first_node_of_root(i)
        || !tree.is_standard_syntax_at_rule(i)
        || self.ignore_at_rules.matches(&tree.nodes[i].name)
      {
        continue;
      }
      let nested = tree.nodes[i].parent.is_some();
      let blockless_after_blockless = is_blockless_after_blockless(&tree, i);
      let blockless_after_same_name = blockless_after_blockless && is_after_same_name(&tree, i);
      if (ignore("blockless-after-blockless") && blockless_after_blockless)
        || (ignore("first-nested") && tree.is_first_nested(i))
        || (ignore("blockless-after-same-name-blockless") && blockless_after_same_name)
        || (ignore("inside-block") && nested)
        || (ignore("after-comment") && tree.is_after_comment(i))
      {
        continue;
      }

      let flip = (except("after-same-name") && is_after_same_name(&tree, i))
        || (except("inside-block") && nested)
        || (except("first-nested") && tree.is_first_nested(i))
        || (except("blockless-after-blockless") && blockless_after_blockless)
        || (except("blockless-after-same-name-blockless") && blockless_after_same_name);
      let expect_empty_line = always != flip;
      if expect_empty_line == has_empty_line(tree.before(i)) {
        continue;
      }
      diags.push(empty_line_report(
        NAME,
        &tree,
        i,
        expect_empty_line,
        "at-rule",
      ));
    }
    diags
  }
}

/// Stylelint's `isBlocklessAtRuleAfterBlocklessAtRule`: neither the at-rule
/// nor the node before it (skipping shared-line comments) has a block, and
/// that node is an at-rule.
fn is_blockless_after_blockless(tree: &PostcssTree, i: usize) -> bool {
  tree
    .previous_non_shared_line_comment(i)
    .is_some_and(|prev| {
      tree.nodes[prev].kind == NodeKind::AtRule && !tree.has_block(prev) && !tree.has_block(i)
    })
}

/// The node before the at-rule (skipping shared-line comments) is an
/// at-rule of the same name (Stylelint's `isAtRuleAfterSameNameAtRule`).
fn is_after_same_name(tree: &PostcssTree, i: usize) -> bool {
  tree
    .previous_non_shared_line_comment(i)
    .is_some_and(|prev| {
      tree.nodes[prev].kind == NodeKind::AtRule && tree.nodes[prev].name == tree.nodes[i].name
    })
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{configured, ctx_with_source, fix, invalid_options};

  /// The messages for `source` with `options`.
  fn messages(source: &str, options: serde_json::Value) -> Vec<String> {
    let ctx = ctx_with_source(source);
    configured(&AtRuleEmptyLineBefore, options)
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn reports_missing_and_unexpected_empty_lines() {
    let always = serde_json::json!("always");
    assert_eq!(
      messages("a {}\n@media screen {}", always.clone()),
      vec!["Expected empty line before at-rule"]
    );
    assert!(messages("a {}\n\n@media screen {}", always.clone()).is_empty());
    assert!(messages("@media screen {}", always).is_empty());
    assert_eq!(
      messages("a {}\n\n@media screen {}", serde_json::json!("never")),
      vec!["Expected no empty line before at-rule"]
    );
  }

  #[test]
  fn sees_at_rules_and_comments_the_css_parser_drops() {
    let always = serde_json::json!("always");
    // lightningcss drops `@import` after a rule and `@mixin` in CSS.
    assert_eq!(
      messages("@keyframes foo {}\n@import 'x.css'", always.clone()).len(),
      1
    );
    assert_eq!(
      messages("a {\n  color: pink;\n  @mixin foo;\n}", always.clone()).len(),
      1
    );
    let ignore_comments = serde_json::json!(["always", { "ignore": ["after-comment"] }]);
    assert!(messages("a {}\n/* c */\n@media {}", ignore_comments).is_empty());
  }

  #[test]
  fn except_and_ignore_options() {
    let grouped =
      serde_json::json!(["always", { "except": ["blockless-after-same-name-blockless"] }]);
    assert!(messages("@import \"a.css\";\n@import \"b.css\";", grouped).is_empty());
    let inside = serde_json::json!(["always", { "except": ["inside-block"] }]);
    assert!(messages("a {\n  color: red;\n  @media x {}\n}", inside).is_empty());
    let ignored = serde_json::json!(["always", { "ignoreAtRules": ["else"] }]);
    assert!(messages("@if a {}\n@else {}", ignored).is_empty());
  }

  #[test]
  fn fix_adds_and_removes_empty_lines_keeping_indentation() {
    assert_eq!(
      fix(
        "at-rule-empty-line-before",
        serde_json::json!("always"),
        "a {\n  color: red;\n  @media x {}\n}\n@import 'b';",
        Syntax::Css,
      ),
      "a {\n  color: red;\n\n  @media x {}\n}\n\n@import 'b';"
    );
    assert_eq!(
      fix(
        "at-rule-empty-line-before",
        serde_json::json!("never"),
        "a {}\n\n  \n@media x {}",
        Syntax::Css,
      ),
      "a {}\n@media x {}"
    );
  }

  #[test]
  fn fix_keeps_the_files_line_breaks() {
    assert_eq!(
      fix(
        "at-rule-empty-line-before",
        serde_json::json!("always"),
        "a {}\r\n@media x {} @import 'y';",
        Syntax::Css,
      ),
      "a {}\r\n\r\n@media x {}\r\n\r\n @import 'y';"
    );
  }

  #[test]
  fn fix_works_in_scss() {
    assert_eq!(
      fix(
        "at-rule-empty-line-before",
        serde_json::json!(["always", { "except": ["first-nested"] }]),
        "a {\n\n  @include x;\n  // note\n  @include y;\n}",
        Syntax::Scss,
      ),
      "a {\n  @include x;\n  // note\n\n  @include y;\n}"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never", {
          "except": ["after-same-name", "first-nested"],
          "ignore": "after-comment",
          "ignoreAtRules": ["else", "/^my-/"]
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"at-rule-empty-line-before\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "except": ["after-comment"], "ignoreRules": [] }])
      ),
      [
        "Invalid value \"after-comment\" for option \"except\" of rule \"at-rule-empty-line-before\"",
        "Invalid option name \"ignoreRules\" for rule \"at-rule-empty-line-before\""
      ]
    );
  }
}
