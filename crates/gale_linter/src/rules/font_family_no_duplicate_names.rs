use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "font-family-no-duplicate-names";

declare_rule! {
  /// Disallow duplicate font family names within `font-family` or `font` declarations.
  ///
  /// Equivalent to Stylelint's `font-family-no-duplicate-names` rule.
  pub struct FontFamilyNoDuplicateNames => FontFamilyNoDuplicateNamesCheck {
    name: NAME,
    description: "Disallow duplicate names in font-family declarations",
    default_severity: Warning,
  }
}

/// `font-family-no-duplicate-names` as configured.
pub struct FontFamilyNoDuplicateNamesCheck {
  /// `ignoreFontFamilyNames`: names (unquoted, as written) that may repeat.
  ignore_font_family_names: Matchers,
}

impl FontFamilyNoDuplicateNamesCheck {
  /// Stylelint's options: `true`, then `ignoreFontFamilyNames`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_font_family_names = options.matchers("ignoreFontFamilyNames");
    options.finish()?;
    Ok(Self {
      ignore_font_family_names,
    })
  }
}

impl Check for FontFamilyNoDuplicateNamesCheck {
  /// Flags a family name listed twice in the same `font-family` or `font` value.
  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let mut diagnostics = Vec::new();

    for decl in &rule.declarations {
      let prop_lower = decl.property.to_ascii_lowercase();
      if prop_lower != "font-family" && prop_lower != "font" {
        continue;
      }

      // For the `font` shorthand, the font-family portion comes after the
      // last `/` (line-height separator) or after size values. A simple
      // heuristic: split on commas, which only appear in the font-family
      // part of the shorthand. If there are no commas, there's at most one
      // family name so no duplicates are possible.
      let families: Vec<&str> = decl.value.split(',').collect();
      if families.len() <= 1 {
        continue;
      }

      let mut seen = HashSet::new();
      for family in &families {
        let unquoted = family.trim().trim_matches(|c| c == '"' || c == '\'');
        if self.ignore_font_family_names.matches(unquoted.trim()) {
          continue;
        }
        let normalized = unquoted.to_ascii_lowercase();

        if normalized.is_empty() {
          continue;
        }

        if !seen.insert(normalized.clone()) {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected duplicate font family name \"{}\"", normalized),
            )
            .span(decl.span),
          );
        }
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule};
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options};

  #[test]
  fn reports_duplicate_font_family_names() {
    let rule = configured(&FontFamilyNoDuplicateNames, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "Arial, Helvetica, Arial".to_string(),
        span: Span::new(4, 36),
        important: false,
      }],
      span: Span::new(0, 42),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate font family name \"arial\""
    );
  }

  #[test]
  fn ignores_unique_font_family_names() {
    let rule = configured(&FontFamilyNoDuplicateNames, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "Arial, Helvetica, sans-serif".to_string(),
        span: Span::new(4, 41),
        important: false,
      }],
      span: Span::new(0, 47),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn detects_duplicates_with_quotes() {
    let rule = configured(&FontFamilyNoDuplicateNames, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "\"Arial\", Arial, sans-serif".to_string(),
        span: Span::new(4, 39),
        important: false,
      }],
      span: Span::new(0, 45),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn ignore_font_family_names_may_repeat() {
    let rule = configured(
      &FontFamilyNoDuplicateNames,
      serde_json::json!([true, { "ignoreFontFamilyNames": ["/^My /", "monospace"] }]),
    );
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "\"My Font\", My Font, monospace, monospace, serif, serif".to_string(),
        span: Span::new(0, 10),
        important: false,
      }],
      span: Span::new(0, 0),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("serif"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFontFamilyNames": "monospace" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"font-family-no-duplicate-names\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFontFamilies": ["x"] }])
      ),
      ["Invalid option name \"ignoreFontFamilies\" for rule \"font-family-no-duplicate-names\""]
    );
  }
}
