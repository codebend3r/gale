use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of universal selectors (`*`) in a selector.
///
/// Equivalent to Stylelint's `selector-max-universal` rule.
/// Default maximum: 1.
///
/// ## Options
///
/// Primary option: `<number>` — maximum allowed universal selectors.
///
/// Secondary options:
/// - `ignoreAfterCombinators: [">", "+", "~", " "]` — universal selectors after
///   these combinators are ignored.
pub struct SelectorMaxUniversal;

const DEFAULT_MAX: usize = 1;

/// Configuration parsed from rule options.
struct Config {
  max: usize,
  ignore_after_child: bool,        // ">"
  ignore_after_next_sibling: bool, // "+"
  ignore_after_sibling: bool,      // "~"
  ignore_after_descendant: bool,   // " "
}

impl Config {
  fn from_context(ctx: &RuleContext) -> Self {
    let max = ctx
      .primary_option()
      .and_then(|v| v.as_u64())
      .map(|n| n as usize)
      .unwrap_or(DEFAULT_MAX);

    let secondary = ctx.secondary_options();

    let ignore_list: Vec<String> = secondary
      .and_then(|v| v.get("ignoreAfterCombinators"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| v.as_str().map(|s| s.to_string()))
          .collect()
      })
      .unwrap_or_default();

    Config {
      max,
      ignore_after_child: ignore_list.iter().any(|s| s == ">"),
      ignore_after_next_sibling: ignore_list.iter().any(|s| s == "+"),
      ignore_after_sibling: ignore_list.iter().any(|s| s == "~"),
      ignore_after_descendant: ignore_list.iter().any(|s| s == " "),
    }
  }
}

impl Rule for SelectorMaxUniversal {
  fn name(&self) -> &'static str {
    "selector-max-universal"
  }

  fn description(&self) -> &'static str {
    "Limit the number of universal selectors in a selector"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let config = Config::from_context(ctx);

    let mut diags = Vec::new();

    for sel in rule.selector.split(',') {
      let sel = sel.trim();
      if sel.is_empty() {
        continue;
      }
      let count = count_universal_selectors(sel, &config);
      if count > config.max {
        diags.push(
                    Diagnostic::new(
                        self.name(),
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} universal selector(s), found {count}",
                            max = config.max,
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

/// Count universal selectors (`*`) in a selector string, respecting
/// `ignoreAfterCombinators` options.
/// Skips `*` inside quoted strings, attribute selectors `[...]`,
/// and pseudo-class arguments.
fn count_universal_selectors(selector: &str, config: &Config) -> usize {
  let mut count = 0;
  let mut in_single_quote = false;
  let mut in_double_quote = false;
  let mut in_attribute = 0; // depth of [ ]
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut last_combinator: Option<char> = None; // ' ', '>', '+', '~'

  while i < len {
    let ch = chars[i];
    match ch {
      '\\' => {
        i += 2; // skip escaped char
        continue;
      }
      '\'' if !in_double_quote && in_attribute == 0 => {
        in_single_quote = !in_single_quote;
      }
      '"' if !in_single_quote && in_attribute == 0 => {
        in_double_quote = !in_double_quote;
      }
      '[' if !in_single_quote && !in_double_quote => {
        in_attribute += 1;
      }
      ']' if !in_single_quote && !in_double_quote && in_attribute > 0 => {
        in_attribute -= 1;
      }
      ' ' | '\t' | '\n' | '\r' if !in_single_quote && !in_double_quote && in_attribute == 0 => {
        // Whitespace acts as descendant combinator if no other combinator
        // was just seen
        if last_combinator.is_none() {
          last_combinator = Some(' ');
        }
        i += 1;
        continue;
      }
      '>' if !in_single_quote && !in_double_quote && in_attribute == 0 => {
        last_combinator = Some('>');
        i += 1;
        continue;
      }
      '+' if !in_single_quote && !in_double_quote && in_attribute == 0 => {
        last_combinator = Some('+');
        i += 1;
        continue;
      }
      '~' if !in_single_quote && !in_double_quote && in_attribute == 0 => {
        last_combinator = Some('~');
        i += 1;
        continue;
      }
      '*' if !in_single_quote && !in_double_quote && in_attribute == 0 => {
        // Check if this universal selector should be ignored
        let should_ignore = match last_combinator {
          Some('>') => config.ignore_after_child,
          Some('+') => config.ignore_after_next_sibling,
          Some('~') => config.ignore_after_sibling,
          Some(' ') => config.ignore_after_descendant,
          _ => false,
        };
        if !should_ignore {
          count += 1;
        }
        last_combinator = None;
      }
      _ if !in_single_quote && !in_double_quote && in_attribute == 0 => {
        // Any other non-whitespace resets the combinator tracking
        // (it's a simple selector component like .class, #id, type, :pseudo)
        if !ch.is_ascii_whitespace() {
          last_combinator = None;
        }
      }
      _ => {}
    }
    i += 1;
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

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
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
  fn reports_too_many_universal() {
    let opts = serde_json::json!([1]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxUniversal.check(&style_with_selector("* *"), &c);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 2"));
  }

  #[test]
  fn allows_single_universal() {
    let d = SelectorMaxUniversal.check(&style_with_selector("*"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_universal() {
    let d = SelectorMaxUniversal.check(&style_with_selector("div .class"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_star_in_attribute_value() {
    let d = SelectorMaxUniversal.check(&style_with_selector("div[class*='foo']"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn checks_each_selector_separately() {
    let d = SelectorMaxUniversal.check(&style_with_selector("*, *"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn respects_custom_max() {
    let options = serde_json::json!([2]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&options),
    };
    let d = SelectorMaxUniversal.check(&style_with_selector("* *"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_after_child_combinator() {
    let opts = serde_json::json!([1, {"ignoreAfterCombinators": [">"]}]);
    let c = ctx_with_options(&opts);
    // * > * — the second * is after child combinator, should be ignored
    let d = SelectorMaxUniversal.check(&style_with_selector("* > *"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn max_zero_rejects_single_universal() {
    let opts = serde_json::json!([0]);
    let c = ctx_with_options(&opts);
    let d = SelectorMaxUniversal.check(&style_with_selector("*"), &c);
    assert_eq!(d.len(), 1);
  }
}
