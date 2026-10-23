use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "at-rule-descriptor-no-unknown";

declare_rule! {
  pub struct AtRuleDescriptorNoUnknown => AtRuleDescriptorNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown descriptors within at-rules",
    default_severity: Error,
  }
}

/// `at-rule-descriptor-no-unknown`, which takes no options.
pub struct AtRuleDescriptorNoUnknownCheck;

impl AtRuleDescriptorNoUnknownCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

/// Known descriptors for `@font-face` (sorted).
static FONT_FACE_DESCRIPTORS: &[&str] = &[
  "ascent-override",
  "descent-override",
  "font-display",
  "font-family",
  "font-feature-settings",
  "font-stretch",
  "font-style",
  "font-variation-settings",
  "font-weight",
  "line-gap-override",
  "size-adjust",
  "src",
  "unicode-range",
];

/// Known descriptors for `@counter-style` (sorted).
static COUNTER_STYLE_DESCRIPTORS: &[&str] = &[
  "additive-symbols",
  "fallback",
  "negative",
  "pad",
  "prefix",
  "range",
  "speak-as",
  "suffix",
  "symbols",
  "system",
];

/// Known descriptors for `@property` (sorted).
static PROPERTY_DESCRIPTORS: &[&str] = &["inherits", "initial-value", "syntax"];

/// The descriptor table for an at-rule, or `None` if it takes no descriptors.
fn known_descriptors_for_at_rule(name: &str) -> Option<&'static [&'static str]> {
  match name.to_ascii_lowercase().as_str() {
    "font-face" => Some(FONT_FACE_DESCRIPTORS),
    "counter-style" => Some(COUNTER_STYLE_DESCRIPTORS),
    "property" => Some(PROPERTY_DESCRIPTORS),
    _ => None,
  }
}

/// Case-insensitive binary search over a sorted descriptor table.
fn is_known_descriptor(descriptors: &[&str], name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  descriptors.binary_search(&lower.as_str()).is_ok()
}

impl Check for AtRuleDescriptorNoUnknownCheck {
  /// Flags declarations directly inside a descriptor-taking at-rule whose
  /// property is not a known descriptor, at the property as Stylelint
  /// reports it.  Vendor-prefixed names are skipped, and so is what
  /// Stylelint's `isStandardSyntaxAtRule` / `isStandardSyntaxDeclaration`
  /// reject (Sass variables among them).
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      let Some(known) = known_descriptors_for_at_rule(at.name()) else {
        continue;
      };
      if !at.is_standard_syntax() {
        continue;
      }
      for decl in at.child_decls() {
        let prop = decl.prop();
        if prop.starts_with('-') || !decl.is_standard_syntax() || is_known_descriptor(known, prop) {
          continue;
        }
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected unknown descriptor \"{prop}\" in @{}", at.name()),
          )
          .span(Span::new(decl.start, prop.len())),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  /// The reports for `@at-rule <params> { <descriptors> }` in `syntax`.
  fn check(at_rule: &str, descriptors: &[(&str, &str)], syntax: Syntax) -> Vec<Diagnostic> {
    let body: String = descriptors
      .iter()
      .map(|(prop, value)| format!(" {prop}: {value};"))
      .collect();
    lint(
      NAME,
      json!(true),
      &format!("@{at_rule} {{{body} }}"),
      syntax,
    )
  }

  #[test]
  fn reports_unknown_font_face_descriptor_at_its_name() {
    for syntax in [Syntax::Css, Syntax::Scss] {
      let d = check(
        "font-face",
        &[("font-family", "MyFont"), ("unknown-thing", "value")],
        syntax,
      );
      assert_eq!(d.len(), 1);
      assert!(d[0].message.contains("unknown-thing"));
      assert!(d[0].message.contains("@font-face"));
      assert_eq!((d[0].span.offset, d[0].span.length), (34, 13));
    }
  }

  #[test]
  fn allows_known_font_face_descriptors() {
    let descriptors = [
      ("font-family", "MyFont"),
      ("src", "url(font.woff2)"),
      ("font-weight", "400"),
      ("FONT-DISPLAY", "swap"),
    ];
    assert!(check("font-face", &descriptors, Syntax::Css).is_empty());
  }

  #[test]
  fn reports_unknown_counter_style_descriptor() {
    let descriptors = [("system", "cyclic"), ("bad-desc", "x")];
    let d = check("counter-style my-counter", &descriptors, Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bad-desc"));
  }

  #[test]
  fn allows_known_counter_style_descriptors() {
    let descriptors = [("system", "cyclic"), ("symbols", "A B C")];
    assert!(check("counter-style my-counter", &descriptors, Syntax::Css).is_empty());
  }

  #[test]
  fn reports_unknown_property_descriptor() {
    let descriptors = [("syntax", "\"<color>\""), ("bad", "value")];
    let d = check("property --my-prop", &descriptors, Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bad"));
  }

  #[test]
  fn allows_known_property_descriptors() {
    let descriptors = [
      ("syntax", "\"<color>\""),
      ("inherits", "false"),
      ("initial-value", "red"),
    ];
    assert!(check("property --my-prop", &descriptors, Syntax::Css).is_empty());
  }

  #[test]
  fn ignores_other_at_rules() {
    let descriptors = [("color", "red")];
    assert!(check("media (min-width: 768px)", &descriptors, Syntax::Css).is_empty());
  }

  #[test]
  fn skips_vendor_prefixed_and_non_standard_descriptors() {
    let descriptors = [
      ("font-family", "MyFont"),
      ("-webkit-font-smoothing", "auto"),
      ("$weight", "400"),
    ];
    assert!(check("font-face", &descriptors, Syntax::Scss).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, json!("always")),
      ["Unexpected option value \"always\" for rule \"at-rule-descriptor-no-unknown\""]
    );
  }
}
