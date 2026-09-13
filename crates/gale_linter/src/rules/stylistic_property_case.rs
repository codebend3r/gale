use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify lowercase or uppercase for properties.
///
/// Equivalent to `@stylistic/property-case`.
pub struct StylisticPropertyCase;

impl Rule for StylisticPropertyCase {
  fn name(&self) -> &'static str {
    "@stylistic/property-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let expected_case = ctx.primary_option_str().unwrap_or("lower");

    let decls: Vec<_> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();
    for decl in decls {
      let prop = &decl.property;
      // Skip custom properties (--*) and SCSS variables ($*)
      if prop.starts_with("--") || prop.starts_with('$') {
        continue;
      }
      // Skip vendor-prefixed properties for the case check
      let check_part = if prop.starts_with('-') {
        // e.g. -webkit-transform -> check "transform" part
        if let Some(idx) = prop[1..].find('-') {
          &prop[idx + 2..]
        } else {
          prop.as_str()
        }
      } else {
        prop.as_str()
      };

      let is_wrong = match expected_case {
        "lower" => check_part.chars().any(|c| c.is_ascii_uppercase()),
        "upper" => check_part.chars().any(|c| c.is_ascii_lowercase()),
        _ => false,
      };

      if is_wrong {
        let fixed = match expected_case {
          "lower" => prop.to_ascii_lowercase(),
          "upper" => prop.to_ascii_uppercase(),
          _ => continue,
        };
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Expected \"{prop}\" to be \"{fixed}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(decl.span.offset, prop.len()))
          .fix(Fix::new(
            format!("Convert to {expected_case}case"),
            vec![Edit::new(Span::new(decl.span.offset, prop.len()), &fixed)],
          )),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_option(opt: &str) -> RuleContext<'_> {
    // We'll leak a small string for test convenience
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_prop(prop: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, prop.len() + 6),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_uppercase_property() {
    let rule = StylisticPropertyCase;
    let ctx = ctx_with_option("lower");
    let d = rule.check(&style_with_prop("Color"), &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color"));
  }

  #[test]
  fn allows_lowercase_property() {
    let rule = StylisticPropertyCase;
    let ctx = ctx_with_option("lower");
    let d = rule.check(&style_with_prop("color"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_custom_properties() {
    let rule = StylisticPropertyCase;
    let ctx = ctx_with_option("lower");
    let d = rule.check(&style_with_prop("--myColor"), &ctx);
    assert!(d.is_empty());
  }
}
