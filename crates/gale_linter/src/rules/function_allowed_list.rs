use crate::list_rules::{Found, List, ListCheck, ListRule, Polarity, list_option, subjects};
use crate::options::Options;
use crate::postcss_tree::PostcssTree;
use crate::rule::{RuleContext, declare_rule};

const NAME: &str = "function-allowed-list";

declare_rule! {
  /// Only allow the functions listed.
  ///
  /// Options: a function name or `/regex/`, or a list of them, matched
  /// exactly as written, as Stylelint 17 does: `RGB()` and `-webkit-calc()`
  /// need entries of their own.  `exceptWithoutPropertyFallback` names
  /// listed functions that are still reported unless an earlier
  /// declaration of the same property gives a fallback.
  ///
  /// Equivalent to Stylelint's `function-allowed-list` rule.
  pub struct FunctionAllowedList => ListCheck<FunctionAllowedList> {
    name: NAME,
    description: "Specify a list of allowed functions",
    default_severity: Warning,
    primary_option_array: true,
  }
}

/// `function-allowed-list`'s secondary options.
#[derive(Default)]
pub struct FunctionAllowedListOptions {
  /// `exceptWithoutPropertyFallback`.
  except_without_fallback: List,
}

impl ListRule for FunctionAllowedList {
  const NAME: &'static str = NAME;
  const POLARITY: Polarity = Polarity::Allowed;
  type Options = FunctionAllowedListOptions;

  fn options(options: &mut Options<'_>) -> Self::Options {
    FunctionAllowedListOptions {
      except_without_fallback: list_option(options, "exceptWithoutPropertyFallback"),
    }
  }

  fn find(tree: &PostcssTree, _: &RuleContext, _: &Self::Options, found: &mut Vec<Found>) {
    subjects::function_names(tree, found);
  }

  /// Listed, unless it is also an exception and nothing before it in its
  /// block declares the property as a fallback.
  fn listed<'l>(
    list: &'l List,
    options: &Self::Options,
    found: &Found,
    tree: &PostcssTree,
  ) -> Option<&'l str> {
    let entry = list.find(&found.name)?;
    let needs_fallback = options.except_without_fallback.matches(&found.name);
    (!needs_fallback || subjects::has_fallback_declaration(tree, found.node)).then_some(entry)
  }

  fn message(found: &Found, _: Option<&str>) -> String {
    format!("Unexpected function \"{}\"", found.name)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::NAME;
  use crate::list_rules::testing::check_css;
  use crate::testing::invalid_options;

  #[test]
  fn reports_functions_missing_from_the_list() {
    check_css(
      NAME,
      &[
        (json!(["rgb", "hsl"]), "a { color: rgb(0, 0, 0); }", &[]),
        (
          json!(["rgb"]),
          "a { color: hsl(0, 100%, 50%); }",
          &["1:12 Unexpected function \"hsl\""],
        ),
        (
          json!(["rgb"]),
          "a { color: RGB(0, 0, 0); }",
          &["1:12 Unexpected function \"RGB\""],
        ),
        (
          json!(["calc"]),
          "a { width: -webkit-calc(1px + 2px); }",
          &["1:12 Unexpected function \"-webkit-calc\""],
        ),
        (json!(["/^rgb/"]), "a { color: rgba(0, 0, 0, 0.5); }", &[]),
        (
          json!([["translate"], { "severity": "warning" }]),
          "a { transform: translate(var(--x)); }",
          &["1:26 Unexpected function \"var\""],
        ),
        (json!(["rgb"]), "a { color: red; }", &[]),
      ],
    );
  }

  #[test]
  fn listed_functions_can_require_a_fallback() {
    let options = json!([["rgb", "oklch"], { "exceptWithoutPropertyFallback": ["oklch"] }]);
    check_css(
      NAME,
      &[
        (
          options.clone(),
          "a { color: oklch(0 0 0); }",
          &["1:12 Unexpected function \"oklch\""],
        ),
        (
          options,
          "a { color: rgb(0 0 0); color: oklch(0 0 0); }",
          &[],
        ),
      ],
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert_eq!(
      invalid_options(
        NAME,
        json!([["rgb"], { "exceptWithoutPropertyFallback": [1] }])
      ),
      [
        "Invalid value \"1\" for option \"exceptWithoutPropertyFallback\" of rule \"function-allowed-list\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, json!([["rgb"], { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"function-allowed-list\""]
    );
  }
}
