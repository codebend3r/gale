use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports when a `font-family` declaration does not end with a generic family keyword.
///
/// Equivalent to Stylelint's `font-family-no-missing-generic-family-keyword` rule.
pub struct FontFamilyNoMissingGenericFamilyKeyword;

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

impl Rule for FontFamilyNoMissingGenericFamilyKeyword {
  fn name(&self) -> &'static str {
    "font-family-no-missing-generic-family-keyword"
  }

  fn description(&self) -> &'static str {
    "Require a generic family keyword in font-family declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

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
      let has_generic = value.split(',').any(|family| {
        let trimmed = family.trim();
        let unquoted = trimmed
          .trim_start_matches(['"', '\''])
          .trim_end_matches(['"', '\''])
          .trim();
        GENERIC_FAMILIES
          .iter()
          .any(|&g| g.eq_ignore_ascii_case(unquoted))
      });

      if !has_generic {
        diagnostics.push(
          Diagnostic::new(self.name(), "Unexpected missing generic font family")
            .severity(self.default_severity())
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_missing_generic_family() {
    let rule = FontFamilyNoMissingGenericFamilyKeyword;
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
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("missing generic font family"));
  }

  #[test]
  fn ignores_when_generic_family_present() {
    let rule = FontFamilyNoMissingGenericFamilyKeyword;
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
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn accepts_system_ui() {
    let rule = FontFamilyNoMissingGenericFamilyKeyword;
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
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }
}
