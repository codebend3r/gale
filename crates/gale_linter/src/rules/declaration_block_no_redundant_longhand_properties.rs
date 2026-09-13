use gale_css_parser::{CssNode, Declaration};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// CSS-wide basic keywords that prevent shorthand combination.
/// When any longhand has one of these values, the declarations cannot
/// be safely combined into a shorthand (Stylelint skips such declarations).
const BASIC_KEYWORDS: &[&str] = &["initial", "inherit", "revert", "revert-layer", "unset"];

/// Reports when all longhand properties of a shorthand are present and could be
/// combined (e.g. margin-top + margin-right + margin-bottom + margin-left → margin).
///
/// Equivalent to Stylelint's `declaration-block-no-redundant-longhand-properties` rule.
pub struct DeclarationBlockNoRedundantLonghandProperties;

struct ShorthandMapping {
  shorthand: &'static str,
  longhands: &'static [&'static str],
}

const SHORTHAND_MAPPINGS: &[ShorthandMapping] = &[
  ShorthandMapping {
    shorthand: "margin",
    longhands: &["margin-top", "margin-right", "margin-bottom", "margin-left"],
  },
  ShorthandMapping {
    shorthand: "padding",
    longhands: &[
      "padding-top",
      "padding-right",
      "padding-bottom",
      "padding-left",
    ],
  },
  ShorthandMapping {
    shorthand: "border-color",
    longhands: &[
      "border-top-color",
      "border-right-color",
      "border-bottom-color",
      "border-left-color",
    ],
  },
  ShorthandMapping {
    shorthand: "border-style",
    longhands: &[
      "border-top-style",
      "border-right-style",
      "border-bottom-style",
      "border-left-style",
    ],
  },
  ShorthandMapping {
    shorthand: "border-width",
    longhands: &[
      "border-top-width",
      "border-right-width",
      "border-bottom-width",
      "border-left-width",
    ],
  },
  ShorthandMapping {
    shorthand: "overflow",
    longhands: &["overflow-x", "overflow-y"],
  },
  ShorthandMapping {
    shorthand: "outline",
    longhands: &["outline-color", "outline-style", "outline-width"],
  },
  ShorthandMapping {
    shorthand: "flex",
    longhands: &["flex-grow", "flex-shrink", "flex-basis"],
  },
  ShorthandMapping {
    shorthand: "grid-gap",
    longhands: &["grid-row-gap", "grid-column-gap"],
  },
  ShorthandMapping {
    shorthand: "gap",
    longhands: &["row-gap", "column-gap"],
  },
  ShorthandMapping {
    shorthand: "inset",
    longhands: &["top", "right", "bottom", "left"],
  },
  ShorthandMapping {
    shorthand: "transition",
    longhands: &[
      "transition-property",
      "transition-duration",
      "transition-timing-function",
      "transition-delay",
    ],
  },
  ShorthandMapping {
    shorthand: "animation",
    longhands: &[
      "animation-name",
      "animation-duration",
      "animation-timing-function",
      "animation-delay",
      "animation-iteration-count",
      "animation-direction",
      "animation-fill-mode",
      "animation-play-state",
    ],
  },
  ShorthandMapping {
    shorthand: "list-style",
    longhands: &["list-style-type", "list-style-position", "list-style-image"],
  },
  ShorthandMapping {
    shorthand: "border-radius",
    longhands: &[
      "border-top-left-radius",
      "border-top-right-radius",
      "border-bottom-right-radius",
      "border-bottom-left-radius",
    ],
  },
  ShorthandMapping {
    shorthand: "border-top",
    longhands: &["border-top-width", "border-top-style", "border-top-color"],
  },
  ShorthandMapping {
    shorthand: "border-bottom",
    longhands: &[
      "border-bottom-width",
      "border-bottom-style",
      "border-bottom-color",
    ],
  },
  ShorthandMapping {
    shorthand: "border-left",
    longhands: &[
      "border-left-width",
      "border-left-style",
      "border-left-color",
    ],
  },
  ShorthandMapping {
    shorthand: "border-right",
    longhands: &[
      "border-right-width",
      "border-right-style",
      "border-right-color",
    ],
  },
  ShorthandMapping {
    shorthand: "flex-flow",
    longhands: &["flex-direction", "flex-wrap"],
  },
  ShorthandMapping {
    shorthand: "padding-inline",
    longhands: &["padding-inline-start", "padding-inline-end"],
  },
  ShorthandMapping {
    shorthand: "padding-block",
    longhands: &["padding-block-start", "padding-block-end"],
  },
  ShorthandMapping {
    shorthand: "margin-inline",
    longhands: &["margin-inline-start", "margin-inline-end"],
  },
  ShorthandMapping {
    shorthand: "margin-block",
    longhands: &["margin-block-start", "margin-block-end"],
  },
  ShorthandMapping {
    shorthand: "inset-inline",
    longhands: &["inset-inline-start", "inset-inline-end"],
  },
  ShorthandMapping {
    shorthand: "inset-block",
    longhands: &["inset-block-start", "inset-block-end"],
  },
  ShorthandMapping {
    shorthand: "text-decoration",
    longhands: &[
      "text-decoration-line",
      "text-decoration-style",
      "text-decoration-color",
    ],
  },
];

impl Rule for DeclarationBlockNoRedundantLonghandProperties {
  fn name(&self) -> &'static str {
    "declaration-block-no-redundant-longhand-properties"
  }

  fn description(&self) -> &'static str {
    "Disallow longhand properties that can be combined into one shorthand property"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    match node {
      CssNode::Style(rule) => check_declarations(self, &rule.declarations, rule.span),
      CssNode::AtRule(at_rule) => {
        // Also check bare declarations inside SCSS at-rule bodies (e.g. @mixin).
        let decls: Vec<&Declaration> = at_rule
          .children
          .iter()
          .filter_map(|c| {
            if let CssNode::Declaration(d) = c {
              Some(d)
            } else {
              None
            }
          })
          .collect();
        if decls.is_empty() {
          return vec![];
        }
        check_declarations_slice(self, &decls, at_rule.span)
      }
      _ => vec![],
    }
  }
}

fn check_declarations(
  rule_impl: &DeclarationBlockNoRedundantLonghandProperties,
  declarations: &[Declaration],
  fallback_span: gale_css_parser::Span,
) -> Vec<Diagnostic> {
  let refs: Vec<&Declaration> = declarations.iter().collect();
  check_declarations_slice(rule_impl, &refs, fallback_span)
}

fn check_declarations_slice(
  rule_impl: &DeclarationBlockNoRedundantLonghandProperties,
  declarations: &[&Declaration],
  fallback_span: gale_css_parser::Span,
) -> Vec<Diagnostic> {
  // Build the set of effective longhand properties, skipping any declarations
  // whose value is a CSS-wide basic keyword (initial, inherit, unset, etc.)
  // because those cannot be safely combined into a shorthand.
  let props: Vec<String> = declarations
    .iter()
    .filter(|d| {
      let val = d.value.trim().to_ascii_lowercase();
      !BASIC_KEYWORDS.contains(&val.as_str())
    })
    .map(|d| d.property.to_ascii_lowercase())
    .collect();

  let mut diags = Vec::new();

  for mapping in SHORTHAND_MAPPINGS {
    let all_present = mapping
      .longhands
      .iter()
      .all(|lh| props.contains(&lh.to_string()));
    if all_present {
      // Report at the last matching longhand declaration, matching Stylelint behavior.
      let last_longhand_span = declarations
        .iter()
        .filter(|d| {
          mapping
            .longhands
            .contains(&d.property.to_ascii_lowercase().as_str())
        })
        .last()
        .map(|d| Span::new(d.span.offset, d.span.length))
        .unwrap_or_else(|| Span::new(fallback_span.offset, fallback_span.length));
      diags.push(
        Diagnostic::new(
          rule_impl.name(),
          format!("Expected shorthand property \"{}\"", mapping.shorthand),
        )
        .severity(rule_impl.default_severity())
        .span(last_longhand_span),
      );
    }
  }

  diags
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_props(props: &[(&str, &str)]) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: props
        .iter()
        .map(|(p, v)| Declaration {
          property: p.to_string(),
          value: v.to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        })
        .collect(),
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_all_margin_longhands() {
    let node = style_with_props(&[
      ("margin-top", "1px"),
      ("margin-right", "2px"),
      ("margin-bottom", "3px"),
      ("margin-left", "4px"),
    ]);
    let d = DeclarationBlockNoRedundantLonghandProperties.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("margin"));
  }

  #[test]
  fn allows_incomplete_longhands() {
    let node = style_with_props(&[("margin-top", "1px"), ("margin-bottom", "3px")]);
    assert!(
      DeclarationBlockNoRedundantLonghandProperties
        .check(&node, &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_overflow_longhands() {
    let node = style_with_props(&[("overflow-x", "hidden"), ("overflow-y", "auto")]);
    let d = DeclarationBlockNoRedundantLonghandProperties.check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("overflow"));
  }
}
