use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity};

use crate::empty_lines::{empty_line_report, has_empty_line};
use crate::pattern::option_matches;
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext};

/// Require or disallow an empty line before rules.
///
/// Equivalent to Stylelint's `rule-empty-line-before` rule, autofix
/// included.  Primary option `"always"`, `"never"`, `"always-multi-line"` or
/// `"never-multi-line"`; secondary options:
///   - `except`: `after-rule`, `after-single-line-comment`, `first-nested`,
///     `inside-block-and-after-rule`, `inside-block`
///   - `ignore`: `after-comment`, `first-nested`, `inside-block`
///
/// Works on the [`PostcssTree`] of the source, so comments (which the CSS
/// parser drops) and keyframe selectors count as Stylelint counts them.
pub struct RuleEmptyLineBefore;

impl Rule for RuleEmptyLineBefore {
  fn name(&self) -> &'static str {
    "rule-empty-line-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow an empty line before rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks every rule in the document, in source order.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let expectation = ctx.primary_option_str().unwrap_or("always");
    let multi_line_only = expectation.contains("multi-line");
    let always = expectation.contains("always") || !expectation.contains("never");
    let secondary = ctx.secondary_options();
    let except = |name: &str| option_matches(secondary.and_then(|s| s.get("except")), name);
    let ignore = |name: &str| option_matches(secondary.and_then(|s| s.get("ignore")), name);
    let tree = PostcssTree::parse(ctx.source, ctx.syntax);

    let mut diags = Vec::new();
    for i in 0..tree.nodes.len() {
      if !tree.is_standard_syntax_rule(i) || tree.is_first_node_of_root(i) {
        continue;
      }
      let nested = tree.nodes[i].parent.is_some();
      let after_comment = tree
        .prev(i)
        .is_some_and(|prev| tree.nodes[prev].kind == NodeKind::Comment);
      if (ignore("after-comment") && after_comment)
        || (ignore("first-nested") && tree.is_first_nested(i))
        || (ignore("inside-block") && nested)
        || (multi_line_only && !tree.text(i).contains(['\n', '\r']))
      {
        continue;
      }

      let after_rule = is_after_rule(&tree, i);
      let flip = (except("first-nested") && tree.is_first_nested(i))
        || (except("after-rule") && after_rule)
        || (except("inside-block-and-after-rule") && nested && after_rule)
        || (except("after-single-line-comment") && tree.is_after_single_line_comment(i))
        || (except("inside-block") && nested);
      let expect_empty_line = always != flip;
      if expect_empty_line == has_empty_line(tree.before(i)) {
        continue;
      }
      diags.push(empty_line_report(
        self.name(),
        self.default_severity(),
        &tree,
        i,
        expect_empty_line,
        "rule",
      ));
    }
    diags
  }
}

/// Stylelint's `isAfterRule`: the node before (skipping shared-line
/// comments) is a rule.
fn is_after_rule(tree: &PostcssTree, i: usize) -> bool {
  tree
    .previous_non_shared_line_comment(i)
    .is_some_and(|prev| tree.nodes[prev].kind == NodeKind::Rule)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::empty_lines::fix_with;
  use gale_css_parser::Syntax;

  /// The messages for `source` in `syntax` with `options`.
  fn messages(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<String> {
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax,
      options: Some(&options),
    };
    RuleEmptyLineBefore
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn reports_missing_and_unexpected_empty_lines() {
    let css = Syntax::Css;
    assert_eq!(
      messages(
        "a { color: red; }\nb { color: blue; }",
        css,
        serde_json::json!("always")
      ),
      vec!["Expected empty line before rule"]
    );
    assert!(messages("a {}\n\nb {}", css, serde_json::json!("always")).is_empty());
    assert_eq!(
      messages("a {}\n\nb {}", css, serde_json::json!("never")),
      vec!["Expected no empty line before rule"]
    );
    assert!(messages("a {}\nb {}", css, serde_json::json!("always-multi-line")).is_empty());
  }

  #[test]
  fn comments_count_as_stylelint_counts_them() {
    let first_nested = serde_json::json!(["always", { "except": ["first-nested"] }]);
    // A comment between the brace and the rule makes it no longer first.
    assert_eq!(
      messages(
        "a {\n  /** c */\n  b { color: red; }\n}",
        Syntax::Css,
        first_nested.clone()
      )
      .len(),
      1
    );
    // ...unless the comment sits on the brace's line.
    assert!(
      messages(
        "a { /* c */\n  b { color: red; }\n}",
        Syntax::Css,
        first_nested
      )
      .is_empty()
    );
    let single_line = serde_json::json!(["always", { "except": ["after-single-line-comment"] }]);
    assert!(messages("a {}\n// c\nb {}", Syntax::Scss, single_line.clone()).is_empty());
    assert_eq!(
      messages("a {}\n/**\n * c\n */\nb {}", Syntax::Css, single_line).len(),
      1
    );
    let after_comment = serde_json::json!(["always", { "ignore": ["after-comment"] }]);
    assert!(messages("a {}\n/**\n * c\n */\nb {}", Syntax::Css, after_comment).is_empty());
  }

  #[test]
  fn keyframe_selectors_are_rules() {
    let source =
      "@keyframes foo {\n  0% {\n    opacity: 0;\n  }\n  100% {\n    opacity: 1;\n  }\n}";
    let opts = serde_json::json!(["always", { "except": ["first-nested"] }]);
    assert_eq!(messages(source, Syntax::Css, opts).len(), 1);
  }

  #[test]
  fn skips_non_standard_selectors() {
    let always = serde_json::json!("always");
    assert!(messages("a {}\n.b-#{$c} {}\n%d {}", Syntax::Scss, always.clone()).is_empty());
    assert!(messages("a {}\n.mixin() {}", Syntax::Less, always).is_empty());
  }

  #[test]
  fn handles_multibyte_text_before_the_rule() {
    let source = "--x: \u{4e2d};\n\nb { color: blue; }";
    assert!(messages(source, Syntax::Css, serde_json::json!("always")).is_empty());
  }

  #[test]
  fn fix_adds_and_removes_empty_lines() {
    assert_eq!(
      fix_with(
        "rule-empty-line-before",
        serde_json::json!(["always", { "except": ["first-nested"] }]),
        "a {\n\n  b {}\n  c {}\n}\nd {}",
        Syntax::Css,
      ),
      "a {\n  b {}\n\n  c {}\n}\n\nd {}"
    );
    assert_eq!(
      fix_with(
        "rule-empty-line-before",
        serde_json::json!("never"),
        "a {}\r\n\r\n\r\nb {}",
        Syntax::Css,
      ),
      "a {}\r\nb {}"
    );
  }

  #[test]
  fn fix_puts_a_same_line_rule_on_its_own_line() {
    assert_eq!(
      fix_with(
        "rule-empty-line-before",
        serde_json::json!("always"),
        "a {} b {}",
        Syntax::Css,
      ),
      "a {}\n\n b {}"
    );
  }
}
