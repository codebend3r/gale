use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "block-no-redundant-nested-style-rules";

declare_rule! {
  /// Disallow redundant nested style rules that could be combined with their parent.
  ///
  /// Specifically, this rule flags nested rules whose selector is just `&` (a bare
  /// ampersand), since all declarations inside such a rule could be placed directly
  /// in the parent block.
  ///
  /// Equivalent to Stylelint's `block-no-redundant-nested-style-rules` rule.
  pub struct BlockNoRedundantNestedStyleRules => BlockNoRedundantNestedStyleRulesCheck {
    name: NAME,
    description: "Disallow redundant nested style rules that could be combined with their parent",
    default_severity: Warning,
  }
}

/// `block-no-redundant-nested-style-rules`, which takes no options.
pub struct BlockNoRedundantNestedStyleRulesCheck;

impl BlockNoRedundantNestedStyleRulesCheck {
  /// Stylelint's option: `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for BlockNoRedundantNestedStyleRulesCheck {
  /// Flags nested rules whose selector is a bare `&`, since their declarations
  /// could live in the enclosing block.  As in Stylelint, any rule that is
  /// not at the top level counts, whether it sits in a rule or an at-rule.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    tree
      .walk_rules()
      .filter(|rule| rule.parent().is_some() && rule.is_standard_syntax() && rule.selector() == "&")
      .map(|rule| {
        Diagnostic::new(
          NAME,
          "Unexpected redundant nested style rule with selector \"&\"; \
                       declarations can be placed directly in the parent rule"
            .to_string(),
        )
        .span(rule.span())
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// What the rule reports for `source`.
  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&BlockNoRedundantNestedStyleRules, None).check_source(&context(source, syntax))
  }

  #[test]
  fn flags_bare_ampersand_nested_rule() {
    let source = ".a { & { color: red; } }";
    let diags = check(source, Syntax::Css);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("redundant"));
    assert!(diags[0].message.contains("&"));
    assert_eq!(diags[0].span.offset, 5);
    assert_eq!(diags[0].span.length, "& { color: red; }".len());
  }

  #[test]
  fn flags_ampersand_with_whitespace() {
    assert_eq!(
      check(".a {\n  &  { color: red; }\n}", Syntax::Scss).len(),
      1
    );
  }

  #[test]
  fn allows_meaningful_nested_selector() {
    assert!(check(".a { .b { color: red; } }", Syntax::Css).is_empty());
  }

  #[test]
  fn allows_ampersand_with_suffix() {
    // `&:hover` is not redundant — it creates `.a:hover`
    assert!(check(".a { &:hover { color: red; } }", Syntax::Css).is_empty());
  }

  #[test]
  fn allows_ampersand_with_class() {
    // `&.active` is not redundant — it creates `.a.active`
    assert!(check(".a { &.active { color: red; } }", Syntax::Css).is_empty());
  }

  #[test]
  fn flags_multiple_redundant_children() {
    let source = ".a { & { color: red; } & { color: blue; } }";
    assert_eq!(check(source, Syntax::Css).len(), 2);
  }

  #[test]
  fn flags_ampersand_under_an_at_rule_as_stylelint_does() {
    // Stylelint skips only rules at the root.
    let source = ".a { @media print { & { color: red; } } }\n@media screen { & { color: red; } }";
    assert_eq!(check(source, Syntax::Scss).len(), 2);
    assert!(check("& { color: red; }", Syntax::Scss).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"block-no-redundant-nested-style-rules\""]
    );
  }
}
