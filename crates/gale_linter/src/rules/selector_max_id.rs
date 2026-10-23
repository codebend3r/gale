use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{blank_comments, split_selectors};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-max-id";

declare_rule! {
  /// Limit the number of ID selectors in a selector.
  ///
  /// Equivalent to Stylelint's `selector-max-id` rule.
  /// Default maximum: 0 (disallow ID selectors entirely).
  pub struct SelectorMaxId => SelectorMaxIdCheck {
    name: NAME,
    description: "Limit the number of ID selectors in a selector",
    default_severity: Warning,
  }
}

/// `selector-max-id` as configured.
pub struct SelectorMaxIdCheck {
  /// The most ID selectors a selector may hold.
  max: usize,
}

/// The number of IDs allowed when the config gives none.
const MAX_ID: usize = 0;

impl SelectorMaxIdCheck {
  /// Stylelint's options: a whole number of zero or more (tested by a
  /// function, so a problem reads `Invalid option`), then
  /// `ignoreContextFunctionalPseudoClasses`, which gale accepts but does not
  /// yet apply.  Gale reads no number as 0.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(|value| Accept::NonNegativeInteger.accepts(value));
    options.strings("ignoreContextFunctionalPseudoClasses");
    options.finish()?;
    Ok(Self {
      max: max
        .and_then(serde_json::Value::as_f64)
        .map_or(MAX_ID, |max| max as usize),
    })
  }
}

impl Check for SelectorMaxIdCheck {
  /// Flags a selector holding more ID selectors than allowed.
  ///
  /// The selector is read as written, and rules that are not standard syntax
  /// (interpolation, placeholders, Less mixins) are skipped, as Stylelint
  /// skips them.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

    // Stylelint checks each individual selector in a comma-separated
    // selector list and reports a separate diagnostic for each one that
    // exceeds the limit.  Match this behaviour.
    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      let selector_source_start = rule.selector_start();
      let source_selector = rule.raw_selector();
      let source_selector = source_selector.as_ref();

      // Comments are blanked out, byte for byte, so that a comma or an `#id`
      // in one is not read; the message quotes the selector as written.
      let blanked = blank_comments(source_selector);
      for (start, part) in split_selectors(&blanked) {
        let trimmed = part.trim();
        if trimmed.is_empty() {
          continue;
        }
        let count = count_id_selectors(trimmed);
        if count > max {
          let begin = start + part.len() - part.trim_start().len();
          let text = &source_selector[begin..begin + trimmed.len()];
          diags.push(
            Diagnostic::new(
              NAME,
              format!(
                "Expected \"{}\" to have no more than {max} ID selector{}",
                text,
                if max == 1 { "" } else { "s" },
              ),
            )
            .span(Span::new(selector_source_start + begin, trimmed.len())),
          );
        }
      }
    }
    diags
  }
}

/// Count `#` characters that are ID selectors (not hex color fragments or SCSS interpolation).
/// A `#` is an ID selector if it is followed by a CSS identifier start character
/// (letter, underscore, hyphen, or non-ASCII) but NOT `{` (which indicates SCSS interpolation `#{...}`).
fn count_id_selectors(selector: &str) -> usize {
  let chars: Vec<char> = selector.chars().collect();
  let mut count = 0;
  let mut i = 0;
  while i < chars.len() {
    if chars[i] == '#'
      && let Some(&next) = chars.get(i + 1)
    {
      if next == '{' {
        // SCSS interpolation #{...} — skip past the closing `}`
        i += 2;
        let mut depth = 1;
        while i < chars.len() && depth > 0 {
          if chars[i] == '{' {
            depth += 1;
          } else if chars[i] == '}' {
            depth -= 1;
          }
          i += 1;
        }
        continue;
      } else if next.is_ascii_alphabetic() || next == '_' || next == '-' || !next.is_ascii() {
        count += 1;
      }
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
  fn reports_id_selector() {
    let d = configured(&SelectorMaxId, None)
      .check_source(&context(&style_with_selector("#foo"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#foo"));
  }

  #[test]
  fn allows_class_selector() {
    let d = configured(&SelectorMaxId, None)
      .check_source(&context(&style_with_selector(".bar"), Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn counts_multiple_ids() {
    let d = configured(&SelectorMaxId, None)
      .check_source(&context(&style_with_selector("#a #b"), Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#a #b"));
  }

  #[test]
  fn skips_scss_interpolation() {
    // #{$var} is SCSS interpolation, not an ID selector
    let d = configured(&SelectorMaxId, None).check_source(&context(
      &style_with_selector(".#{$prefix}-item"),
      Syntax::Css,
    ));
    assert!(
      d.is_empty(),
      "SCSS interpolation should not count as ID selector"
    );
  }

  #[test]
  fn a_configured_maximum_allows_that_many_ids() {
    let rule = configured(
      &SelectorMaxId,
      serde_json::json!([1, { "ignoreContextFunctionalPseudoClasses": [":not"] }]),
    );
    assert!(
      rule
        .check_source(&context(&style_with_selector("#a"), Syntax::Css))
        .is_empty()
    );
    assert_eq!(
      rule
        .check_source(&context(&style_with_selector("#a #b"), Syntax::Css))
        .len(),
      1
    );
  }

  #[test]
  fn reports_each_selector_where_it_is_written() {
    let rule = configured(&SelectorMaxId, None);
    let source = "a,\n  #b /* #c, #d */ .e,\n.f /* #g */ {}";
    let d = rule.check_source(&context(source, Syntax::Css));
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected \"#b /* #c, #d */ .e\" to have no more than 0 ID selectors"
    );
    assert_eq!(d[0].span.offset, source.find("#b").unwrap());
    // Interpolation makes the selector non-standard, so it is skipped as in
    // Stylelint, rather than cut short at the `{` of `#{`.
    let scss = ".a-#{$b} #c {}";
    assert!(rule.check_source(&context(scss, Syntax::Scss)).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The primary option is tested by a function, so Stylelint words its
    // problem without "value".
    assert_eq!(
      invalid_options(NAME, serde_json::json!(-1)),
      ["Invalid option \"-1\" for rule \"selector-max-id\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([0, { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"selector-max-id\""]
    );
  }
}
