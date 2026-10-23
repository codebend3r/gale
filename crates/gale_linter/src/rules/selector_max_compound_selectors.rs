use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-max-compound-selectors";

declare_rule! {
  /// Limit the number of compound selectors in a selector.
  ///
  /// Equivalent to Stylelint's `selector-max-compound-selectors` rule.
  /// Default maximum: 3. Counts parts separated by combinators (space, `>`, `+`, `~`).
  pub struct SelectorMaxCompoundSelectors => SelectorMaxCompoundSelectorsCheck {
    name: NAME,
    description: "Limit the number of compound selectors in a selector",
    default_severity: Warning,
  }
}

/// `selector-max-compound-selectors` as configured.
pub struct SelectorMaxCompoundSelectorsCheck {
  /// The most compound selectors allowed.
  max: usize,
}

impl SelectorMaxCompoundSelectorsCheck {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`), then `ignoreSelectors`,
  /// which gale accepts but does not yet apply.  Gale reads no number as 3.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(|value| Accept::NonNegativeInteger.accepts(value));
    options.strings("ignoreSelectors");
    options.finish()?;
    Ok(Self {
      max: max
        .and_then(serde_json::Value::as_u64)
        .map_or(MAX_COMPOUND, |max| max as usize),
    })
  }
}

const MAX_COMPOUND: usize = 3;

impl Check for SelectorMaxCompoundSelectorsCheck {
  /// Flags a selector holding more compound selectors than allowed.
  ///
  /// The selector is read as PostCSS stores it, without its comments, and
  /// rules that are not standard syntax are skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      let selector = rule.selector();
      // Check each comma-separated selector
      for sel in selector.split(',') {
        let sel = sel.trim();
        let count = count_compound_selectors(sel);
        if count > max {
          diags.push(
                    Diagnostic::new(
                        NAME,
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} compound selector(s), found {count}"
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

/// Count compound selectors by splitting on combinators (whitespace, `>`, `+`, `~`).
/// Only skips content inside attribute selectors `[...]`, NOT inside pseudo-class
/// function arguments — matching Stylelint v17 behavior where selectors are
/// evaluated as-written.
fn count_compound_selectors(selector: &str) -> usize {
  if selector.is_empty() {
    return 0;
  }

  let mut count = 1usize;
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut in_brackets = 0i32; // only tracks [ ]

  while i < len {
    // Skip attribute selectors only
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
      // Skip following whitespace
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      continue;
    }

    // Descendant combinator (whitespace between simple selectors)
    if chars[i].is_ascii_whitespace() {
      // Whitespace just inside a `(`, as in `:not( .a )`, is padding.
      let after_paren = i > 0 && chars[i - 1] == '(';
      // Skip all whitespace
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      // If not at end and next char is not a combinator or closing paren, it's a descendant combinator
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
  fn reports_too_many_compound_selectors() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    // 4 compound selectors: .a .b .c .d
    let d = rule.check_source(&context(&style_with_selector(".a .b .c .d"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 4"));
  }

  #[test]
  fn allows_within_limit() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    // 3 compound selectors: .a .b .c
    let d = rule.check_source(&context(&style_with_selector(".a .b .c"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn counts_child_combinator() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    // 4 compound selectors: .a > .b > .c > .d
    let d = rule.check_source(&context(
      &style_with_selector(".a > .b > .c > .d"),
      Syntax::Css,
    ));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn single_selector_ok() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    let d = rule.check_source(&context(&style_with_selector(".foo"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn strips_inline_scss_comments() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    // "dt, // TODO: Deprecate\n.form-group-header" should be treated as
    // "dt,\n.form-group-header" which is two selectors each with 1 compound.
    let sel = "dt, // TODO: Deprecate\n.form-group-header";
    let d = rule.check_source(&context(&style_with_selector(sel), Syntax::Scss));
    assert!(d.is_empty(), "Expected no diagnostics, got: {:?}", d);
  }

  #[test]
  fn strips_full_line_scss_comments() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    // Full-line SCSS comments should be stripped entirely
    let sel = "// This is a comment\n.foo .bar";
    let d = rule.check_source(&context(&style_with_selector(sel), Syntax::Scss));
    assert!(d.is_empty());
  }

  #[test]
  fn counts_compounds_inside_is() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    // Stylelint v17: compound selectors inside :is() ARE counted.
    // `:is(.a .b .c .d)` has 4 compound selectors.
    let d = rule.check_source(&context(
      &style_with_selector(":is(.a .b .c .d)"),
      Syntax::Css,
    ));
    assert_eq!(
      d.len(),
      1,
      "expected 1 diagnostic for 4 compounds inside :is()"
    );
    assert!(d[0].message.contains("found 4"));
  }

  #[test]
  fn counts_compounds_inside_has() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    // Stylelint v17: `.foo:has(.bar > .baz > .qux > .quux)` has compounds
    // counted across the whole selector as-written.
    let d = rule.check_source(&context(
      &style_with_selector(".foo:has(.bar > .baz > .qux > .quux)"),
      Syntax::Css,
    ));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn padding_inside_parentheses_is_no_combinator() {
    let rule = configured(&SelectorMaxCompoundSelectors, None);
    let source = style_with_selector("> div:not( :only-of-type ) .button");
    assert!(rule.check_source(&context(&source, Syntax::Css)).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function, so Stylelint words its
    // problem without "value".
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([2, { "ignoreSelectors": [":host", "/^:deep/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(-1)),
      ["Invalid option \"-1\" for rule \"selector-max-compound-selectors\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([2, { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"selector-max-compound-selectors\""]
    );
  }
}
