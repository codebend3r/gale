use crate::list_rules::{Found, ListCheck, ListRule, Polarity, subjects};
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "scss/function-disallowed-list";

declare_rule! {
  /// Disallow the Sass functions listed.
  ///
  /// Options: a function name or `/regex/`, or a list of them, matched
  /// against the name without its module namespace: `"random"` disallows
  /// `math.random()` too.  Functions in declaration values and at-rule
  /// params (`@if`, `@return`, mixin arguments) are checked; native CSS
  /// functions never are.
  ///
  /// Equivalent to `scss/function-disallowed-list`.
  pub struct ScssFunctionDisallowedList => ListCheck<ScssFunctionDisallowedList> {
    name: NAME,
    description: "Specify a list of disallowed SCSS functions",
    default_severity: Warning,
    primary_option_array: true,
  }
}

impl ListRule for ScssFunctionDisallowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Disallowed;
  type Options = ();

  fn find(tree: &PostcssTree, _: &RuleContext, _: &(), found: &mut Vec<Found>) {
    subjects::scss_function_names(tree, found);
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected disallowed function \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check;
  use crate::testing::invalid_options;

  #[test]
  fn reports_listed_functions() {
    check(
      NAME,
      Syntax::Scss,
      &[
        (
          json!(["random", "/double/"]),
          "a { margin-left: math.random(100); }",
          &["1:23 Unexpected disallowed function \"random\""],
        ),
        (
          json!(["/double/"]),
          "@function double($n) { @return $n * 2; }\na { margin: double(1px); }",
          &["2:13 Unexpected disallowed function \"double\""],
        ),
        (
          json!(["random"]),
          "a {\n  @if math.random(100) > 50 { margin: 0; }\n}",
          &["2:12 Unexpected disallowed function \"random\""],
        ),
        (
          json!(["min-width"]),
          "@include min-width(math.random(1)) {}",
          &[],
        ),
        (json!(["darken"]), "a { color: Darken($c, 10%); }", &[]),
        (json!(["rgba"]), "a { color: rgba(0, 0, 0, 0); }", &[]),
        (
          json!(["color.adjust"]),
          "a { color: color.adjust($c); }",
          &[],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!(["random"])).is_empty());
  }
}
