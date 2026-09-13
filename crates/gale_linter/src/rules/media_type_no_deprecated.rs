use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Deprecated media types (sorted for binary search).
static DEPRECATED_MEDIA_TYPES: &[&str] = &[
  "aural",
  "braille",
  "embossed",
  "handheld",
  "projection",
  "tty",
  "tv",
];

fn is_deprecated_media_type(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  DEPRECATED_MEDIA_TYPES
    .binary_search(&lower.as_str())
    .is_ok()
}

/// Extract media types from the `params` string of a `@media` at-rule.
///
/// Media types appear as bare identifiers before any `and`/`or`/`not`/`only`
/// keywords and parenthesized feature expressions. For example:
///   - `"screen and (min-width: 768px)"` → `["screen"]`
///   - `"tty, projection"` → `["tty", "projection"]`
///   - `"not handheld"` → `["handheld"]`
fn extract_media_types(params: &str) -> Vec<String> {
  let mut types = Vec::new();
  // Split on commas to handle media query lists, then process each query.
  for query in params.split(',') {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    for token in &tokens {
      let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
      if t.is_empty() || t.starts_with('(') {
        continue;
      }
      let lower = t.to_ascii_lowercase();
      // Skip logical keywords
      if matches!(lower.as_str(), "and" | "or" | "not" | "only") {
        continue;
      }
      // If it looks like a media type (no parens), check it
      if is_deprecated_media_type(&lower) {
        types.push(t.to_string());
      }
    }
  }
  types
}

pub struct MediaTypeNoDeprecated;

impl Rule for MediaTypeNoDeprecated {
  fn name(&self) -> &'static str {
    "media-type-no-deprecated"
  }

  fn description(&self) -> &'static str {
    "Disallow deprecated media types"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if !at.name.eq_ignore_ascii_case("media") {
      return vec![];
    }

    let deprecated_types = extract_media_types(&at.params);
    deprecated_types
      .into_iter()
      .map(|media_type| {
        Diagnostic::new(
          self.name(),
          format!("Unexpected deprecated media type \"{}\"", media_type),
        )
        .severity(self.default_severity())
        .span(Span::new(at.span.offset, at.span.length))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Span, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_deprecated_tty() {
    let d = MediaTypeNoDeprecated.check(&media("tty"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("tty"));
  }

  #[test]
  fn reports_deprecated_handheld() {
    let d = MediaTypeNoDeprecated.check(&media("handheld and (min-width: 768px)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("handheld"));
  }

  #[test]
  fn reports_multiple_deprecated() {
    let d = MediaTypeNoDeprecated.check(&media("tty, projection"), &ctx());
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn allows_screen() {
    assert!(
      MediaTypeNoDeprecated
        .check(&media("screen and (min-width: 768px)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_all() {
    assert!(
      MediaTypeNoDeprecated
        .check(&media("all"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn ignores_non_media_at_rule() {
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "tty".to_string(),
      span: Span::new(0, 0),
      children: vec![],
    });
    assert!(MediaTypeNoDeprecated.check(&node, &ctx()).is_empty());
  }
}
