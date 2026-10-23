use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting, is_truthy};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::stylelint_order::{is_standard_syntax_property, normalized_property};

const NAME: &str = "order/properties-alphabetical-order";

declare_rule! {
  /// Enforce alphabetical ordering of properties within declaration blocks.
  ///
  /// Equivalent to stylelint-order's `order/properties-alphabetical-order` rule.
  pub struct OrderPropertiesAlphabeticalOrder => OrderPropertiesAlphabeticalOrderCheck {
    name: NAME,
    description: "Require properties within declaration blocks to be in alphabetical order",
    default_severity: Warning,
  }
}

/// `order/properties-alphabetical-order`, which takes no options.
pub struct OrderPropertiesAlphabeticalOrderCheck;

impl OrderPropertiesAlphabeticalOrderCheck {
  /// stylelint-order validates the primary option with `possible: Boolean`,
  /// a function: any truthy value is accepted.  It validates no secondary
  /// options.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_test(is_truthy);
    options.finish()?;
    Ok(Self)
  }
}

impl Check for OrderPropertiesAlphabeticalOrderCheck {
  /// Flags a property that sorts before the one declared above it in the
  /// same style rule.  SCSS variables and custom properties are skipped;
  /// names print as written, as stylelint-order prints them.
  fn check(&self, tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for rule in tree.walk_rules() {
      let mut prev: Option<(String, &str)> = None;
      for decl in rule.child_decls() {
        let prop = decl.prop();
        // stylelint-order passes over variables, interpolated names and
        // custom properties.
        if !is_standard_syntax_property(prop) || prop.starts_with("--") {
          continue;
        }
        // Compare without the vendor prefix, ignoring case, and with
        // `-moz-osx-font-smoothing` read as `font-smoothing`.
        let sort_key = normalized_property(prop);
        if let Some((prev_key, prev_prop)) = &prev
          && sort_key < *prev_key
        {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Expected \"{prop}\" to come before \"{prev_prop}\""),
            )
            .span(decl.span()),
          );
        }
        prev = Some((sort_key, prop));
      }
    }
    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// The messages reported for `source` in `syntax`.
  fn messages(source: &str, syntax: Syntax) -> Vec<String> {
    configured(&OrderPropertiesAlphabeticalOrder, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn accepts_alphabetical_order() {
    let source = "a { color: red; display: block; font-size: 12px; }";
    assert!(messages(source, Syntax::Css).is_empty());
  }

  #[test]
  fn reports_non_alphabetical_order() {
    assert_eq!(
      messages("a { display: block; color: red; }", Syntax::Css),
      ["Expected \"color\" to come before \"display\""]
    );
  }

  #[test]
  fn skips_scss_variables() {
    let source = "a { display: block; $my-var: 10px; color: red; }";
    // "color" still follows "display": `$my-var` is skipped.
    assert_eq!(messages(source, Syntax::Scss).len(), 1);
  }

  #[test]
  fn skips_interpolated_properties_and_nested_property_blocks() {
    let source = "a { padding: { #{$end}: 0; #{$start}: 0; } margin: { left: 0; } top: 0; }";
    assert!(messages(source, Syntax::Scss).is_empty());
  }

  #[test]
  fn case_insensitive_comparison() {
    assert!(messages("a { Color: red; display: block; }", Syntax::Css).is_empty());
  }

  #[test]
  fn names_print_as_written() {
    assert_eq!(
      messages("a { DISPLAY: block; Color: red; }", Syntax::Css),
      ["Expected \"Color\" to come before \"DISPLAY\""]
    );
  }

  #[test]
  fn osx_font_smoothing_sorts_as_font_smoothing() {
    let source = "a { -moz-osx-font-smoothing: grayscale; line-height: 1; }";
    assert!(messages(source, Syntax::Css).is_empty());
  }

  #[test]
  fn skips_custom_properties() {
    let source = "a { display: block; --my-var: 10px; font-size: 12px; }";
    assert!(messages(source, Syntax::Css).is_empty());
  }

  #[test]
  fn vendor_prefixed_sorts_as_unprefixed() {
    // -webkit-transform sorts as "transform", which comes after "display".
    let source = "a { display: block; -webkit-transform: scale(1); transform: scale(1); }";
    assert!(messages(source, Syntax::Css).is_empty());
  }

  #[test]
  fn vendor_prefixed_out_of_order() {
    // -webkit-transform sorts as "transform", after "appearance".
    let source = "a { -webkit-transform: scale(1); appearance: none; }";
    let d = messages(source, Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].contains("appearance"));
  }

  #[test]
  fn each_rule_is_its_own_block() {
    let source = "a { display: block; b { color: red; } top: 0 }";
    assert!(messages(source, Syntax::Scss).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint_order() {
    use crate::testing::invalid_options;
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Any truthy value passes `Boolean`, and secondary options are ignored.
    assert!(invalid_options(NAME, serde_json::json!(["yes", { "foo": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(0)),
      ["Invalid option \"0\" for rule \"order/properties-alphabetical-order\""]
    );
  }
}
