use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-block-no-duplicate-custom-properties";

declare_rule! {
  /// Disallow duplicate custom properties within declaration blocks.
  ///
  /// Equivalent to Stylelint's `declaration-block-no-duplicate-custom-properties` rule.
  pub struct DeclarationBlockNoDuplicateCustomProperties => DeclarationBlockNoDuplicateCustomPropertiesCheck {
    name: NAME,
    description: "Disallow duplicate custom properties within declaration blocks",
    default_severity: Warning,
  }
}

/// `declaration-block-no-duplicate-custom-properties` as configured.
pub struct DeclarationBlockNoDuplicateCustomPropertiesCheck {
  /// `ignoreProperties`: custom properties that may repeat.
  ignore_properties: Matchers,
}

impl DeclarationBlockNoDuplicateCustomPropertiesCheck {
  /// Stylelint's options: `true`, then `ignoreProperties`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_properties = options.matchers("ignoreProperties");
    options.finish()?;
    Ok(Self { ignore_properties })
  }
}

impl Check for DeclarationBlockNoDuplicateCustomPropertiesCheck {
  /// Flags a custom property declared more than once in the same block.
  /// Interpolated names are skipped, as they cannot be resolved.
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
      if self.ignore_properties.matches(&decl.property) {
        continue;
      }
      if !seen.insert(decl.property.clone()) {
        diagnostics.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected duplicate custom property \"{}\"", decl.property),
          )
          .span(decl.span),
        );
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
  fn reports_duplicate_custom_properties() {
    let rule = configured(&DeclarationBlockNoDuplicateCustomProperties, None);
    let node = CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![
        Declaration {
          property: "--color-primary".to_string(),
          value: "red".to_string(),
          span: Span::new(8, 22),
          important: false,
        },
        Declaration {
          property: "--color-secondary".to_string(),
          value: "blue".to_string(),
          span: Span::new(31, 24),
          important: false,
        },
        Declaration {
          property: "--color-primary".to_string(),
          value: "green".to_string(),
          span: Span::new(56, 24),
          important: false,
        },
      ],
      span: Span::new(0, 82),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate custom property \"--color-primary\""
    );
  }

  #[test]
  fn ignores_duplicate_non_custom_properties() {
    let rule = configured(&DeclarationBlockNoDuplicateCustomProperties, None);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: Span::new(4, 10),
          important: false,
        },
        Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: Span::new(15, 11),
          important: false,
        },
      ],
      span: Span::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_unique_custom_properties() {
    let rule = configured(&DeclarationBlockNoDuplicateCustomProperties, None);
    let node = CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![
        Declaration {
          property: "--color-a".to_string(),
          value: "red".to_string(),
          span: Span::new(8, 16),
          important: false,
        },
        Declaration {
          property: "--color-b".to_string(),
          value: "blue".to_string(),
          span: Span::new(25, 17),
          important: false,
        },
      ],
      span: Span::new(0, 44),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignore_properties_may_repeat() {
    let rule = configured(
      &DeclarationBlockNoDuplicateCustomProperties,
      serde_json::json!([true, { "ignoreProperties": ["/^--x/"] }]),
    );
    let decl = |property: &str| Declaration {
      property: property.to_string(),
      value: "1".to_string(),
      span: Span::new(0, 4),
      important: false,
    };
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![decl("--x-a"), decl("--x-a"), decl("--y"), decl("--y")],
      span: Span::new(0, 0),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("--y"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreProperties": "--x" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"declaration-block-no-duplicate-custom-properties\""
      ]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["--x"] }])),
      [
        "Invalid option name \"ignore\" for rule \"declaration-block-no-duplicate-custom-properties\""
      ]
    );
  }
}
