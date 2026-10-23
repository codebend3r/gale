use gale_css_parser::postcss_tree::{AtRuleRef, NodeRef, PostcssTree};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "keyframe-declaration-no-important";

declare_rule! {
  /// Reports `!important` within `@keyframes` declarations.
  ///
  /// Using `!important` inside keyframe declarations is ignored by browsers
  /// and is almost certainly a mistake.
  ///
  /// Equivalent to Stylelint's `keyframe-declaration-no-important` rule.
  pub struct KeyframeDeclarationNoImportant => KeyframeDeclarationNoImportantCheck {
    name: NAME,
    description: "Disallow !important within keyframe declarations",
    default_severity: Warning,
  }
}

/// `keyframe-declaration-no-important`, which takes no options.
pub struct KeyframeDeclarationNoImportantCheck;

impl KeyframeDeclarationNoImportantCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for KeyframeDeclarationNoImportantCheck {
  /// Flags every `!important` declaration in a `@keyframes` block or in
  /// one of its keyframe selectors: browsers ignore them there.
  fn check(&self, tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_decls()
      .filter(|decl| decl.important() && in_keyframes_block(decl.node()))
      .map(|decl| {
        Diagnostic::new(
          NAME,
          format!(
            "Unexpected !important in keyframe declaration \"{}\"",
            decl.prop()
          ),
        )
        .span(decl.span())
      })
      .collect()
  }
}

/// Whether `decl` sits directly in a `@keyframes` block, or in a rule that
/// does.
fn in_keyframes_block(decl: NodeRef) -> bool {
  let Some(parent) = decl.parent() else {
    return false;
  };
  let block = match parent.as_rule() {
    Some(rule) => rule.parent(),
    None => Some(parent),
  };
  block
    .and_then(NodeRef::as_at_rule)
    .is_some_and(AtRuleRef::is_keyframes)
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source` in `syntax`: each message and the
  /// text it points at.
  fn check(source: &str, syntax: Syntax) -> Vec<(String, &str)> {
    configured(&KeyframeDeclarationNoImportant, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| {
        (
          d.message,
          &source[d.span.offset..d.span.offset + d.span.length],
        )
      })
      .collect()
  }

  #[test]
  fn reports_important_in_keyframe() {
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      assert_eq!(
        check(
          "@keyframes fade { from { opacity: 0 !important; } to { opacity: 1 } }",
          syntax
        ),
        [(
          "Unexpected !important in keyframe declaration \"opacity\"".to_string(),
          "opacity: 0 !important;"
        )]
      );
    }
  }

  #[test]
  fn reports_declarations_directly_in_the_at_rule() {
    let d = check("@keyframes fade { opacity: 0 !important }", Syntax::Scss);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn names_match_like_stylelint() {
    for name in ["-webkit-keyframes", "KEYFRAMES", "-ms-keyframes"] {
      let source = format!("@{name} fade {{ from {{ opacity: 0 !important }} }}");
      assert_eq!(check(&source, Syntax::Scss).len(), 1, "{name}");
      assert_eq!(check(&source, Syntax::Css).len(), 1, "{name}");
    }
    assert!(
      check(
        "@-x-keyframes f { from { top: 0 !important } }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn ignores_keyframe_without_important() {
    assert!(check("@keyframes fade { from { opacity: 0 } }", Syntax::Css).is_empty());
  }

  #[test]
  fn ignores_non_keyframe_at_rule() {
    let source = "@media (min-width: 768px) { a { color: red !important } }";
    assert!(check(source, Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options("keyframe-declaration-no-important", serde_json::json!(true)).is_empty()
    );
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "keyframe-declaration-no-important",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        "keyframe-declaration-no-important",
        serde_json::json!("always")
      ),
      ["Unexpected option value \"always\" for rule \"keyframe-declaration-no-important\""]
    );
  }
}
