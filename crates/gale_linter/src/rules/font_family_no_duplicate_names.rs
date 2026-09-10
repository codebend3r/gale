use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate font family names within `font-family` or `font` declarations.
///
/// Equivalent to Stylelint's `font-family-no-duplicate-names` rule.
pub struct FontFamilyNoDuplicateNames;

impl Rule for FontFamilyNoDuplicateNames {
  fn name(&self) -> &'static str {
    "font-family-no-duplicate-names"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate names in font-family declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

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
        let normalized = family
          .trim()
          .trim_matches(|c| c == '"' || c == '\'')
          .to_ascii_lowercase();

        if normalized.is_empty() {
          continue;
        }

        if !seen.insert(normalized.clone()) {
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Unexpected duplicate font family name \"{}\"", normalized),
            )
            .severity(self.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
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
  fn reports_duplicate_font_family_names() {
    let rule = FontFamilyNoDuplicateNames;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "Arial, Helvetica, Arial".to_string(),
        span: ParserSpan::new(4, 36),
        important: false,
      }],
      span: ParserSpan::new(0, 42),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate font family name \"arial\""
    );
  }

  #[test]
  fn ignores_unique_font_family_names() {
    let rule = FontFamilyNoDuplicateNames;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "Arial, Helvetica, sans-serif".to_string(),
        span: ParserSpan::new(4, 41),
        important: false,
      }],
      span: ParserSpan::new(0, 47),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn detects_duplicates_with_quotes() {
    let rule = FontFamilyNoDuplicateNames;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "font-family".to_string(),
        value: "\"Arial\", Arial, sans-serif".to_string(),
        span: ParserSpan::new(4, 39),
        important: false,
      }],
      span: ParserSpan::new(0, 45),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
  }
}
