use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::lookup;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "at-rule-descriptor-value-no-unknown";

declare_rule! {
  pub struct AtRuleDescriptorValueNoUnknown => AtRuleDescriptorValueNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown values for known descriptors in at-rules",
    default_severity: Error,
  }
}

/// `at-rule-descriptor-value-no-unknown`, which takes no options.
pub struct AtRuleDescriptorValueNoUnknownCheck;

impl AtRuleDescriptorValueNoUnknownCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

/// Known values for `font-display` in `@font-face`.
static FONT_DISPLAY_VALUES: &[&str] = &["auto", "block", "fallback", "optional", "swap"];

/// Known values for `font-style` in `@font-face` (keyword part only).
static FONT_STYLE_KEYWORDS: &[&str] = &["italic", "normal", "oblique"];

/// Known keyword values for `font-stretch` in `@font-face`.
static FONT_STRETCH_KEYWORDS: &[&str] = &[
  "condensed",
  "expanded",
  "extra-condensed",
  "extra-expanded",
  "normal",
  "semi-condensed",
  "semi-expanded",
  "ultra-condensed",
  "ultra-expanded",
];

/// Known values for `inherits` in `@property`.
static PROPERTY_INHERITS_VALUES: &[&str] = &["false", "true"];

/// Returns `true` if the value looks like a percentage (e.g. `50%`, `100%`, `62.5%`).
fn is_percentage(value: &str) -> bool {
  let trimmed = value.trim();
  if let Some(num_part) = trimmed.strip_suffix('%') {
    num_part.parse::<f64>().is_ok()
  } else {
    false
  }
}

/// Returns `true` if the value looks like an angle (e.g. `14deg`, `-20deg`).
fn is_angle(value: &str) -> bool {
  let trimmed = value.trim();
  for suffix in &["deg", "grad", "rad", "turn"] {
    if let Some(num_part) = trimmed.strip_suffix(suffix)
      && num_part.parse::<f64>().is_ok()
    {
      return true;
    }
  }
  false
}

/// Validate a descriptor value within a specific at-rule.
///
/// Returns `Some(message)` if the value is invalid, `None` if valid or not checked.
fn validate_descriptor_value(at_rule_name: &str, descriptor: &str, value: &str) -> Option<String> {
  let at_lower = at_rule_name.to_ascii_lowercase();
  let desc_lower = descriptor.to_ascii_lowercase();
  let val_trimmed = value.trim();

  match at_lower.as_str() {
    "font-face" => match desc_lower.as_str() {
      "font-display" => {
        if !lookup(FONT_DISPLAY_VALUES, val_trimmed) {
          return Some(format!(
            "Unexpected value \"{val_trimmed}\" for descriptor \"font-display\" in @font-face"
          ));
        }
      }
      "font-style" => {
        // Accept keyword alone, or `oblique <angle>` / `oblique <angle> <angle>`
        let parts: Vec<&str> = val_trimmed.split_whitespace().collect();
        if parts.is_empty() {
          return Some(
            "Unexpected empty value for descriptor \"font-style\" in @font-face".to_string(),
          );
        }
        if !lookup(FONT_STYLE_KEYWORDS, parts[0]) {
          return Some(format!(
            "Unexpected value \"{val_trimmed}\" for descriptor \"font-style\" in @font-face"
          ));
        }
        // If "oblique", additional parts must be angle values
        if parts[0].eq_ignore_ascii_case("oblique") {
          for part in &parts[1..] {
            if !is_angle(part) {
              return Some(format!(
                "Unexpected value \"{val_trimmed}\" for descriptor \"font-style\" in @font-face"
              ));
            }
          }
        }
      }
      "font-stretch" => {
        // Accept keyword or percentage(s)
        let parts: Vec<&str> = val_trimmed.split_whitespace().collect();
        if parts.is_empty() {
          return Some(
            "Unexpected empty value for descriptor \"font-stretch\" in @font-face".to_string(),
          );
        }
        // All parts must be either keywords or percentages
        let all_valid = parts
          .iter()
          .all(|p| lookup(FONT_STRETCH_KEYWORDS, p) || is_percentage(p));
        if !all_valid {
          return Some(format!(
            "Unexpected value \"{val_trimmed}\" for descriptor \"font-stretch\" in @font-face"
          ));
        }
      }
      _ => {}
    },
    "property" => {
      if desc_lower == "inherits" && !lookup(PROPERTY_INHERITS_VALUES, val_trimmed) {
        return Some(format!(
          "Unexpected value \"{val_trimmed}\" for descriptor \"inherits\" in @property"
        ));
      }
    }
    _ => {}
  }

  None
}

impl Check for AtRuleDescriptorValueNoUnknownCheck {
  /// Validates descriptor values inside `@font-face` and `@property`, the only
  /// at-rules this rule has value knowledge for.
  ///
  /// Reads the descriptors directly in the block, as Stylelint reads
  /// `decl.value`, and reports at the value as it does.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      // Only validate at-rules we have knowledge about
      let at_lower = at.name().to_ascii_lowercase();
      if !matches!(at_lower.as_str(), "font-face" | "property") || !at.is_standard_syntax() {
        continue;
      }
      for decl in at.child_decls() {
        if !decl.is_standard_syntax() {
          continue;
        }
        if let Some(msg) = validate_descriptor_value(at.name(), decl.prop(), decl.value()) {
          diags.push(
            Diagnostic::new(NAME, msg).span(Span::new(decl.value_start(), decl.value().len())),
          );
        }
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

  /// The reports for `@at-rule { <prop>: <value>; }` in CSS.
  fn check(at_rule: &str, prop: &str, value: &str) -> Vec<Diagnostic> {
    let css = format!("@{at_rule} {{ {prop}: {value}; }}");
    lint(NAME, json!(true), &css, Syntax::Css)
  }

  #[test]
  fn reports_invalid_font_display_at_the_value() {
    let d = check("font-face", "font-display", "fast");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fast"));
    assert!(d[0].message.contains("font-display"));
    assert_eq!((d[0].span.offset, d[0].span.length), (27, 4));
  }

  #[test]
  fn allows_valid_font_display() {
    for val in &["auto", "block", "swap", "fallback", "optional"] {
      assert!(
        check("font-face", "font-display", val).is_empty(),
        "Expected \"{val}\" to be valid for font-display"
      );
    }
  }

  #[test]
  fn reports_invalid_property_inherits() {
    let d = check("property --my-prop", "inherits", "yes");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("yes"));
    assert!(d[0].message.contains("inherits"));
    let scss = "@property --my-prop { inherits: yes; }";
    assert_eq!(lint(NAME, json!(true), scss, Syntax::Scss).len(), 1);
  }

  #[test]
  fn allows_valid_property_inherits() {
    assert!(check("property --my-prop", "inherits", "true").is_empty());
    assert!(check("property --my-prop", "inherits", "false").is_empty());
  }

  #[test]
  fn allows_valid_font_style() {
    for val in ["normal", "italic", "oblique 14deg"] {
      assert!(check("font-face", "font-style", val).is_empty(), "{val}");
    }
  }

  #[test]
  fn reports_invalid_font_style() {
    let d = check("font-face", "font-style", "bold");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("bold"));
  }

  #[test]
  fn allows_valid_font_stretch_keywords_and_percentages() {
    for val in ["condensed", "75%", "75% 100%"] {
      assert!(check("font-face", "font-stretch", val).is_empty(), "{val}");
    }
  }

  #[test]
  fn reports_invalid_font_stretch() {
    let d = check("font-face", "font-stretch", "wide");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("wide"));
  }

  #[test]
  fn ignores_unchecked_descriptors() {
    // `src` is a known descriptor but we don't validate its value
    assert!(check("font-face", "src", "url(anything-goes.woff2)").is_empty());
  }

  #[test]
  fn ignores_other_at_rules() {
    assert!(check("media (min-width: 768px)", "font-display", "fast").is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, json!("always")),
      ["Unexpected option value \"always\" for rule \"at-rule-descriptor-value-no-unknown\""]
    );
  }
}
