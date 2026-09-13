use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow specific properties within rules matching certain selectors.
///
/// Options: an object mapping selector patterns to arrays of disallowed
/// property names. Selector patterns can be plain strings (exact match) or
/// regex patterns (strings starting and ending with `/`).
///
/// Example config:
/// ```json
/// { "/^my-/": ["color", "background"], ".foo": ["margin"] }
/// ```
///
/// Equivalent to Stylelint's `rule-selector-property-disallowed-list` rule.
pub struct RuleSelectorPropertyDisallowedList;

impl Rule for RuleSelectorPropertyDisallowedList {
  fn name(&self) -> &'static str {
    "rule-selector-property-disallowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of disallowed properties within rules matching certain selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(style_rule) = node else {
      return vec![];
    };

    let entries = match parse_options(ctx.options) {
      Some(e) => e,
      None => return vec![],
    };

    let selector = style_rule.selector.trim();
    let mut diags = Vec::new();

    for (pattern, disallowed_props) in &entries {
      let matches =
        if let Some(regex_body) = pattern.strip_prefix('/').and_then(|s| s.strip_suffix('/')) {
          regex::Regex::new(regex_body)
            .map(|re| re.is_match(selector))
            .unwrap_or(false)
        } else {
          selector == pattern.as_str()
        };

      if matches {
        for decl in &style_rule.declarations {
          let prop_lower = decl.property.to_ascii_lowercase();
          if disallowed_props.contains(&prop_lower) {
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Unexpected property \"{prop}\" for selector \"{selector}\"",
                  prop = decl.property,
                ),
              )
              .severity(self.default_severity())
              .span(Span::new(decl.span.offset, decl.span.length)),
            );
          }
        }
      }
    }

    diags
  }
}

/// A selector pattern paired with its disallowed properties.
type PatternEntry = (String, Vec<String>);

/// Parse the rule options as `{ "selector-pattern": ["prop1", "prop2"] }`.
fn parse_options(options: Option<&serde_json::Value>) -> Option<Vec<PatternEntry>> {
  let obj = options?.as_object()?;
  let mut entries = Vec::new();
  for (key, val) in obj {
    if let Some(arr) = val.as_array() {
      let props: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
        .collect();
      entries.push((key.clone(), props));
    }
  }
  Some(entries)
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn style_with_selector_and_decl(sel: &str, prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    })
  }

  #[test]
  fn reports_disallowed_property_with_regex_selector() {
    let opts = serde_json::json!({ "/^my-/": ["color", "background"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = style_with_selector_and_decl("my-component", "color", "red");
    let d = RuleSelectorPropertyDisallowedList.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("color"));
  }

  #[test]
  fn allows_non_matching_selector() {
    let opts = serde_json::json!({ "/^my-/": ["color"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = style_with_selector_and_decl(".other", "color", "red");
    let d = RuleSelectorPropertyDisallowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_disallowed_property() {
    let opts = serde_json::json!({ "/^my-/": ["color"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = style_with_selector_and_decl("my-component", "display", "block");
    let d = RuleSelectorPropertyDisallowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn supports_exact_selector_match() {
    let opts = serde_json::json!({ ".foo": ["margin"] });
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = style_with_selector_and_decl(".foo", "margin", "10px");
    let d = RuleSelectorPropertyDisallowedList.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("margin"));
  }

  #[test]
  fn returns_empty_when_no_options() {
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    let node = style_with_selector_and_decl(".foo", "color", "red");
    let d = RuleSelectorPropertyDisallowedList.check(&node, &ctx);
    assert!(d.is_empty());
  }
}
