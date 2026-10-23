use gale_css_parser::postcss_tree::PostcssTree;
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
  ///
  /// The selector is read as PostCSS stores it, without its comments, and
  /// rules that are not standard syntax are skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      let selector = rule.selector();
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
                    .span(rule.span()),
                );
        }
      }
    }
    diags
  }
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

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_too_many_classes() {
    let rule = configured(&SelectorMaxClass, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector(".a .b .c .d")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 4"));
  }

  #[test]
  fn allows_within_limit() {
    let rule = configured(&SelectorMaxClass, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector(".a .b .c")));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_no_classes() {
    let rule = configured(&SelectorMaxClass, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector("div span")));
    assert!(d.is_empty());
  }

  #[test]
  fn reads_the_selector_without_its_comments() {
    use crate::testing::context;
    use gale_css_parser::Syntax;
    let rule = configured(&SelectorMaxClass, None);
    let source = ".a .b, // .x .y\n.c .d /* .e */ .f .g {}";
    let d = rule.check_source(&context(source, Syntax::Scss));
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected selector \".c .d  .f .g\" to have no more than 3 class selector(s), found 4"
    );
    // Not standard syntax, so not read.
    let less = ".m() when (@a) { .a .b .c .d {} }\n.n(@x) {}";
    assert_eq!(rule.check_source(&context(less, Syntax::Less)).len(), 1);
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
