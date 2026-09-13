use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// Specify a pattern for SCSS `@mixin` names.
///
/// Accepts a regex string as the primary option. Defaults to kebab-case.
///
/// Equivalent to `scss/at-mixin-pattern`.
pub struct ScssAtMixinPattern;

impl Rule for ScssAtMixinPattern {
  fn name(&self) -> &'static str {
    "scss/at-mixin-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for SCSS mixin names"
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

    if at.name != "mixin" {
      return vec![];
    }

    let pattern_str = ctx.primary_option_str().unwrap_or(DEFAULT_PATTERN);
    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    // The mixin name is the first word in params (before `(` or whitespace)
    let first_token = at.params.split_whitespace().next().unwrap_or("");
    let mixin_name = first_token.split('(').next().unwrap_or("");

    if mixin_name.is_empty() {
      return vec![];
    }

    if !re.is_match(mixin_name) {
      vec![
        Diagnostic::new(
          self.name(),
          format!(
            "Expected @mixin name \"{}\" to match pattern \"{}\"",
            mixin_name, pattern_str
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

  fn mixin(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn allows_kebab_case() {
    assert!(
      ScssAtMixinPattern
        .check(&mixin("my-mixin($a)"), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_non_matching_pattern() {
    let d = ScssAtMixinPattern.check(&mixin("MyMixin($a)"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MyMixin"));
  }

  #[test]
  fn skips_non_mixin() {
    let node = CssNode::AtRule(AtRule {
      name: "function".to_string(),
      params: "MyFunc".to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    });
    assert!(ScssAtMixinPattern.check(&node, &scss_ctx()).is_empty());
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
      ScssAtMixinPattern
        .check(&mixin("MyMixin"), &css_ctx)
        .is_empty()
    );
  }
}
