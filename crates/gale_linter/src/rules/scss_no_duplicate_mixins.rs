use std::collections::HashSet;

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/no-duplicate-mixins";

declare_rule! {
  /// Disallow duplicate `@mixin` declarations with the same name.
  pub struct ScssNoDuplicateMixins => ScssNoDuplicateMixinsCheck {
    name: NAME,
    description: "Disallow duplicate mixin names",
    default_severity: Warning,
  }
}

/// `scss/no-duplicate-mixins`, which takes no options.
pub struct ScssNoDuplicateMixinsCheck;

impl ScssNoDuplicateMixinsCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssNoDuplicateMixinsCheck {
  /// Flags a `@mixin` name declared more than once.  The name is everything
  /// before the first `(` or space.
  ///
  /// Mixins inside style rules are left out, as they always were: they are
  /// scoped to the rule, and stylelint-scss, which does look at them, only
  /// compares those that share a parent with the first one.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut seen: HashSet<&str> = HashSet::new();
    let mut diagnostics = Vec::new();
    for at in tree.walk_at_rules() {
      if at.name() != "mixin" || at.ancestors().any(|node| node.as_rule().is_some()) {
        continue;
      }
      let mixin_name = at
        .params()
        .trim()
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
      if mixin_name.is_empty() || seen.insert(mixin_name) {
        continue;
      }
      // Point to the mixin name, not the `@mixin` keyword.
      // Stylelint message has no quotes around the name.
      diagnostics.push(
        Diagnostic::new(NAME, format!("Unexpected duplicate mixin {mixin_name}"))
          .span(at.word_span(mixin_name)),
      );
    }
    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&ScssNoDuplicateMixins, None).check_source(&context(source, syntax))
  }

  #[test]
  fn skips_non_scss() {
    assert!(check("@mixin foo {}\n@mixin foo {}", Syntax::Css).is_empty());
  }

  #[test]
  fn reports_duplicate_mixins_at_the_name() {
    let d = check(
      "@mixin foo {}\n@mixin bar {}\n@if $a { @mixin foo($b) {} }",
      Syntax::Scss,
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("foo"));
    assert_eq!((d[0].span.offset, d[0].span.length), (44, 3));
  }

  #[test]
  fn allows_unique_mixins_and_mixins_in_rules() {
    assert!(check("@mixin foo {}\n@mixin bar {}\n@mixin baz {}", Syntax::Scss).is_empty());
    assert!(check("a { @mixin foo {} }\nb { @mixin foo {} }", Syntax::Scss).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/no-duplicate-mixins\""]
    );
  }
}
