use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-block-no-shorthand-property-overrides";

declare_rule! {
  /// Reports when a shorthand property overrides a previously declared longhand property
  /// in the same declaration block.
  ///
  /// Equivalent to Stylelint's `declaration-block-no-shorthand-property-overrides` rule.
  pub struct DeclarationBlockNoShorthandPropertyOverrides => DeclarationBlockNoShorthandPropertyOverridesCheck {
    name: NAME,
    description: "Disallow shorthand properties that override related longhand properties",
    default_severity: Warning,
  }
}

/// `declaration-block-no-shorthand-property-overrides`, which takes no options.
pub struct DeclarationBlockNoShorthandPropertyOverridesCheck;

impl DeclarationBlockNoShorthandPropertyOverridesCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

/// The shorthands this rule knows, each with the longhands it resets.
///
/// Narrower than Stylelint's `longhandSubPropertiesOfShorthandProperties`
/// (which `declaration-block-no-redundant-longhand-properties` follows): this
/// is the subset gale's version of the rule has always checked, so the two
/// tables are kept apart.
const SHORTHAND_LONGHANDS: &[(&str, &[&str])] = &[
  (
    "margin",
    &["margin-top", "margin-right", "margin-bottom", "margin-left"],
  ),
  (
    "padding",
    &[
      "padding-top",
      "padding-right",
      "padding-bottom",
      "padding-left",
    ],
  ),
  (
    "border",
    &[
      "border-color",
      "border-style",
      "border-width",
      "border-top-color",
      "border-top-style",
      "border-top-width",
      "border-right-color",
      "border-right-style",
      "border-right-width",
      "border-bottom-color",
      "border-bottom-style",
      "border-bottom-width",
      "border-left-color",
      "border-left-style",
      "border-left-width",
    ],
  ),
  (
    "background",
    &[
      "background-color",
      "background-image",
      "background-position",
      "background-size",
      "background-repeat",
      "background-origin",
      "background-clip",
      "background-attachment",
    ],
  ),
  (
    "font",
    &[
      "font-style",
      "font-variant",
      "font-weight",
      "font-size",
      "line-height",
      "font-family",
    ],
  ),
  ("flex", &["flex-grow", "flex-shrink", "flex-basis"]),
  (
    "grid",
    &[
      "grid-template-rows",
      "grid-template-columns",
      "grid-template-areas",
      "grid-auto-rows",
      "grid-auto-columns",
      "grid-auto-flow",
    ],
  ),
  (
    "outline",
    &["outline-color", "outline-style", "outline-width"],
  ),
  ("overflow", &["overflow-x", "overflow-y"]),
  (
    "transition",
    &[
      "transition-property",
      "transition-duration",
      "transition-timing-function",
      "transition-delay",
    ],
  ),
  (
    "animation",
    &[
      "animation-name",
      "animation-duration",
      "animation-timing-function",
      "animation-delay",
      "animation-iteration-count",
      "animation-direction",
      "animation-fill-mode",
      "animation-play-state",
    ],
  ),
  (
    "list-style",
    &["list-style-type", "list-style-position", "list-style-image"],
  ),
  (
    "text-decoration",
    &[
      "text-decoration-color",
      "text-decoration-style",
      "text-decoration-line",
    ],
  ),
];

/// The longhands `shorthand` resets, if this rule knows it.
fn longhands_for(shorthand: &str) -> Option<&'static [&'static str]> {
  SHORTHAND_LONGHANDS
    .iter()
    .find(|(name, _)| *name == shorthand)
    .map(|(_, longhands)| *longhands)
}

impl Check for DeclarationBlockNoShorthandPropertyOverridesCheck {
  /// Flags a shorthand that appears after (and so silently overrides) a
  /// longhand it covers.  Names match ignoring case and print as written,
  /// as Stylelint's do.
  fn check(&self, tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for rule in tree.walk_rules() {
      // The longhands seen so far: lower-cased, then as written.
      let mut seen_longhands: Vec<(String, &str)> = Vec::new();
      for decl in rule.child_decls() {
        let written = decl.prop();
        let prop = written.to_ascii_lowercase();

        if let Some(longhands) = longhands_for(&prop)
          && let Some(longhand) = longhands.iter().find_map(|longhand| {
            seen_longhands
              .iter()
              .find(|(seen, _)| seen == longhand)
              .map(|(_, written)| *written)
          })
        {
          // Only the first overridden longhand (in the table's order) is
          // reported.
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected shorthand \"{written}\" after \"{longhand}\""),
            )
            .span(decl.span()),
          );
        }

        if !seen_longhands.iter().any(|(seen, _)| *seen == prop)
          && SHORTHAND_LONGHANDS
            .iter()
            .any(|(_, longhands)| longhands.contains(&prop.as_str()))
        {
          seen_longhands.push((prop, written));
        }
      }
    }
    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The messages reported for `source` in `syntax`.
  fn messages(source: &str, syntax: Syntax) -> Vec<String> {
    configured(&DeclarationBlockNoShorthandPropertyOverrides, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn reports_shorthand_overriding_longhand() {
    assert_eq!(
      messages("a { border-color: red; border: 1px solid; }", Syntax::Css),
      ["Unexpected shorthand \"border\" after \"border-color\""]
    );
  }

  #[test]
  fn ignores_longhand_after_shorthand() {
    assert!(messages("a { margin: 10px; margin-top: 20px; }", Syntax::Css).is_empty());
  }

  #[test]
  fn reports_font_shorthand_overriding_font_weight() {
    assert_eq!(
      messages(
        "h1 { font-weight: bold; font: 16px/1.5 Arial; }",
        Syntax::Css
      ),
      ["Unexpected shorthand \"font\" after \"font-weight\""]
    );
  }

  #[test]
  fn reports_the_first_overridden_longhand_once() {
    assert_eq!(
      messages(
        "a { margin-left: 0; margin-top: 0; margin: 1px; }",
        Syntax::Css
      ),
      ["Unexpected shorthand \"margin\" after \"margin-top\""]
    );
  }

  #[test]
  fn names_match_ignoring_case_and_print_as_written() {
    let expected = ["Unexpected shorthand \"MARGIN\" after \"Margin-Top\""];
    let source = "a { Margin-Top: 0; MARGIN: 1px; }";
    assert_eq!(messages(source, Syntax::Css), expected);
    assert_eq!(messages(source, Syntax::Scss), expected);
  }

  #[test]
  fn each_rule_is_its_own_block() {
    assert!(messages("a { margin-top: 0; b { margin: 0; } }", Syntax::Scss).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"declaration-block-no-shorthand-property-overrides\""
      ]
    );
  }
}
