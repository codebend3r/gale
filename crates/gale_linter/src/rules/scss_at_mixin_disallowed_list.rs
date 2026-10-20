use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "scss/at-mixin-disallowed-list";

declare_rule! {
  /// Disallow including the SCSS mixins listed.
  ///
  /// Options: a mixin name or `/regex/`, or a list of them, matched
  /// exactly against the name each `@include` gives, in SCSS files.
  ///
  /// A Gale rule in the `scss/` namespace; stylelint-scss has no such rule.
  pub struct ScssAtMixinDisallowedList => ListCheck<ScssAtMixinDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed mixins",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for ScssAtMixinDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, ctx: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::included_mixins(tree, ctx.syntax, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected mixin \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::{check, check_css};
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_mixins() {
    check(
      NAME,
      Syntax::Scss,
      &[
        (
          json!(["breakpoint", "/^old-/"]),
          "a {\n  @include breakpoint(md) { color: red; }\n  @include old-header;\n  @include reset();\n}",
          &[
            "2:3 Unexpected mixin \"breakpoint\"",
            "3:3 Unexpected mixin \"old-header\"",
          ],
        ),
        (json!(["Breakpoint"]), "a { @include breakpoint; }", &[]),
        (json!(["breakpoint"]), "@mixin breakpoint {}", &[]),
      ],
    );
    check_css(
      NAME,
      &[(json!(["breakpoint"]), "@include breakpoint;", &[])],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!("breakpoint")).is_empty());
  }
}
