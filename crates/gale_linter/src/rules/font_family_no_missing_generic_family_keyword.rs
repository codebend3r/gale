use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "font-family-no-missing-generic-family-keyword";

declare_rule! {
  /// Reports when a `font-family` declaration does not end with a generic family keyword.
  ///
  /// Equivalent to Stylelint's `font-family-no-missing-generic-family-keyword` rule.
  pub struct FontFamilyNoMissingGenericFamilyKeyword => FontFamilyNoMissingGenericFamilyKeywordCheck {
    name: NAME,
    description: "Require a generic family keyword in font-family declarations",
    default_severity: Warning,
  }
}

/// `font-family-no-missing-generic-family-keyword` as configured.
pub struct FontFamilyNoMissingGenericFamilyKeywordCheck {
  /// `ignoreFontFamilies`: a declaration naming one of these families
  /// (unquoted) needs no generic family.
  ignore_font_families: Matchers,
}

impl FontFamilyNoMissingGenericFamilyKeywordCheck {
  /// Stylelint's options: `true`, then `ignoreFontFamilies`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_font_families = options.matchers("ignoreFontFamilies");
    options.finish()?;
    Ok(Self {
      ignore_font_families,
    })
  }
}

const GENERIC_FAMILIES: &[&str] = &[
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "emoji",
  "math",
  "fangsong",
];

impl Check for FontFamilyNoMissingGenericFamilyKeywordCheck {
  /// Flags a `font-family` whose last entry is not a generic family keyword.
  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let style = match node {
      CssNode::Style(s) => s,
      _ => return vec![],
    };

    let mut diagnostics = Vec::new();

    for decl in &style.declarations {
      if decl.property != "font-family" {
        continue;
      }

      let value = decl.value.trim();
      if value.is_empty() {
        continue;
      }

      // Skip CSS-wide keywords (inherit, initial, unset, revert, revert-layer)
      let lower = value.to_ascii_lowercase();
      if matches!(
        lower.as_str(),
        "inherit" | "initial" | "unset" | "revert" | "revert-layer"
      ) {
        continue;
      }

      // Skip if the value contains var() — the custom property may
      // resolve to a value that includes a generic family.
      // Also match SCSS interpolation like var(--#{$prefix}...)
      if value.contains("var(") || value.contains("#{") || value.contains('$') {
        continue;
      }

      // Skip non-standard-syntax values: SCSS function calls (the
      // entire value is a single function invocation like
      // `font-family('sans')` or `type.font('mono')`).
      if value.ends_with(')') && !value.contains(',') {
        // If the value looks like a single function call, skip it.
        if let Some(paren_pos) = value.find('(') {
          let func_name = &value[..paren_pos];
          if func_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
          {
            continue;
          }
        }
      }

      // Split by comma and check if ANY entry is a generic family keyword.
      // Stylelint accepts the declaration if any family in the list is generic,
      // not just the last one.
      let unquoted = |family: &'_ str| {
        family
          .trim()
          .trim_start_matches(['"', '\''])
          .trim_end_matches(['"', '\''])
          .trim()
          .to_string()
      };
      let has_generic = value.split(',').any(|family| {
        let unquoted = unquoted(family);
        GENERIC_FAMILIES
          .iter()
          .any(|&g| g.eq_ignore_ascii_case(&unquoted))
      });
      let ignored = !self.ignore_font_families.is_empty()
        && value
          .split(',')
          .any(|family| self.ignore_font_families.matches(&unquoted(family)));

      if !has_generic && !ignored {
        diagnostics.push(
          Diagnostic::new(NAME, "Unexpected missing generic font family")
            .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  #[test]
  fn reports_missing_generic_family() {
    let rule = configured(&FontFamilyNoMissingGenericFamilyKeyword, None);
    let node = CssNode::Style(StyleRule {
      selector: "body".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "Arial, Helvetica".to_string(),
        span: ParserSpan::new(6, 28),
        important: false,
      }],
      span: ParserSpan::new(0, 36),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("missing generic font family"));
  }

  #[test]
  fn ignores_when_generic_family_present() {
    let rule = configured(&FontFamilyNoMissingGenericFamilyKeyword, None);
    let node = CssNode::Style(StyleRule {
      selector: "body".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "Arial, Helvetica, sans-serif".to_string(),
        span: ParserSpan::new(6, 40),
        important: false,
      }],
      span: ParserSpan::new(0, 48),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn accepts_system_ui() {
    let rule = configured(&FontFamilyNoMissingGenericFamilyKeyword, None);
    let node = CssNode::Style(StyleRule {
      selector: "body".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "system-ui".to_string(),
        span: ParserSpan::new(6, 20),
        important: false,
      }],
      span: ParserSpan::new(0, 28),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignore_font_families_needs_no_generic_family() {
    let rule = configured(
      &FontFamilyNoMissingGenericFamilyKeyword,
      serde_json::json!([true, { "ignoreFontFamilies": ["/^Icon/"] }]),
    );
    let node = |value: &str| {
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![Declaration {
          property: "font-family".to_string(),
          value: value.to_string(),
          span: ParserSpan::new(0, 10),
          important: false,
        }],
        span: ParserSpan::new(0, 0),
        ..Default::default()
      })
    };
    assert!(rule.check(&node("Arial, \"Icon Font\""), &ctx()).is_empty());
    assert_eq!(rule.check(&node("Arial"), &ctx()).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFontFamilies": "Icons" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"font-family-no-missing-generic-family-keyword\""
      ]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFontFamilies": [false] }])
      ),
      [
        "Invalid value \"false\" for option \"ignoreFontFamilies\" of rule \"font-family-no-missing-generic-family-keyword\""
      ]
    );
  }
}
