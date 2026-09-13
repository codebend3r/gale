use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*-?$";

/// Specify a pattern for `%placeholder` selectors.
///
/// Accepts a regex string as the primary option. Defaults to kebab-case.
/// The `%` prefix is stripped before matching.
///
/// Equivalent to `scss/percent-placeholder-pattern`.
pub struct ScssPercentPlaceholderPattern;

impl Rule for ScssPercentPlaceholderPattern {
  fn name(&self) -> &'static str {
    "scss/percent-placeholder-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for %placeholder selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let pattern_str = ctx.primary_option_str().unwrap_or(DEFAULT_PATTERN);
    let re = match Regex::new(pattern_str) {
      Ok(r) => r,
      Err(_) => return vec![],
    };

    let mut diags = Vec::new();

    // Check each selector in the comma-separated list
    for part in rule.selector.split(',') {
      let sel = part.trim();
      // Look for %placeholder within the selector
      for segment in sel.split_whitespace() {
        let segment = segment.trim_start_matches('&');
        if let Some(rest) = segment.strip_prefix('%') {
          // If the placeholder contains SCSS interpolation #{}, skip validation
          // since the final name is dynamic
          if rest.contains("#{") {
            continue;
          }

          // Strip any trailing pseudo-class or combinator chars
          let placeholder_name = rest
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .next()
            .unwrap_or("");

          if placeholder_name.is_empty() {
            continue;
          }

          if !re.is_match(placeholder_name) {
            diags.push(
              Diagnostic::new(
                self.name(),
                format!(
                  "Expected %{} to match pattern \"{}\"",
                  placeholder_name, pattern_str
                ),
              )
              .severity(self.default_severity())
              .span(Span::new(rule.span.offset, rule.span.length)),
            );
          }
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Span as ParserSpan, StyleRule, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn style(selector: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: selector.to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, 20),
      ..Default::default()
    })
  }

  #[test]
  fn allows_kebab_case_placeholder() {
    assert!(
      ScssPercentPlaceholderPattern
        .check(&style("%my-placeholder"), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_trailing_hyphen() {
    assert!(
      ScssPercentPlaceholderPattern
        .check(&style("%responsive-container-"), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn skips_interpolated_placeholder() {
    assert!(
      ScssPercentPlaceholderPattern
        .check(&style("%responsive-container-#{$breakpoint}"), &scss_ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_non_matching_placeholder() {
    let d = ScssPercentPlaceholderPattern.check(&style("%MyPlaceholder"), &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MyPlaceholder"));
  }

  #[test]
  fn skips_non_placeholder_selectors() {
    assert!(
      ScssPercentPlaceholderPattern
        .check(&style(".my-class"), &scss_ctx())
        .is_empty()
    );
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
      ScssPercentPlaceholderPattern
        .check(&style("%MyPlaceholder"), &css_ctx)
        .is_empty()
    );
  }
}
