use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of class selectors in a selector.
///
/// Equivalent to Stylelint's `selector-max-class` rule.
/// Default maximum: 3. Detection-only.
pub struct SelectorMaxClass;

const MAX_CLASS: usize = 3;

impl Rule for SelectorMaxClass {
  fn name(&self) -> &'static str {
    "selector-max-class"
  }

  fn description(&self) -> &'static str {
    "Limit the number of class selectors in a selector"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read configured max from options (primary option is a number).
    let max = ctx
      .options
      .and_then(|v| v.as_u64())
      .map(|n| n as usize)
      .unwrap_or(MAX_CLASS);

    // Strip SCSS line comments from the selector text
    let selector = if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    ) {
      strip_scss_line_comments(&rule.selector)
    } else {
      rule.selector.clone()
    };

    let mut diags = Vec::new();
    // Check each comma-separated selector individually
    for sel in selector.split(',') {
      let sel = sel.trim();
      if sel.is_empty() {
        continue;
      }
      let count = count_class_selectors(sel);
      if count > max {
        diags.push(
                    Diagnostic::new(
                        self.name(),
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} class selector(s), found {count}",
                        ),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(rule.span.offset, rule.span.length)),
                );
      }
    }
    diags
  }
}

/// Strip `//` line comments from a selector string (SCSS/Less).
fn strip_scss_line_comments(selector: &str) -> String {
  selector
    .lines()
    .filter(|line| !line.trim_start().starts_with("//"))
    .collect::<Vec<_>>()
    .join("\n")
}

/// Count `.` characters that are class selectors (followed by a CSS ident start character).
fn count_class_selectors(selector: &str) -> usize {
  let chars: Vec<char> = selector.chars().collect();
  let mut count = 0;
  for (i, &ch) in chars.iter().enumerate() {
    if ch == '.'
      && let Some(&next) = chars.get(i + 1)
      && (next.is_ascii_alphabetic() || next == '_' || next == '-' || !next.is_ascii())
    {
      count += 1;
    }
  }
  count
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_too_many_classes() {
    let d = SelectorMaxClass.check(&style_with_selector(".a .b .c .d"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 4"));
  }

  #[test]
  fn allows_within_limit() {
    let d = SelectorMaxClass.check(&style_with_selector(".a .b .c"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_classes() {
    let d = SelectorMaxClass.check(&style_with_selector("div span"), &ctx());
    assert!(d.is_empty());
  }
}
