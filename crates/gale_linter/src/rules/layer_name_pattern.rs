use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

/// Enforces a naming pattern for CSS layer names.
///
/// Checks `@layer` at-rules. Layer names can be dotted (`base.utilities`)
/// and comma-separated (`@layer reset, base`). Each segment of a dotted
/// name is validated individually.
///
/// Accepts a regex string as the primary option.
/// Defaults to kebab-case pattern if no option is provided.
///
/// Equivalent to Stylelint's `layer-name-pattern` rule.
pub struct LayerNamePattern;

impl Rule for LayerNamePattern {
  fn name(&self) -> &'static str {
    "layer-name-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for layer names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if !at.name.eq_ignore_ascii_case("layer") {
      return vec![];
    }

    let params = at.params.trim();
    if params.is_empty() {
      return vec![];
    }

    let pattern_str = ctx
      .options
      .and_then(|v| v.as_str())
      .unwrap_or(DEFAULT_PATTERN);

    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    let mut diags = Vec::new();

    // Layer names can be comma-separated: `@layer reset, base, utilities`
    for layer_name in params.split(',') {
      let layer_name = layer_name.trim();
      if layer_name.is_empty() {
        continue;
      }

      // Layer names can be dotted: `base.utilities` — validate each segment
      for segment in layer_name.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
          continue;
        }
        if !re.is_match(segment) {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!(
                "Expected layer name segment \"{segment}\" to match pattern \"{pattern_str}\""
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(at.span.offset, at.span.length)),
          );
        }
      }
    }

    diags
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

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn layer(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "layer".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_camel_case_layer_name() {
    let d = LayerNamePattern.check(&layer("myLayer"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myLayer"));
  }

  #[test]
  fn allows_kebab_case_layer_name() {
    let d = LayerNamePattern.check(&layer("my-layer"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_bad_segment_in_dotted_name() {
    let d = LayerNamePattern.check(&layer("base.myUtils"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myUtils"));
  }

  #[test]
  fn allows_kebab_case_dotted_name() {
    let d = LayerNamePattern.check(&layer("base.utilities"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_bad_names_in_comma_list() {
    let d = LayerNamePattern.check(&layer("reset, myBase, utilities"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myBase"));
  }

  #[test]
  fn allows_all_kebab_case_comma_list() {
    let d = LayerNamePattern.check(&layer("reset, base, utilities"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_layer_at_rule() {
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    let d = LayerNamePattern.check(&node, &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn custom_pattern() {
    let opts = serde_json::json!("^[A-Z][a-zA-Z0-9]+$");
    let c = ctx_with_options(&opts);
    // PascalCase should pass
    assert!(LayerNamePattern.check(&layer("MyLayer"), &c).is_empty());
    // kebab-case should fail
    let d = LayerNamePattern.check(&layer("my-layer"), &c);
    assert_eq!(d.len(), 1);
  }
}
