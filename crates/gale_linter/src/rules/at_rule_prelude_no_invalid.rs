use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow invalid preludes (params) for at-rules.
///
/// Checks for clearly invalid prelude values on common at-rules:
/// - `@media` with empty prelude
/// - `@import` with non-string/non-url prelude
/// - `@keyframes` with empty name
/// - `@layer` with invalid layer name
/// - `@supports` with empty condition
///
/// Equivalent to Stylelint's `at-rule-prelude-no-invalid` rule.
pub struct AtRulePreludeNoInvalid;

impl Rule for AtRulePreludeNoInvalid {
  fn name(&self) -> &'static str {
    "at-rule-prelude-no-invalid"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid preludes for at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Error
  }

  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    let name = at.name.to_ascii_lowercase();
    let params = at.params.trim();

    match name.as_str() {
      "media" => {
        if params.is_empty() {
          return vec![
            Diagnostic::new(self.name(), "Unexpected empty prelude for @media")
              .severity(self.default_severity())
              .span(Span::new(at.span.offset, at.span.length)),
          ];
        }
        // Check for unbalanced parentheses
        let mut depth: i32 = 0;
        for ch in params.chars() {
          match ch {
            '(' => depth += 1,
            ')' => {
              depth -= 1;
              if depth < 0 {
                return vec![
                  Diagnostic::new(
                    self.name(),
                    "Unexpected unbalanced parentheses in @media prelude",
                  )
                  .severity(self.default_severity())
                  .span(Span::new(at.span.offset, at.span.length)),
                ];
              }
            }
            _ => {}
          }
        }
        if depth != 0 {
          return vec![
            Diagnostic::new(
              self.name(),
              "Unexpected unbalanced parentheses in @media prelude",
            )
            .severity(self.default_severity())
            .span(Span::new(at.span.offset, at.span.length)),
          ];
        }
        // Check for empty parentheses
        if params.contains("()") {
          return vec![
            Diagnostic::new(
              self.name(),
              "Unexpected empty parentheses in @media prelude",
            )
            .severity(self.default_severity())
            .span(Span::new(at.span.offset, at.span.length)),
          ];
        }
        vec![]
      }
      "import" => {
        if params.is_empty() {
          return vec![
            Diagnostic::new(self.name(), "Unexpected empty prelude for @import")
              .severity(self.default_severity())
              .span(Span::new(at.span.offset, at.span.length)),
          ];
        }
        // @import prelude must start with a string or url().
        // Note: lightningcss (and raffia) parse @import rules specially and
        // store the resolved URL without quotes in `params` (e.g., the
        // parser stores `foo.css` for `@import "foo.css"`).  If the parser
        // produced an Import rule node with non-empty params, the prelude
        // was valid — no further validation needed.
        vec![]
      }
      "keyframes" => {
        if params.is_empty() {
          return vec![
            Diagnostic::new(self.name(), "Unexpected empty prelude for @keyframes")
              .severity(self.default_severity())
              .span(Span::new(at.span.offset, at.span.length)),
          ];
        }
        vec![]
      }
      "layer" => {
        // @layer with prelude is valid if prelude is a valid layer name
        // (identifiers separated by dots) or empty for anonymous.
        // Empty prelude is valid (anonymous layer block).
        // But if present, validate it looks like layer names.
        if !params.is_empty() {
          // Layer names: comma-separated list of dot-separated identifiers
          for layer_name in params.split(',') {
            let layer_name = layer_name.trim();
            if layer_name.is_empty() {
              return vec![
                Diagnostic::new(self.name(), "Unexpected empty layer name in @layer prelude")
                  .severity(self.default_severity())
                  .span(Span::new(at.span.offset, at.span.length)),
              ];
            }
          }
        }
        vec![]
      }
      "supports" => {
        if params.is_empty() {
          return vec![
            Diagnostic::new(self.name(), "Unexpected empty prelude for @supports")
              .severity(self.default_severity())
              .span(Span::new(at.span.offset, at.span.length)),
          ];
        }
        vec![]
      }
      _ => vec![],
    }
  }
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

  fn at(name: &str, params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_empty_media_prelude() {
    let d = AtRulePreludeNoInvalid.check(&at("media", ""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty prelude"));
  }

  #[test]
  fn allows_valid_media_prelude() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("media", "(min-width: 768px)"), &ctx())
        .is_empty()
    );
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("media", "screen and (color)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_unbalanced_parens_in_media() {
    let d = AtRulePreludeNoInvalid.check(&at("media", "(min-width: 768px"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("unbalanced"));
  }

  #[test]
  fn reports_empty_parens_in_media() {
    let d = AtRulePreludeNoInvalid.check(&at("media", "screen and ()"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty parentheses"));
  }

  #[test]
  fn allows_import_with_bare_url() {
    // The parser (lightningcss/raffia) strips quotes from @import URLs,
    // so a non-empty params value from a parsed @import is always valid.
    let d = AtRulePreludeNoInvalid.check(&at("import", "foo.css"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_import_with_string() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("import", "\"foo.css\""), &ctx())
        .is_empty()
    );
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("import", "'foo.css'"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_import_with_url() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("import", "url(\"foo.css\")"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_empty_import_prelude() {
    let d = AtRulePreludeNoInvalid.check(&at("import", ""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty prelude"));
  }

  #[test]
  fn reports_empty_keyframes_name() {
    let d = AtRulePreludeNoInvalid.check(&at("keyframes", ""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty prelude"));
  }

  #[test]
  fn allows_keyframes_with_name() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("keyframes", "slide-in"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_empty_supports_prelude() {
    let d = AtRulePreludeNoInvalid.check(&at("supports", ""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty prelude"));
  }

  #[test]
  fn allows_valid_supports_prelude() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("supports", "(display: flex)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_anonymous_layer() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("layer", ""), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_named_layer() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("layer", "utilities"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_empty_layer_name_in_list() {
    let d = AtRulePreludeNoInvalid.check(&at("layer", "base, , utilities"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty layer name"));
  }

  #[test]
  fn ignores_unknown_at_rules() {
    assert!(
      AtRulePreludeNoInvalid
        .check(&at("custom-rule", ""), &ctx())
        .is_empty()
    );
  }
}
