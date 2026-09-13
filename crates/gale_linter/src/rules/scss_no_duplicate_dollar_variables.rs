use std::collections::HashMap;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate dollar variables within the same scope.
///
/// Equivalent to `scss/no-duplicate-dollar-variables`.
pub struct ScssNoDuplicateDollarVariables;

impl Rule for ScssNoDuplicateDollarVariables {
  fn name(&self) -> &'static str {
    "scss/no-duplicate-dollar-variables"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate dollar variables within a scope"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, Span> = HashMap::new();
    collect_dollar_vars(nodes, &mut seen, &mut diagnostics, self);
    diagnostics
  }
}

fn collect_dollar_vars(
  nodes: &[CssNode],
  seen: &mut HashMap<String, Span>,
  diagnostics: &mut Vec<Diagnostic>,
  rule: &ScssNoDuplicateDollarVariables,
) {
  for node in nodes {
    match node {
      CssNode::Declaration(decl) if decl.property.starts_with('$') => {
        let var_name = decl.property.clone();
        if seen.contains_key(&var_name) {
          diagnostics.push(
            Diagnostic::new(
              rule.name(),
              format!("Unexpected duplicate dollar variable \"{}\"", var_name),
            )
            .severity(rule.default_severity())
            .span(Span::new(decl.span.offset, decl.span.length)),
          );
        } else {
          seen.insert(var_name, Span::new(decl.span.offset, decl.span.length));
        }
      }
      CssNode::AtRule(at) => {
        // Each at-rule block gets its own scope for dollar variables,
        // but we still track the outer scope's variables. For simplicity
        // (matching the default stylelint-scss behaviour), we check at
        // the top-level scope only and recurse for nested at-rules.
        collect_dollar_vars(&at.children, seen, diagnostics, rule);
      }
      _ => {}
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn css_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn dollar_var(name: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: name.to_string(),
      value: "red".to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn skips_non_scss() {
    let nodes = vec![dollar_var("$color"), dollar_var("$color")];
    assert!(
      ScssNoDuplicateDollarVariables
        .check_root(&nodes, &css_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_duplicate_dollar_variable() {
    let nodes = vec![
      dollar_var("$color"),
      dollar_var("$size"),
      dollar_var("$color"),
    ];
    let d = ScssNoDuplicateDollarVariables.check_root(&nodes, &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("$color"));
  }

  #[test]
  fn allows_unique_dollar_variables() {
    let nodes = vec![
      dollar_var("$color"),
      dollar_var("$size"),
      dollar_var("$weight"),
    ];
    let d = ScssNoDuplicateDollarVariables.check_root(&nodes, &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_dollar_declarations() {
    let nodes = vec![
      CssNode::Declaration(Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }),
      CssNode::Declaration(Declaration {
        property: "color".to_string(),
        value: "blue".to_string(),
        span: ParserSpan::new(20, 10),
        important: false,
      }),
    ];
    let d = ScssNoDuplicateDollarVariables.check_root(&nodes, &scss_ctx());
    assert!(d.is_empty());
  }
}
