use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow parentheses in argumentless `@include` mixin calls.
///
/// By default expects `"always"` (parentheses are required):
///
/// ```scss
/// // Good (always)
/// @include mixin-name();
///
/// // Bad (always)
/// @include mixin-name;
/// ```
///
/// Equivalent to `scss/at-mixin-argumentless-call-parentheses`.
pub struct ScssAtMixinArgumentlessCallParentheses;

impl Rule for ScssAtMixinArgumentlessCallParentheses {
  fn name(&self) -> &'static str {
    "scss/at-mixin-argumentless-call-parentheses"
  }

  fn description(&self) -> &'static str {
    "Require or disallow parentheses in argumentless @include calls"
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

    if at.name != "include" {
      return vec![];
    }

    let option = ctx.primary_option_str().unwrap_or("always");

    let params = at.params.trim();
    // An include with arguments (contains `(` with non-empty content before `)`) is not
    // "argumentless" — skip it.
    let has_args = if let Some(paren_pos) = params.find('(') {
      let after_paren = &params[paren_pos + 1..];
      let inner = after_paren.trim_end_matches(')').trim();
      !inner.is_empty()
    } else {
      false
    };

    if has_args {
      return vec![];
    }

    let has_parens = params.contains('(');

    match option {
      "always" => {
        if !has_parens {
          vec![
            Diagnostic::new(
              self.name(),
              format!(
                "Expected parentheses in argumentless @include \"{}\"",
                params
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(at.span.offset, at.span.length)),
          ]
        } else {
          vec![]
        }
      }
      "never" => {
        if has_parens {
          let mixin_name = params.trim_end_matches("()");
          // Point to the mixin name, not the `@include` keyword.
          let at_src_end = (at.span.offset + at.span.length).min(ctx.source.len());
          let at_src = &ctx.source[at.span.offset..at_src_end];
          let name_off = at_src.find(mixin_name).unwrap_or(0);
          vec![
            Diagnostic::new(
              self.name(),
              format!(
                "Unexpected parentheses in argumentless mixin \"{}\" call",
                mixin_name
              ),
            )
            .severity(self.default_severity())
            .span(Span::new(at.span.offset + name_off, mixin_name.len())),
          ]
        } else {
          vec![]
        }
      }
      _ => vec![],
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

  fn include(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "include".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    })
  }

  #[test]
  fn reports_missing_parens_always() {
    let d = ScssAtMixinArgumentlessCallParentheses.check(&include("mixin-name"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("parentheses"));
  }

  #[test]
  fn allows_parens_always() {
    let d = ScssAtMixinArgumentlessCallParentheses.check(&include("mixin-name()"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_include_with_args() {
    let d = ScssAtMixinArgumentlessCallParentheses.check(&include("mixin-name($arg)"), &scss_ctx());
    assert!(d.is_empty());
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
      ScssAtMixinArgumentlessCallParentheses
        .check(&include("mixin-name"), &css_ctx)
        .is_empty()
    );
  }

  #[test]
  fn skips_non_include() {
    let node = CssNode::AtRule(AtRule {
      name: "mixin".to_string(),
      params: "foo".to_string(),
      span: ParserSpan::new(0, 10),
      children: vec![],
    });
    assert!(
      ScssAtMixinArgumentlessCallParentheses
        .check(&node, &scss_ctx())
        .is_empty()
    );
  }
}
