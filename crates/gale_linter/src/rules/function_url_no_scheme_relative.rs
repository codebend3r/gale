use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow scheme-relative URLs (e.g. `//example.com/path`).
///
/// Scheme-relative URLs (starting with `//`) are deprecated and should be
/// replaced with explicit `https://` or `http://` URLs.
///
/// Equivalent to Stylelint's `function-url-no-scheme-relative` rule.
pub struct FunctionUrlNoSchemeRelative;

impl Rule for FunctionUrlNoSchemeRelative {
  fn name(&self) -> &'static str {
    "function-url-no-scheme-relative"
  }

  fn description(&self) -> &'static str {
    "Disallow scheme-relative urls"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };
    for decl in declarations {
      check_value(&decl.value, decl.span.offset, self, &mut diags);
    }
    diags
  }
}

fn check_value(
  value: &str,
  base_offset: usize,
  rule: &FunctionUrlNoSchemeRelative,
  diags: &mut Vec<Diagnostic>,
) {
  let lower = value.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    if bytes[i] == b'(' && i >= 3 {
      // Check if this is a url( function.
      let paren_pos = i;
      let mut start = i;
      while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
          || bytes[start - 1] == b'-'
          || bytes[start - 1] == b'_')
      {
        start -= 1;
      }
      let fname = &lower[start..paren_pos];
      if fname == "url" {
        // Find matching close paren.
        let after_paren = paren_pos + 1;
        let mut depth = 1i32;
        let mut close = after_paren;
        for (j, &byte) in bytes.iter().enumerate().take(len).skip(after_paren) {
          if byte == b'(' {
            depth += 1;
          } else if byte == b')' {
            depth -= 1;
            if depth == 0 {
              close = j;
              break;
            }
          }
        }
        // Extract url content, stripping quotes.
        let content = lower[after_paren..close].trim();
        let unquoted = if (content.starts_with('"') && content.ends_with('"'))
          || (content.starts_with('\'') && content.ends_with('\''))
        {
          &content[1..content.len() - 1]
        } else {
          content
        };

        if unquoted.starts_with("//") {
          diags.push(
            Diagnostic::new(
              rule.name(),
              format!("Unexpected scheme-relative url \"{}\"", unquoted),
            )
            .severity(rule.default_severity())
            .span(Span::new(base_offset + start, close + 1 - start)),
          );
        }

        i = close + 1;
        continue;
      }
    }
    i += 1;
  }
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

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "background".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, val.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  fn decl_with_value(val: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "background".to_string(),
      value: val.to_string(),
      span: ParserSpan::new(0, val.len()),
      important: false,
    })
  }

  #[test]
  fn reports_scheme_relative_url() {
    let d =
      FunctionUrlNoSchemeRelative.check(&style_with_value("url(//example.com/image.png)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("//example.com"));
  }

  #[test]
  fn reports_quoted_scheme_relative_url() {
    let d = FunctionUrlNoSchemeRelative
      .check(&style_with_value("url('//example.com/image.png')"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn accepts_absolute_url() {
    let d = FunctionUrlNoSchemeRelative.check(
      &style_with_value("url(https://example.com/image.png)"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn accepts_relative_url() {
    let d = FunctionUrlNoSchemeRelative.check(&style_with_value("url(images/bg.png)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_scheme_relative_in_declaration_node() {
    let d = FunctionUrlNoSchemeRelative
      .check(&decl_with_value("url(//cdn.example.com/font.woff)"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_non_url_function() {
    let d = FunctionUrlNoSchemeRelative.check(&style_with_value("rgb(0, 0, 0)"), &ctx());
    assert!(d.is_empty());
  }
}
