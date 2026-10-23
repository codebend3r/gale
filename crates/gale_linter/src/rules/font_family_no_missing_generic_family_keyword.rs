use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::data::is_css_wide_keyword;
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
  fn check(&self, tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // The declarations of style rules, so never `@font-face`'s.
    for decl in tree.walk_decls() {
      if !decl.in_style_rule() {
        continue;
      }
      // The property in any case in CSS, as written elsewhere.
      let prop = decl.prop();
      let is_font_family = match context.syntax {
        Syntax::Css => prop.eq_ignore_ascii_case("font-family"),
        _ => prop == "font-family",
      };
      if !is_font_family {
        continue;
      }

      let value = decl.value().trim();
      if value.is_empty() {
        continue;
      }

      if is_css_wide_keyword(value) {
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
        diagnostics
          .push(Diagnostic::new(NAME, "Unexpected missing generic font family").span(decl.span()));
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_diagnostics::Span;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(rule: &dyn Check, source: &str) -> Vec<Diagnostic> {
    rule.check_source(&ctx_with_source(source))
  }

  #[test]
  fn reports_missing_generic_family() {
    let rule = configured(&FontFamilyNoMissingGenericFamilyKeyword, None);
    let diags = check(&*rule, "body { font-family: Arial, Helvetica; }");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("missing generic font family"));
    assert_eq!(diags[0].span, Span::new(7, 30));
  }

  #[test]
  fn ignores_when_generic_family_present() {
    let rule = configured(&FontFamilyNoMissingGenericFamilyKeyword, None);
    assert!(
      check(
        &*rule,
        "body { font-family: Arial, Helvetica, sans-serif; }"
      )
      .is_empty()
    );
  }

  #[test]
  fn accepts_system_ui() {
    let rule = configured(&FontFamilyNoMissingGenericFamilyKeyword, None);
    assert!(check(&*rule, "body { font-family: system-ui; }").is_empty());
  }

  #[test]
  fn skips_font_face() {
    let rule = configured(&FontFamilyNoMissingGenericFamilyKeyword, None);
    assert!(check(&*rule, "@font-face { font-family: Arial; }").is_empty());
  }

  #[test]
  fn ignore_font_families_needs_no_generic_family() {
    let rule = configured(
      &FontFamilyNoMissingGenericFamilyKeyword,
      serde_json::json!([true, { "ignoreFontFamilies": ["/^Icon/"] }]),
    );
    assert!(check(&*rule, "a { font-family: Arial, \"Icon Font\"; }").is_empty());
    assert_eq!(check(&*rule, "a { font-family: Arial; }").len(), 1);
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
