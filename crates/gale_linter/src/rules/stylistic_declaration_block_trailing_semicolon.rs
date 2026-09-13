use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a trailing semicolon within declaration blocks.
///
/// Equivalent to `@stylistic/declaration-block-trailing-semicolon`.
pub struct StylisticDeclarationBlockTrailingSemicolon;

impl Rule for StylisticDeclarationBlockTrailingSemicolon {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-block-trailing-semicolon"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a trailing semicolon within declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let option = ctx.primary_option_str().unwrap_or("always");

    if rule.declarations.is_empty() {
      return vec![];
    }

    // Skip SCSS nested property blocks (value ends with '}')
    {
      let last = &rule.declarations[rule.declarations.len() - 1];
      if last.value.trim_end().ends_with('}') {
        return vec![];
      }
    }

    // Skip SCSS nested property blocks (selector ends with ':')
    if rule.selector.trim_end().ends_with(':') {
      return vec![];
    }

    let source = ctx.source;
    let rule_end = rule.span.offset + rule.span.length;

    // Find the closing brace
    if rule_end == 0 || rule_end > source.len() {
      return vec![];
    }

    // Look for the closing brace of this rule
    let block_source = &source[rule.span.offset..rule_end.min(source.len())];
    let closing_brace = match block_source.rfind('}') {
      Some(pos) => rule.span.offset + pos,
      None => return vec![],
    };

    // Find the last declaration's end
    let last_decl = &rule.declarations[rule.declarations.len() - 1];
    let last_decl_end = last_decl.span.offset + last_decl.span.length;

    // Check text between last declaration end and closing brace
    if last_decl_end > closing_brace || last_decl_end > source.len() {
      return vec![];
    }
    let between = &source[last_decl_end..closing_brace];

    // The parser's declaration span may or may not include the trailing
    // semicolon (lightningcss includes it, hand-crafted test spans may
    // not).  We must check both inside the span AND in the gap after it.
    let span_includes_semi = last_decl.span.length > 0
      && last_decl_end > 0
      && last_decl_end <= source.len()
      && source.as_bytes()[last_decl_end - 1] == b';';
    let has_semicolon = span_includes_semi || between.contains(';');

    match option {
      "always" if !has_semicolon => {
        vec![
          Diagnostic::new(self.name(), "Expected a trailing semicolon")
            .severity(self.default_severity())
            .span(Span::new(last_decl_end, 0))
            .fix(Fix::new(
              "Add trailing semicolon",
              vec![Edit::new(Span::new(last_decl_end, 0), ";")],
            )),
        ]
      }
      "never" if has_semicolon => {
        // Find the semicolon position — it may be the last byte of the
        // declaration span or somewhere in the gap before the closing
        // brace.
        let abs_semi = if span_includes_semi {
          last_decl_end - 1
        } else if let Some(pos) = between.find(';') {
          last_decl_end + pos
        } else {
          return vec![];
        };
        vec![
          Diagnostic::new(self.name(), "Unexpected trailing semicolon")
            .severity(self.default_severity())
            .span(Span::new(abs_semi, 1))
            .fix(Fix::new(
              "Remove trailing semicolon",
              vec![Edit::new(Span::new(abs_semi, 1), "")],
            )),
        ]
      }
      _ => vec![],
    }
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
  fn allows_trailing_semicolon() {
    let rule = StylisticDeclarationBlockTrailingSemicolon;
    // Span does NOT include the semicolon (semi is in the gap)
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
  fn allows_trailing_semicolon_span_includes_semi() {
    let rule = StylisticDeclarationBlockTrailingSemicolon;
    // Span includes the semicolon (what lightningcss actually produces)
    let source = "a { color: red; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(4, 11), // includes the ';'
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_multiline_trailing_semicolon() {
    let rule = StylisticDeclarationBlockTrailingSemicolon;
    // Mimics the Grafana false-positive case: multi-declaration block
    // where the parser span includes the ';' on the last declaration.
    let source = ".foo {\n  position: absolute;\n  top: 0;\n}";
    let ctx = ctx_with_source(source);
    // "top: 0;" starts at byte 29, length 7 (includes ';')
    let node = CssNode::Style(StyleRule {
      selector: ".foo".to_string(),
      declarations: vec![
        Declaration {
          property: "position".to_string(),
          value: "absolute".to_string(),
          span: ParserSpan::new(9, 19), // "position: absolute;"
          important: false,
        },
        Declaration {
          property: "top".to_string(),
          value: "0".to_string(),
          span: ParserSpan::new(31, 7), // "top: 0;"
          important: false,
        },
      ],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(
      d.is_empty(),
      "should not report when trailing ; exists, got: {:?}",
      d
    );
  }

  #[test]
  fn reports_missing_trailing_semicolon() {
    let rule = StylisticDeclarationBlockTrailingSemicolon;
    let source = "a { color: red }";
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
    assert!(d[0].message.contains("trailing semicolon"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn never_reports_with_span_including_semi() {
    let rule = StylisticDeclarationBlockTrailingSemicolon;
    // "never" mode: span includes the ';'
    let source = "a { color: red; }";
    let opts = serde_json::json!(["never"]);
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(4, 11), // includes ';'
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
    // The fix should point at byte 14 (the ';')
    assert_eq!(d[0].span.offset, 14);
    assert_eq!(d[0].span.length, 1);
  }
}
