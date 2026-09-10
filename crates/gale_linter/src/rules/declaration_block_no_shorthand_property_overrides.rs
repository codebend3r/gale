use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports when a shorthand property overrides a previously declared longhand property
/// in the same declaration block.
///
/// Equivalent to Stylelint's `declaration-block-no-shorthand-property-overrides` rule.
pub struct DeclarationBlockNoShorthandPropertyOverrides;

/// Returns the list of longhand properties that a given shorthand expands to.
fn longhands_for(shorthand: &str) -> Option<&'static [&'static str]> {
  match shorthand {
    "margin" => Some(&["margin-top", "margin-right", "margin-bottom", "margin-left"]),
    "padding" => Some(&[
      "padding-top",
      "padding-right",
      "padding-bottom",
      "padding-left",
    ]),
    "border" => Some(&[
      "border-color",
      "border-style",
      "border-width",
      "border-top-color",
      "border-top-style",
      "border-top-width",
      "border-right-color",
      "border-right-style",
      "border-right-width",
      "border-bottom-color",
      "border-bottom-style",
      "border-bottom-width",
      "border-left-color",
      "border-left-style",
      "border-left-width",
    ]),
    "background" => Some(&[
      "background-color",
      "background-image",
      "background-position",
      "background-size",
      "background-repeat",
      "background-origin",
      "background-clip",
      "background-attachment",
    ]),
    "font" => Some(&[
      "font-style",
      "font-variant",
      "font-weight",
      "font-size",
      "line-height",
      "font-family",
    ]),
    "flex" => Some(&["flex-grow", "flex-shrink", "flex-basis"]),
    "grid" => Some(&[
      "grid-template-rows",
      "grid-template-columns",
      "grid-template-areas",
      "grid-auto-rows",
      "grid-auto-columns",
      "grid-auto-flow",
    ]),
    "outline" => Some(&["outline-color", "outline-style", "outline-width"]),
    "overflow" => Some(&["overflow-x", "overflow-y"]),
    "transition" => Some(&[
      "transition-property",
      "transition-duration",
      "transition-timing-function",
      "transition-delay",
    ]),
    "animation" => Some(&[
      "animation-name",
      "animation-duration",
      "animation-timing-function",
      "animation-delay",
      "animation-iteration-count",
      "animation-direction",
      "animation-fill-mode",
      "animation-play-state",
    ]),
    "list-style" => Some(&["list-style-type", "list-style-position", "list-style-image"]),
    "text-decoration" => Some(&[
      "text-decoration-color",
      "text-decoration-style",
      "text-decoration-line",
    ]),
    _ => None,
  }
}

/// All shorthand property names we know about.
const SHORTHANDS: &[&str] = &[
  "margin",
  "padding",
  "border",
  "background",
  "font",
  "flex",
  "grid",
  "outline",
  "overflow",
  "transition",
  "animation",
  "list-style",
  "text-decoration",
];

impl Rule for DeclarationBlockNoShorthandPropertyOverrides {
  fn name(&self) -> &'static str {
    "declaration-block-no-shorthand-property-overrides"
  }

  fn description(&self) -> &'static str {
    "Disallow shorthand properties that override related longhand properties"
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
    // Track longhand properties we've seen so far (property name).
    let mut seen_longhands: Vec<&str> = Vec::new();

    for decl in &style.declarations {
      let prop = decl.property.as_str();

      // Check if this property is a shorthand that would override a seen longhand.
      if let Some(longhands) = longhands_for(prop) {
        for &longhand in longhands {
          if seen_longhands.contains(&longhand) {
            diagnostics.push(
              Diagnostic::new(
                self.name(),
                format!("Unexpected shorthand \"{}\" after \"{}\"", prop, longhand,),
              )
              .severity(self.default_severity())
              .span(Span::new(decl.span.offset, decl.span.length)),
            );
            // Only report the first overridden longhand per shorthand occurrence.
            break;
          }
        }
      }

      // Check if this property is a longhand of any shorthand.
      for &shorthand in SHORTHANDS {
        if let Some(longhands) = longhands_for(shorthand)
          && longhands.contains(&prop)
          && !seen_longhands.contains(&prop)
        {
          seen_longhands.push(prop);
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
  fn reports_shorthand_overriding_longhand() {
    let rule = DeclarationBlockNoShorthandPropertyOverrides;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "border-color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 16),
          important: false,
        },
        Declaration {
          property: "border".to_string(),
          value: "1px solid".to_string(),
          span: ParserSpan::new(22, 18),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 42),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("\"border\""));
    assert!(diags[0].message.contains("\"border-color\""));
  }

  #[test]
  fn ignores_longhand_after_shorthand() {
    let rule = DeclarationBlockNoShorthandPropertyOverrides;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "margin".to_string(),
          value: "10px".to_string(),
          span: ParserSpan::new(4, 14),
          important: false,
        },
        Declaration {
          property: "margin-top".to_string(),
          value: "20px".to_string(),
          span: ParserSpan::new(20, 16),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 38),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn reports_font_shorthand_overriding_font_weight() {
    let rule = DeclarationBlockNoShorthandPropertyOverrides;
    let node = CssNode::Style(StyleRule {
      selector: "h1".to_string(),
      declarations: vec![
        Declaration {
          property: "font-weight".to_string(),
          value: "bold".to_string(),
          span: ParserSpan::new(4, 16),
          important: false,
        },
        Declaration {
          property: "font".to_string(),
          value: "16px/1.5 Arial".to_string(),
          span: ParserSpan::new(22, 22),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 46),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("\"font\""));
    assert!(diags[0].message.contains("\"font-weight\""));
  }
}
