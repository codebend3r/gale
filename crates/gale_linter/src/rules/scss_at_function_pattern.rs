use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// Specify a pattern for SCSS `@function` names.
///
/// Accepts a regex string as the primary option. Defaults to kebab-case.
///
/// Equivalent to `scss/at-function-pattern`.
pub struct ScssAtFunctionPattern;

impl Rule for ScssAtFunctionPattern {
  fn name(&self) -> &'static str {
    "scss/at-function-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for SCSS function names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if at.name != "function" {
      return vec![];
    }

    let pattern_str = ctx.primary_option_str().unwrap_or(DEFAULT_PATTERN);
    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    // The function name is the first word in params (before `(`)
    let first_token = at.params.split_whitespace().next().unwrap_or("");
    let func_name = first_token.split('(').next().unwrap_or("");

    if func_name.is_empty() {
      return vec![];
    }

    if !re.is_match(func_name) {
      vec![
        Diagnostic::new(
          self.name(),
          format!(
            "Expected @function name \"{}\" to match pattern \"{}\"",
            func_name, pattern_str
          ),
        )
        .severity(self.default_severity())
        .span(Span::new(at.span.offset, at.span.length)),
      ]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn function(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "function".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn allows_kebab_case() {
    assert!(
      ScssAtFunctionPattern
        .check(&function("my-func($a)"), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_non_matching_pattern() {
    let d = ScssAtFunctionPattern.check(&function("MyFunc($a)"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MyFunc"));
  }

  #[test]
  fn skips_non_function_at_rule() {
    let node = CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "MyMixin".to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    });
    assert!(ScssAtFunctionPattern.check(&node, &scss_ctx()).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    let css_ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    };
    assert!(
      ScssAtFunctionPattern
        .check(&function("MyFunc"), &css_ctx)
        .is_empty()
    );
  }
}
