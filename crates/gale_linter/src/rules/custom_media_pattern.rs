use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Enforce a naming pattern for custom media queries.
///
/// Equivalent to Stylelint's `custom-media-pattern` rule.
/// Default pattern: kebab-case. Detection-only.
pub struct CustomMediaPattern;

impl Rule for CustomMediaPattern {
  fn name(&self) -> &'static str {
    "custom-media-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for custom media query names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "custom-media" {
      return vec![];
    }

    // @custom-media --name <media-query>
    // The params should start with the custom media name (--name).
    let params = at.params.trim();
    let name = params
      .split_whitespace()
      .next()
      .and_then(|s| s.strip_prefix("--"));

    if let Some(name) = name
      && !is_kebab_case(name)
    {
      return vec![
        Diagnostic::new(
          self.name(),
          format!("Expected custom media query name \"--{name}\" to match kebab-case pattern"),
        )
        .severity(self.default_severity())
        .span(Span::new(at.span.offset, at.span.length)),
      ];
    }

    vec![]
  }
}

/// Matches `^([a-z][a-z0-9]*)(-[a-z0-9]+)*$`
fn is_kebab_case(name: &str) -> bool {
  let bytes = name.as_bytes();
  if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
    return false;
  }

  let mut i = 1;
  while i < bytes.len() {
    if bytes[i] == b'-' {
      i += 1;
      if i >= bytes.len() || !(bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit()) {
        return false;
      }
    } else if bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit() {
      // ok
    } else {
      return false;
    }
    i += 1;
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn custom_media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "custom-media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_non_kebab_case() {
    let d = CustomMediaPattern.check(&custom_media("--myQuery (min-width: 768px)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myQuery"));
  }

  #[test]
  fn allows_kebab_case() {
    let d = CustomMediaPattern.check(&custom_media("--my-query (min-width: 768px)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_custom_media_at_rules() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    let d = CustomMediaPattern.check(&node, &ctx());
    assert!(d.is_empty());
  }
}
