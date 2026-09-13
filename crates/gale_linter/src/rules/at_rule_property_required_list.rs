use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require specific properties for at-rules.
///
/// Options: an object mapping at-rule names (lowercase, without `@`) to arrays
/// of required property names.
///
/// Example config:
/// ```json
/// { "font-face": ["font-family", "font-style"] }
/// ```
///
/// Equivalent to Stylelint's `at-rule-property-required-list` rule.
pub struct AtRulePropertyRequiredList;

impl Rule for AtRulePropertyRequiredList {
  fn name(&self) -> &'static str {
    "at-rule-property-required-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of required properties for at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };

    let required_map = match parse_options(ctx.options) {
      Some(m) => m,
      None => return vec![],
    };

    let name_lower = at_rule.name.to_ascii_lowercase();
    let required_props = match required_map.get(name_lower.as_str()) {
      Some(props) => props,
      None => return vec![],
    };

    // Collect declared properties from the at-rule's children.
    let declared: Vec<String> = at_rule
      .children
      .iter()
      .filter_map(|child| {
        if let CssNode::Declaration(decl) = child {
          Some(decl.property.to_ascii_lowercase())
        } else {
          None
        }
      })
      .collect();

    let mut diags = Vec::new();
    for req in required_props {
      if !declared.iter().any(|d| d == req) {
        diags.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Expected property \"{req}\" in at-rule \"@{name}\"",
              name = at_rule.name,
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(at_rule.span.offset, at_rule.span.length)),
        );
      }
    }
    diags
  }
}

/// Parse the rule options as `{ "at-rule-name": ["prop1", "prop2"] }`.
fn parse_options(options: Option<&serde_json::Value>) -> Option<HashMap<String, Vec<String>>> {
  let obj = options?.as_object()?;
  let mut map = HashMap::new();
  for (key, val) in obj {
    if let Some(arr) = val.as_array() {
      let props: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
        .collect();
      map.insert(key.to_ascii_lowercase(), props);
    }
  }
  Some(map)
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, Syntax};

  fn ctx_with_options(options: serde_json::Value) -> (serde_json::Value, RuleContext<'static>) {
    // We need to return owned value so the reference lives long enough.
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    (options, ctx)
  }

  fn at_rule_with_decls(name: &str, decls: Vec<(&str, &str)>) -> CssNode {
    let children = decls
      .into_iter()
      .map(|(prop, val)| {
        CssNode::Declaration(Declaration {
          property: prop.to_string(),
          value: val.to_string(),
          span: ParserSpan::new(0, 10),
          important: false,
        })
      })
      .collect();
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: String::new(),
      span: ParserSpan::new(0, 20),
      children,
    })
  }

  #[test]
  fn reports_missing_required_property() {
    let opts = serde_json::json!({ "font-face": ["font-family", "font-style"] });
    let (val, mut c) = ctx_with_options(opts);
    c.options = Some(&val);

    let node = at_rule_with_decls("font-face", vec![("font-family", "Arial")]);
    let d = AtRulePropertyRequiredList.check(&node, &c);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("font-style"));
  }

  #[test]
  fn allows_when_all_properties_present() {
    let opts = serde_json::json!({ "font-face": ["font-family", "font-style"] });
    let (val, mut c) = ctx_with_options(opts);
    c.options = Some(&val);

    let node = at_rule_with_decls(
      "font-face",
      vec![("font-family", "Arial"), ("font-style", "normal")],
    );
    let d = AtRulePropertyRequiredList.check(&node, &c);
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_unmatched_at_rules() {
    let opts = serde_json::json!({ "font-face": ["font-family"] });
    let (val, mut c) = ctx_with_options(opts);
    c.options = Some(&val);

    let node = at_rule_with_decls("media", vec![]);
    let d = AtRulePropertyRequiredList.check(&node, &c);
    assert!(d.is_empty());
  }

  #[test]
  fn returns_empty_when_no_options() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let node = at_rule_with_decls("font-face", vec![]);
    let d = AtRulePropertyRequiredList.check(&node, &ctx);
    assert!(d.is_empty());
  }
}
