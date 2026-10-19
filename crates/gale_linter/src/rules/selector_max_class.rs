use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-max-class";

declare_rule! {
  /// Limit the number of class selectors in a selector.
  ///
  /// Equivalent to Stylelint's `selector-max-class` rule.
  /// Default maximum: 3. Detection-only.
  pub struct SelectorMaxClass => SelectorMaxClassCheck {
    name: NAME,
    description: "Limit the number of class selectors in a selector",
    default_severity: Warning,
  }
}

/// `selector-max-class` as configured.
pub struct SelectorMaxClassCheck {
  /// The most class selectors allowed.
  max: usize,
}

impl SelectorMaxClassCheck {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`).
  /// Gale reads no number as 3.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(|value| Accept::NonNegativeInteger.accepts(value));
    options.finish()?;
    Ok(Self {
      max: max
        .and_then(serde_json::Value::as_u64)
        .map_or(MAX_CLASS, |max| max as usize),
    })
  }
}

const MAX_CLASS: usize = 3;

impl Check for SelectorMaxClassCheck {
  /// Flags a selector holding more class selectors than allowed.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read configured max from options (primary option is a number).
    let max = self.max;

    // Strip SCSS line comments from the selector text
    let selector = if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    ) {
      strip_comment_lines(&rule.selector)
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
                        NAME,
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} class selector(s), found {count}",
                        ),
                    )
                    .span(rule.span),
                );
      }
    }
    diags
  }
}

/// The selector without the lines that are only a `//` comment (SCSS,
/// Less).  A comment after a selector on the same line is left in, unlike
/// [`crate::selector::strip_line_comments`]: this rule has always counted
/// classes in it, and the message quotes the selector as it is cut here.
fn strip_comment_lines(selector: &str) -> String {
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
  use gale_css_parser::{Declaration, StyleRule};
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, invalid_options};

  fn style_with_selector(sel: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(0, 0),
        important: false,
      }],
      span: Span::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_too_many_classes() {
    let rule = configured(&SelectorMaxClass, None);
    let d = rule.check(&style_with_selector(".a .b .c .d"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 4"));
  }

  #[test]
  fn allows_within_limit() {
    let rule = configured(&SelectorMaxClass, None);
    let d = rule.check(&style_with_selector(".a .b .c"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_classes() {
    let rule = configured(&SelectorMaxClass, None);
    let d = rule.check(&style_with_selector("div span"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function, so Stylelint words its
    // problem without "value"; the rule validates no secondary options.
    assert!(invalid_options(NAME, serde_json::json!([2, { "anything": true }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(1.5)),
      ["Invalid option \"1.5\" for rule \"selector-max-class\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("3")),
      ["Invalid option \"3\" for rule \"selector-max-class\""]
    );
  }
}
