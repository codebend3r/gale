use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-extend-no-missing-placeholder";

declare_rule! {
  /// Disallow `@extend` without a `%placeholder` selector.
  pub struct ScssAtExtendNoMissingPlaceholder => ScssAtExtendNoMissingPlaceholderCheck {
    name: NAME,
    description: "Disallow @extend without a %placeholder selector",
    default_severity: Warning,
  }
}

/// `scss/at-extend-no-missing-placeholder`, which takes no options.
pub struct ScssAtExtendNoMissingPlaceholderCheck;

impl ScssAtExtendNoMissingPlaceholderCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssAtExtendNoMissingPlaceholderCheck {
  /// Flags an `@extend` whose target is not a `%placeholder` selector, at the
  /// selector as stylelint-scss finds it.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      let params = at.params().trim();
      if at.name() != "extend" || params.starts_with('%') {
        continue;
      }
      diags.push(
        Diagnostic::new(
          NAME,
          "Expected a placeholder selector (e.g. %placeholder) to be used in @extend",
        )
        .span(at.word_span(params)),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&ScssAtExtendNoMissingPlaceholder, None).check_source(&context(source, syntax))
  }

  #[test]
  fn reports_non_placeholder_at_the_selector() {
    let d = check("a { @extend .foo; }", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("placeholder"));
    assert_eq!((d[0].span.offset, d[0].span.length), (12, 4));
  }

  #[test]
  fn allows_placeholder() {
    assert!(check("a { @extend %placeholder; }", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_extend() {
    assert!(check("@mixin foo { }", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(check("a { @extend .foo; }", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-extend-no-missing-placeholder\""]
    );
  }
}
