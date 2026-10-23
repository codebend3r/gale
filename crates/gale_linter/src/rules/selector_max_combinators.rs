use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-max-combinators";

declare_rule! {
  /// Limit the number of combinators in a selector.
  ///
  /// Equivalent to Stylelint's `selector-max-combinators` rule.
  /// Default maximum: 2. Counts combinators (>, +, ~, space) between compound selectors.
  pub struct SelectorMaxCombinators => SelectorMaxCombinatorsCheck {
    name: NAME,
    description: "Limit the number of combinators in a selector",
    default_severity: Warning,
  }
}

/// `selector-max-combinators` as configured.
pub struct SelectorMaxCombinatorsCheck {
  /// The most combinators allowed.
  max: usize,
}

impl SelectorMaxCombinatorsCheck {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`).
  /// Gale reads no number as 2.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(|value| Accept::NonNegativeInteger.accepts(value));
    options.finish()?;
    Ok(Self {
      max: max
        .and_then(serde_json::Value::as_u64)
        .map_or(DEFAULT_MAX, |max| max as usize),
    })
  }
}

const DEFAULT_MAX: usize = 2;

impl Check for SelectorMaxCombinatorsCheck {
  /// Flags a selector holding more combinators than allowed.
  ///
  /// The selector is read as PostCSS stores it, without its comments, and
  /// rules that are not standard syntax are skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      for sel in rule.selector().split(',') {
        let sel = sel.trim();
        let count = count_combinators(sel);
        if count > max {
          diags.push(
          Diagnostic::new(
            NAME,
            format!(
              "Expected selector \"{sel}\" to have no more than {max} combinator(s), found {count}"
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

/// Count combinators in a selector (>, +, ~, descendant whitespace).
/// Skips content inside attribute selectors `[...]` but NOT inside
/// pseudo-class function arguments — matching Stylelint v17 behavior
/// where selectors are evaluated as-written.
fn count_combinators(selector: &str) -> usize {
  if selector.is_empty() {
    return 0;
  }

  let mut count = 0usize;
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_brackets = 0i32; // only tracks [ ]

  while i < len {
    // Skip SCSS line comments
    if chars[i] == '/' && i + 1 < len && chars[i + 1] == '/' {
      while i < len && chars[i] != '\n' {
        i += 1;
      }
      continue;
    }

    if chars[i] == '[' {
      in_brackets += 1;
      i += 1;
      continue;
    }
    if chars[i] == ']' {
      in_brackets -= 1;
      i += 1;
      continue;
    }
    if in_brackets > 0 {
      i += 1;
      continue;
    }

    // Explicit combinators
    if chars[i] == '>' || chars[i] == '+' || chars[i] == '~' {
      count += 1;
      i += 1;
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      continue;
    }

    // Descendant combinator (whitespace between simple selectors); whitespace
    // just inside a `(`, as in `:not( .a )`, is padding.
    if chars[i].is_ascii_whitespace() {
      let after_paren = i > 0 && chars[i - 1] == '(';
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if !after_paren
        && i < len
        && chars[i] != '>'
        && chars[i] != '+'
        && chars[i] != '~'
        && chars[i] != ')'
      {
        count += 1;
      }
      continue;
    }

    i += 1;
  }

  count
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_too_many_combinators() {
    let rule = configured(&SelectorMaxCombinators, None);
    // 3 combinators: .a .b > .c + .d
    let d = rule.check_source(&context(
      &style_with_selector(".a .b > .c + .d"),
      Syntax::Css,
    ));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 3"));
  }

  #[test]
  fn allows_within_default_limit() {
    let rule = configured(&SelectorMaxCombinators, None);
    // 2 combinators: .a .b > .c
    let d = rule.check_source(&context(&style_with_selector(".a .b > .c"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn respects_configured_max() {
    let rule = configured(&SelectorMaxCombinators, serde_json::json!(1));
    let d = rule.check_source(&context(&style_with_selector(".a .b > .c"), Syntax::Css));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn single_selector_no_combinators() {
    let rule = configured(&SelectorMaxCombinators, None);
    let d = rule.check_source(&context(&style_with_selector(".foo"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      SelectorMaxCombinators::META.name,
      "selector-max-combinators"
    );
  }

  #[test]
  fn counts_combinators_inside_is() {
    let rule = configured(&SelectorMaxCombinators, serde_json::json!(0));
    // Stylelint v17: combinators inside :is() ARE counted.
    // `:is(.a .b)` has 1 descendant combinator.
    let d = rule.check_source(&context(&style_with_selector(":is(.a .b)"), Syntax::Css));
    assert_eq!(
      d.len(),
      1,
      "expected 1 diagnostic for combinator inside :is()"
    );
  }

  #[test]
  fn counts_combinators_inside_has() {
    let rule = configured(&SelectorMaxCombinators, serde_json::json!(0));
    // Stylelint v17: combinators inside :has() ARE counted.
    // `.foo:has(.bar > .baz)` has 1 child combinator inside :has().
    let d = rule.check_source(&context(
      &style_with_selector(".foo:has(.bar > .baz)"),
      Syntax::Css,
    ));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn padding_inside_parentheses_is_no_combinator() {
    let rule = configured(&SelectorMaxCombinators, None);
    let source = style_with_selector("> div:not( :only-of-type ) .button");
    assert!(rule.check_source(&context(&source, Syntax::Css)).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function, so Stylelint words its
    // problem without "value"; the rule validates no secondary options.
    assert!(invalid_options(NAME, serde_json::json!([2, { "anything": true }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(-1)),
      ["Invalid option \"-1\" for rule \"selector-max-combinators\""]
    );
  }
}
