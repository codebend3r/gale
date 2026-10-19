use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-max-universal";

declare_rule! {
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
  pub struct SelectorMaxUniversal => SelectorMaxUniversalCheck {
    name: NAME,
    description: "Limit the number of universal selectors in a selector",
    default_severity: Warning,
  }
}

/// `selector-max-universal` as configured.
pub struct SelectorMaxUniversalCheck {
  config: Config,
}

impl SelectorMaxUniversalCheck {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`), then
  /// `ignoreAfterCombinators`.  Gale reads no number as 1.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(|value| Accept::NonNegativeInteger.accepts(value));
    let ignore_list = options.strings("ignoreAfterCombinators");
    options.finish()?;
    Ok(Self {
      config: Config {
        max: max
          .and_then(serde_json::Value::as_u64)
          .map_or(DEFAULT_MAX, |max| max as usize),
        ignore_after_child: ignore_list.iter().any(|s| s == ">"),
        ignore_after_next_sibling: ignore_list.iter().any(|s| s == "+"),
        ignore_after_sibling: ignore_list.iter().any(|s| s == "~"),
        ignore_after_descendant: ignore_list.iter().any(|s| s == " "),
      },
    })
  }
}

const DEFAULT_MAX: usize = 1;

/// Configuration parsed from rule options.
struct Config {
  max: usize,
  ignore_after_child: bool,        // ">"
  ignore_after_next_sibling: bool, // "+"
  ignore_after_sibling: bool,      // "~"
  ignore_after_descendant: bool,   // " "
}

impl Check for SelectorMaxUniversalCheck {
  /// Flags selectors holding more `*` than allowed, checking each comma-separated
  /// selector on its own.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let config = &self.config;

    let mut diags = Vec::new();

    for sel in rule.selector.split(',') {
      let sel = sel.trim();
      if sel.is_empty() {
        continue;
      }
      let count = count_universal_selectors(sel, config);
      if count > config.max {
        diags.push(
                    Diagnostic::new(
                        NAME,
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} universal selector(s), found {count}",
                            max = config.max,
                        ),
                    )
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, ctx_with_options, invalid_options};

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
    let rule = configured(&SelectorMaxUniversal, opts.clone());
    let c = ctx_with_options(&opts);
    let d = rule.check(&style_with_selector("* *"), &c);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 2"));
  }

  #[test]
  fn allows_single_universal() {
    let rule = configured(&SelectorMaxUniversal, None);
    let d = rule.check(&style_with_selector("*"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_universal() {
    let rule = configured(&SelectorMaxUniversal, None);
    let d = rule.check(&style_with_selector("div .class"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_star_in_attribute_value() {
    let rule = configured(&SelectorMaxUniversal, None);
    let d = rule.check(&style_with_selector("div[class*='foo']"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn checks_each_selector_separately() {
    let rule = configured(&SelectorMaxUniversal, None);
    let d = rule.check(&style_with_selector("*, *"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn respects_custom_max() {
    let options = serde_json::json!([2]);
    let rule = configured(&SelectorMaxUniversal, options.clone());
    let ctx = ctx_with_options(&options);
    let d = rule.check(&style_with_selector("* *"), &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn ignore_after_child_combinator() {
    let opts = serde_json::json!([1, {"ignoreAfterCombinators": [">"]}]);
    let rule = configured(&SelectorMaxUniversal, opts.clone());
    let c = ctx_with_options(&opts);
    // * > * — the second * is after child combinator, should be ignored
    let d = rule.check(&style_with_selector("* > *"), &c);
    assert!(d.is_empty());
  }

  #[test]
  fn max_zero_rejects_single_universal() {
    let opts = serde_json::json!([0]);
    let rule = configured(&SelectorMaxUniversal, opts.clone());
    let c = ctx_with_options(&opts);
    let d = rule.check(&style_with_selector("*"), &c);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function, so Stylelint words its
    // problem without "value".
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([2, { "ignoreAfterCombinators": [">", "+"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(-1)),
      ["Invalid option \"-1\" for rule \"selector-max-universal\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([2, { "ignoreAfterCombinators": [1] }])
      ),
      [
        "Invalid value \"1\" for option \"ignoreAfterCombinators\" of rule \"selector-max-universal\""
      ]
    );
  }
}
