use gale_css_parser::postcss_tree::{NodeKind, PostcssTree};
use gale_diagnostics::Diagnostic;

use crate::empty_lines::{empty_line_report, has_empty_line};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "custom-property-empty-line-before";

declare_rule! {
  /// Require or disallow an empty line before custom properties (`--*`).
  ///
  /// Equivalent to Stylelint's `custom-property-empty-line-before` rule,
  /// autofix included.  Primary option `"always"` or `"never"`; secondary
  /// options:
  ///   - `except`: `after-block`, `after-comment`, `after-custom-property`,
  ///     `first-nested`
  ///   - `ignore`: `after-comment`, `after-custom-property`, `first-nested`,
  ///     `inside-single-line-block`
  ///
  /// Works on the [`PostcssTree`] of the source, so comments and multi-line
  /// values read as Stylelint reads them.
  pub struct CustomPropertyEmptyLineBefore => CustomPropertyEmptyLineBeforeCheck {
    name: NAME,
    description: "Require or disallow an empty line before custom properties",
    default_severity: Warning,
  }
}

/// `custom-property-empty-line-before` as configured.
pub struct CustomPropertyEmptyLineBeforeCheck {
  /// `"always"` rather than `"never"`.
  always: bool,
  /// The `except` keywords.
  except: Vec<&'static str>,
  /// The `ignore` keywords.
  ignore: Vec<&'static str>,
}

impl CustomPropertyEmptyLineBeforeCheck {
  /// Stylelint's options: `"always"` or `"never"`, then `except` and
  /// `ignore`.  Gale reads no option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options.primary_one_of(&["always", "never"]);
    let except = options.keywords(
      "except",
      &[
        "after-block",
        "after-comment",
        "after-custom-property",
        "first-nested",
      ],
    );
    let ignore = options.keywords(
      "ignore",
      &[
        "after-comment",
        "after-custom-property",
        "first-nested",
        "inside-single-line-block",
      ],
    );
    options.finish()?;
    Ok(Self {
      always: expectation != Some("never"),
      except,
      ignore,
    })
  }
}

impl Check for CustomPropertyEmptyLineBeforeCheck {
  /// Checks every custom property in the document, in source order.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let always = self.always;
    let except = |name: &str| self.except.contains(&name);
    let ignore = |name: &str| self.ignore.contains(&name);

    let mut diags = Vec::new();
    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if node.kind != NodeKind::Decl
        || !tree.is_standard_syntax_declaration(i)
        || !node.name.starts_with("--")
      {
        continue;
      }
      let in_block = node
        .parent
        .is_some_and(|p| matches!(tree.nodes[p].kind, NodeKind::Rule | NodeKind::AtRule));
      if (ignore("after-comment") && tree.is_after_comment(i))
        || (ignore("after-custom-property") && is_after_custom_property(tree, i))
        || (ignore("first-nested") && tree.is_first_nested(i))
        || (ignore("inside-single-line-block")
          && in_block
          && !tree.parent_block_string(i).contains(['\n', '\r']))
      {
        continue;
      }

      let flip = (except("first-nested") && tree.is_first_nested(i))
        || (except("after-block") && tree.is_after_block(i))
        || (except("after-comment") && tree.is_after_comment(i))
        || (except("after-custom-property") && is_after_custom_property(tree, i));
      let expect_empty_line = always != flip;
      if expect_empty_line == has_empty_line(tree.before(i)) {
        continue;
      }
      diags.push(empty_line_report(
        NAME,
        tree,
        i,
        expect_empty_line,
        "custom property",
      ));
    }
    diags
  }
}

/// The node before (skipping shared-line comments) is a custom property
/// (the rule's own `isAfterCustomProperty`).
fn is_after_custom_property(tree: &PostcssTree, i: usize) -> bool {
  tree
    .previous_non_shared_line_comment(i)
    .is_some_and(|prev| {
      tree.nodes[prev].kind == NodeKind::Decl && tree.nodes[prev].name.starts_with("--")
    })
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, fix, invalid_options};

  /// The messages for `source` in `syntax` with `options`.
  fn messages(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<String> {
    let ctx = context(source, syntax);
    configured(&CustomPropertyEmptyLineBefore, options)
      .check_source(&ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn reports_missing_and_unexpected_empty_lines() {
    let css = Syntax::Css;
    assert_eq!(
      messages(
        "a {\n  top: 0;\n  --foo: 1;\n}",
        css,
        serde_json::json!("always")
      ),
      vec!["Expected empty line before custom property"]
    );
    assert_eq!(
      messages(
        "a {\n  top: 0;\n\n  --foo: 1;\n}",
        css,
        serde_json::json!("never")
      ),
      vec!["Expected no empty line before custom property"]
    );
    let grouped =
      serde_json::json!(["always", { "except": ["after-custom-property", "first-nested"] }]);
    assert!(messages("a {\n  --a: 1;\n  --b: 2;\n}", css, grouped).is_empty());
  }

  #[test]
  fn a_declaration_using_custom_properties_is_no_custom_property() {
    // mastodon: the declaration before uses `var(--...)`, after two custom
    // properties, and the empty line before the next one is wanted.
    let source = "a {\n  --header-height: 80px;\n  --footer-height: 200px;\n\n  scroll-padding-block: var(--header-height) var(--footer-height);\n\n  --navigation-background-color: red;\n}\n";
    let opts = serde_json::json!(["always", { "except": ["after-custom-property", "first-nested"], "ignore": ["after-comment", "inside-single-line-block"] }]);
    assert!(messages(source, Syntax::Scss, opts.clone()).is_empty());
    let multi_line = "a {\n  --a: 1px;\n  --b:\n    2px\n    3px;\n  --c: 0;\n}\n";
    assert!(messages(multi_line, Syntax::Css, opts).is_empty());
  }

  #[test]
  fn handles_multibyte_text_before_the_custom_property() {
    let source = "a {\n  content: \"\u{4e2d}\";\n\n  --b: 1;\n}";
    assert!(messages(source, Syntax::Css, serde_json::json!("always")).is_empty());
  }

  #[test]
  fn fix_adds_and_removes_empty_lines() {
    assert_eq!(
      fix(
        "custom-property-empty-line-before",
        serde_json::json!(["always", { "except": ["after-custom-property", "first-nested"] }]),
        "a {\n\n  --a: 1;\n\n  --b: 2;\n  top: 0;\n  --c: 3;\n}",
        Syntax::Css,
      ),
      "a {\n  --a: 1;\n  --b: 2;\n  top: 0;\n\n  --c: 3;\n}"
    );
    assert_eq!(
      fix(
        "custom-property-empty-line-before",
        serde_json::json!("never"),
        "a {\r\n  top: 0;\r\n\r\n  --c: 3;\r\n}",
        Syntax::Scss,
      ),
      "a {\r\n  top: 0;\r\n  --c: 3;\r\n}"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never", {
          "except": ["after-block", "after-comment", "after-custom-property", "first-nested"],
          "ignore": ["after-comment", "after-custom-property", "first-nested", "inside-single-line-block"]
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"custom-property-empty-line-before\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "ignore": ["after-block"] }])
      ),
      [
        "Invalid value \"after-block\" for option \"ignore\" of rule \"custom-property-empty-line-before\""
      ]
    );
  }
}
