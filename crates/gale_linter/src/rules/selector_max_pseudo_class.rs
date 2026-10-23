use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::data::is_legacy_pseudo_element;
use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-max-pseudo-class";

declare_rule! {
  /// Limit the number of pseudo-classes in a selector.
  ///
  /// Equivalent to Stylelint's `selector-max-pseudo-class` rule.
  /// Default maximum: 1.
  pub struct SelectorMaxPseudoClass => SelectorMaxPseudoClassCheck {
    name: NAME,
    description: "Limit the number of pseudo-classes in a selector",
    default_severity: Warning,
  }
}

/// `selector-max-pseudo-class` as configured.
pub struct SelectorMaxPseudoClassCheck {
  /// The most pseudo-classes allowed.
  max: usize,
}

impl SelectorMaxPseudoClassCheck {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`).
  /// Gale reads no number as 1.
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

const DEFAULT_MAX: usize = 1;

impl Check for SelectorMaxPseudoClassCheck {
  /// Flags a selector holding more pseudo-classes than allowed.
  ///
  /// The selector is read as PostCSS stores it, without its comments, and
  /// rules that are not standard syntax are skipped, as in Stylelint.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      for sel in rule.selector().split(',') {
        let sel = sel.trim();
        let count = count_pseudo_classes(sel);
        if count > max {
          diags.push(
                    Diagnostic::new(
                        NAME,
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} pseudo-class(es), found {count}"
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

/// Count pseudo-classes in a selector (:name patterns, excluding ::pseudo-elements).
/// Counts pseudo-classes inside functional pseudo-class arguments (`:not()`,
/// `:is()`, `:where()`, `:has()`, etc.) — matching Stylelint v17 behavior
/// where selectors are evaluated as-written.
fn count_pseudo_classes(selector: &str) -> usize {
  let mut count = 0usize;
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    // Skip attribute selectors
    if chars[i] == '[' {
      let mut depth = 1;
      i += 1;
      while i < len && depth > 0 {
        match chars[i] {
          '[' => depth += 1,
          ']' => depth -= 1,
          '"' | '\'' => {
            let quote = chars[i];
            i += 1;
            while i < len && chars[i] != quote {
              if chars[i] == '\\' {
                i += 1;
              }
              i += 1;
            }
          }
          _ => {}
        }
        i += 1;
      }
      continue;
    }
    // Skip pseudo-elements (::name)
    if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      // Skip parenthesized argument of pseudo-element (e.g. ::slotted(...))
      // but do NOT recurse into it for pseudo-class counting
      if i < len && chars[i] == '(' {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if chars[i] == '(' {
            depth += 1;
          } else if chars[i] == ')' {
            depth -= 1;
          }
          i += 1;
        }
      }
      continue;
    }
    if chars[i] == ':' {
      i += 1;
      let start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
        i += 1;
      }
      // The four pseudo-elements CSS 2 wrote with one colon are elements,
      // as Stylelint counts them.
      let name: String = chars[start..i].iter().collect();
      if i > start && !is_legacy_pseudo_element(&name) {
        count += 1;
      }
      // Do NOT skip parenthesized arguments — continue scanning so that
      // pseudo-classes inside (e.g. `:not(:hover)`) are also counted.
    } else {
      i += 1;
    }
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
  fn reports_too_many_pseudo_classes() {
    let rule = configured(&SelectorMaxPseudoClass, None);
    // 2 pseudo-classes: :hover:focus
    let d = rule.check_source(&context(&style_with_selector("a:hover:focus"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 2"));
  }

  #[test]
  fn allows_within_default_limit() {
    let rule = configured(&SelectorMaxPseudoClass, None);
    let d = rule.check_source(&context(&style_with_selector("a:hover"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn respects_configured_max() {
    let rule = configured(&SelectorMaxPseudoClass, serde_json::json!(2));
    let d = rule.check_source(&context(&style_with_selector("a:hover:focus"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_count_pseudo_elements() {
    let rule = configured(&SelectorMaxPseudoClass, None);
    let d = rule.check_source(&context(
      &style_with_selector("a::before:hover"),
      Syntax::Css,
    ));
    assert!(d.is_empty());
  }

  #[test]
  fn does_not_count_legacy_single_colon_pseudo_elements() {
    let rule = configured(&SelectorMaxPseudoClass, None);
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      let source = style_with_selector("a:hover:BEFORE, p:first-line:focus");
      assert!(rule.check_source(&context(&source, syntax)).is_empty());
    }
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      SelectorMaxPseudoClass::META.name,
      "selector-max-pseudo-class"
    );
  }

  #[test]
  fn counts_pseudo_classes_inside_not() {
    let rule = configured(&SelectorMaxPseudoClass, None);
    // Stylelint v17: `:not(:hover)` counts as 2 pseudo-classes (`:not` and `:hover`).
    let d = rule.check_source(&context(&style_with_selector("a:not(:hover)"), Syntax::Css));
    assert_eq!(
      d.len(),
      1,
      "expected 1 diagnostic for 2 pseudo-classes with max 1"
    );
    assert!(d[0].message.contains("found 2"));
  }

  #[test]
  fn counts_pseudo_classes_inside_is() {
    let rule = configured(&SelectorMaxPseudoClass, None);
    // Stylelint v17: `a:is(:hover)` counts as 2 pseudo-classes (`:is` and `:hover`).
    let d = rule.check_source(&context(&style_with_selector("a:is(:hover)"), Syntax::Css));
    assert_eq!(
      d.len(),
      1,
      "expected 1 diagnostic for 2 pseudo-classes with max 1"
    );
    assert!(d[0].message.contains("found 2"));
  }

  #[test]
  fn allows_pseudo_classes_inside_functional_within_limit() {
    let rule = configured(&SelectorMaxPseudoClass, serde_json::json!(2));
    // `:not(:hover)` = 2 pseudo-classes, max 2 should be OK
    let d = rule.check_source(&context(&style_with_selector("a:not(:hover)"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function, so Stylelint words its
    // problem without "value"; the rule validates no secondary options.
    assert!(invalid_options(NAME, serde_json::json!([2, { "anything": true }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(-1)),
      ["Invalid option \"-1\" for rule \"selector-max-pseudo-class\""]
    );
  }
}
