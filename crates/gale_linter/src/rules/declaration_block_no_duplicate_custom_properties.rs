use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate custom properties within declaration blocks.
///
/// Equivalent to Stylelint's `declaration-block-no-duplicate-custom-properties` rule.
pub struct DeclarationBlockNoDuplicateCustomProperties;

impl Rule for DeclarationBlockNoDuplicateCustomProperties {
  fn name(&self) -> &'static str {
    "declaration-block-no-duplicate-custom-properties"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate custom properties within declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let is_preprocessor = matches!(
      context.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    );

    let mut seen = HashSet::new();
    let mut diagnostics = Vec::new();

    for decl in &rule.declarations {
      if !decl.property.starts_with("--") {
        continue;
      }
      // Skip properties with SCSS/Less interpolation — we can't resolve the
      // actual name, so duplicate detection would produce false positives.
      if is_preprocessor && decl.property.contains("#{") {
        continue;
      }
      if !seen.insert(decl.property.clone()) {
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected duplicate custom property \"{}\"", decl.property),
          )
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
  fn reports_duplicate_custom_properties() {
    let rule = DeclarationBlockNoDuplicateCustomProperties;
    let node = CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![
        Declaration {
          property: "--color-primary".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(8, 22),
          important: false,
        },
        Declaration {
          property: "--color-secondary".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(31, 24),
          important: false,
        },
        Declaration {
          property: "--color-primary".to_string(),
          value: "green".to_string(),
          span: ParserSpan::new(56, 24),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 82),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate custom property \"--color-primary\""
    );
  }

  #[test]
  fn ignores_duplicate_non_custom_properties() {
    let rule = DeclarationBlockNoDuplicateCustomProperties;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 10),
          important: false,
        },
        Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(15, 11),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_unique_custom_properties() {
    let rule = DeclarationBlockNoDuplicateCustomProperties;
    let node = CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![
        Declaration {
          property: "--color-a".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(8, 16),
          important: false,
        },
        Declaration {
          property: "--color-b".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(25, 17),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 44),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }
}
