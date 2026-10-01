use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::pattern;
use crate::rule::{Rule, RuleContext};

/// Kebab-case, used when the config gives no pattern.
const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// Enforce a naming pattern for custom media queries.
///
/// Equivalent to Stylelint's `custom-media-pattern` rule.  The primary
/// option is the pattern the name after `--` must match; without one the
/// rule falls back to kebab-case.  Detection-only.
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

  /// Flags an `@custom-media` name that does not match the configured
  /// pattern, at the name itself.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "custom-media" {
      return vec![];
    }

    let pattern_str = ctx.primary_option_str().unwrap_or(DEFAULT_PATTERN);
    let re = match pattern::for_rule(self.name(), pattern_str) {
      Ok(re) => re,
      Err(invalid) => return vec![invalid],
    };

    // @custom-media --name <media-query>
    // The params should start with the custom media name (--name).
    let Some(full_name) = at.params.split_whitespace().next() else {
      return vec![];
    };
    let Some(name) = full_name.strip_prefix("--") else {
      return vec![];
    };
    if pattern::is_match(&re, name) {
      return vec![];
    }

    // Point at the name in the source when it can be found there, as
    // Stylelint does, else at the whole at-rule.
    let span = ctx
      .source_slice(at.span.offset, at.span.end())
      .and_then(|text| text.find(full_name))
      .map(|at_name| Span::new(at.span.offset + at_name, full_name.len()))
      .unwrap_or(Span::new(at.span.offset, at.span.length));

    vec![
      Diagnostic::new(
        self.name(),
        format!("Expected \"{full_name}\" to match pattern \"{pattern_str}\""),
      )
      .severity(self.default_severity())
      .span(span)
      .message_args([full_name, pattern_str]),
    ]
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
      cache: None,
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
    assert_eq!(
      d[0].message,
      "Expected \"--myQuery\" to match pattern \"^[a-z][a-z0-9]*(-[a-z0-9]+)*$\""
    );
  }

  #[test]
  fn uses_the_configured_pattern() {
    let options = serde_json::json!("^(?!bp-)[a-z-]+$");
    let source = "@custom-media --bp-small (max-width: 30em);";
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(&options),
      cache: None,
    };
    let mut node = custom_media("--bp-small (max-width: 30em)");
    if let CssNode::AtRule(at) = &mut node {
      at.span = ParserSpan::new(0, source.len());
    }
    let d = CustomMediaPattern.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected \"--bp-small\" to match pattern \"^(?!bp-)[a-z-]+$\""
    );
    // Reported at the name, not the whole at-rule.
    assert_eq!(d[0].span, Span::new(14, "--bp-small".len()));

    let ok = custom_media("--small (max-width: 30em)");
    assert!(CustomMediaPattern.check(&ok, &ctx).is_empty());
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
