use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-if-no-null";

declare_rule! {
  /// Disallow `@if` conditions that compare to `null`.
  ///
  /// Users should use `not $variable` instead of `$variable != null`,
  /// and `$variable` instead of `$variable == null`.
  pub struct ScssAtIfNoNull => ScssAtIfNoNullCheck {
    name: NAME,
    description: "Disallow null comparisons in @if conditions",
    default_severity: Warning,
  }
}

/// `scss/at-if-no-null`, which takes no options.
pub struct ScssAtIfNoNullCheck;

impl ScssAtIfNoNullCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssAtIfNoNullCheck {
  /// Flags an `@if` condition comparing against `null` — `not $x` and `$x` say
  /// the same thing more directly — at the condition, as stylelint-scss
  /// reports it.  `@else if` is left alone, as stylelint-scss leaves it.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if at.name() != "if" {
        continue;
      }
      let params = at.params().to_ascii_lowercase();
      // Stylelint's scss/at-if-no-null skips `!= null and ...` patterns —
      // these are compound conditions where the null check is part of a
      // larger boolean expression and removing it would change semantics.
      if params.contains("!= null and ")
        || !(params.contains("== null") || params.contains("!= null"))
      {
        continue;
      }
      diags.push(
        Diagnostic::new(NAME, "Unexpected null comparison in @if condition")
          .span(at.word_span(at.params())),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The reports for `@if <params> {}` in `syntax`.
  fn if_rule(params: &str, syntax: Syntax) -> Vec<Diagnostic> {
    let source = format!("@if {params} {{ a: b }}");
    configured(&ScssAtIfNoNull, None).check_source(&context(&source, syntax))
  }

  #[test]
  fn reports_equals_null_at_the_condition() {
    let d = if_rule("$var == null", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].span.offset, d[0].span.length), (4, 12));
  }

  #[test]
  fn reports_not_equals_null() {
    assert_eq!(if_rule("$var != null", Syntax::Scss).len(), 1);
  }

  #[test]
  fn reports_inside_rules_but_not_else_if() {
    let scss = "a { @if $a == null { b: c } @else if $d == null { e: f } }";
    let d = configured(&ScssAtIfNoNull, None).check_source(&context(scss, Syntax::Scss));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span.offset, 8);
  }

  #[test]
  fn allows_non_null_comparison() {
    assert!(if_rule("$var", Syntax::Scss).is_empty());
    assert!(if_rule("not $var", Syntax::Scss).is_empty());
  }

  #[test]
  fn allows_not_equals_null_and_compound() {
    // `!= null and <expr>` is a compound condition — Stylelint skips these.
    assert!(if_rule("$var != null and $var != ''", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(if_rule("$var == null", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-if-no-null\""]
    );
  }
}
