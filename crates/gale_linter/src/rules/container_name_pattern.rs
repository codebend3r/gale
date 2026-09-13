use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

/// Enforces a naming pattern for container names.
///
/// Checks both the `container-name` property and `@container` at-rule.
/// Accepts a regex string as the primary option.
/// Defaults to kebab-case pattern if no option is provided.
///
/// Equivalent to Stylelint's `container-name-pattern` rule.
pub struct ContainerNamePattern;

impl Rule for ContainerNamePattern {
  fn name(&self) -> &'static str {
    "container-name-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for container names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let pattern_str = ctx
      .options
      .and_then(|v| v.as_str())
      .unwrap_or(DEFAULT_PATTERN);

    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    match node {
      CssNode::Style(rule) => {
        let mut diags = Vec::new();
        for decl in &rule.declarations {
          let prop = decl.property.to_ascii_lowercase();
          if prop == "container-name" {
            for name in extract_container_names(&decl.value) {
              if !re.is_match(&name) {
                diags.push(
                  Diagnostic::new(
                    self.name(),
                    format!(
                      "Expected container name \"{name}\" to match pattern \"{pattern_str}\""
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
      CssNode::AtRule(at) => {
        if !at.name.eq_ignore_ascii_case("container") {
          return vec![];
        }
        // @container <name> <query>
        // The name is the first token in params (before '(' or whitespace that precedes '(')
        let params = at.params.trim();
        if params.is_empty() || params.starts_with('(') {
          return vec![];
        }
        let name = params
          .split(|c: char| c == '(' || c.is_whitespace())
          .next()
          .unwrap_or("")
          .trim();
        if name.is_empty() {
          return vec![];
        }
        // Skip CSS-wide keywords
        let lower = name.to_ascii_lowercase();
        if lower == "none" || lower == "inherit" || lower == "initial" || lower == "unset" {
          return vec![];
        }
        if !re.is_match(name) {
          return vec![
            Diagnostic::new(
              self.name(),
              format!("Expected container name \"{name}\" to match pattern \"{pattern_str}\""),
            )
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

/// Extract container names from a `container-name` value.
/// The value can be a single name or space-separated list. CSS keywords `none`,
/// `inherit`, `initial`, `unset` are skipped.
fn extract_container_names(value: &str) -> Vec<String> {
  value
    .split_whitespace()
    .filter(|s| {
      let lower = s.to_ascii_lowercase();
      lower != "none" && lower != "inherit" && lower != "initial" && lower != "unset"
    })
    .map(|s| s.to_string())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, StyleRule, Syntax};

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

  fn style_with_container_name(value: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: ".box".to_string(),
      declarations: vec![Declaration {
        property: "container-name".to_string(),
        value: value.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  fn container_at_rule(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "container".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_camel_case_container_name_property() {
    let d = ContainerNamePattern.check(&style_with_container_name("myContainer"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myContainer"));
  }

  #[test]
  fn allows_kebab_case_container_name_property() {
    let d = ContainerNamePattern.check(&style_with_container_name("my-container"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_camel_case_container_at_rule() {
    let d = ContainerNamePattern.check(&container_at_rule("myCard (min-width: 700px)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myCard"));
  }

  #[test]
  fn allows_kebab_case_container_at_rule() {
    let d = ContainerNamePattern.check(&container_at_rule("my-card (min-width: 700px)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_none_keyword() {
    let d = ContainerNamePattern.check(&style_with_container_name("none"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn custom_pattern() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let c = ctx_with_options(&opts);
    // camelCase should pass
    assert!(
      ContainerNamePattern
        .check(&style_with_container_name("myContainer"), &c)
        .is_empty()
    );
    // kebab-case should fail
    let d = ContainerNamePattern.check(&style_with_container_name("my-container"), &c);
    assert_eq!(d.len(), 1);
  }
}
