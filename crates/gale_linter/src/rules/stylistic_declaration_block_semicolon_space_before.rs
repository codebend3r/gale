use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before the semicolons of declaration blocks.
///
/// Equivalent to `@stylistic/declaration-block-semicolon-space-before`.
pub struct StylisticDeclarationBlockSemicolonSpaceBefore;

impl Rule for StylisticDeclarationBlockSemicolonSpaceBefore {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-block-semicolon-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before the semicolons of declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let option = ctx.primary_option_str().unwrap_or("never");

    let mut diags = Vec::new();
    let source = ctx.source;

    for decl in &rule.declarations {
      let decl_end = decl.span.offset + decl.span.length;
      if decl_end >= source.len() {
        continue;
      }

      // Find the semicolon following this declaration
      let remaining = &source[decl_end..];
      let semi_rel = match remaining.find(';') {
        Some(pos) => pos,
        None => continue,
      };
      let semi_offset = decl_end + semi_rel;

      if semi_offset == 0 {
        continue;
      }

      let char_before = source.as_bytes()[semi_offset - 1];
      let has_space = char_before == b' ' || char_before == b'\t';

      let violation = match option {
        "always" => !has_space,
        "never" => has_space,
        _ => false,
      };

      if violation {
        let msg = match option {
          "always" => "Expected single space before \";\"",
          "never" => "Unexpected whitespace before \";\"",
          _ => continue,
        };
        diags.push(
          Diagnostic::new(self.name(), msg)
            .severity(self.default_severity())
            .span(Span::new(semi_offset, 1)),
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

  fn ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn allows_no_space_before_semicolon() {
    let rule = StylisticDeclarationBlockSemicolonSpaceBefore;
    let source = "a { color: red; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(4, 10),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_space_before_semicolon_when_never() {
    let rule = StylisticDeclarationBlockSemicolonSpaceBefore;
    let source = "a { color: red ; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(4, 10),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }
}
