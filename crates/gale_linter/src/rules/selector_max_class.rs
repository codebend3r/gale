use crate::max_rules::{MaxCheck, MaxRule};
use crate::rule::declare_rule;

const NAME: &str = "selector-max-class";

declare_rule! {
  /// Limit the number of class selectors in a selector.
  ///
  /// Equivalent to Stylelint's `selector-max-class` rule.
  /// Default maximum: 3. Detection-only.
  pub struct SelectorMaxClass => MaxCheck<SelectorMaxClass> {
    name: NAME,
    description: "Limit the number of class selectors in a selector",
    default_severity: Warning,
  }
}

impl MaxRule for SelectorMaxClass {
  const NAME: &'static str = NAME;
  const DEFAULT: usize = 3;
  type Options = ();

  fn count(selector: &str, _: &()) -> usize {
    count_class_selectors(selector)
  }

  fn message(selector: &str, max: usize, count: usize) -> String {
    format!(
      "Expected selector \"{selector}\" to have no more than {max} class selector(s), found {count}"
    )
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
  fn a_whole_float_maximum_is_that_maximum() {
    // `5.0` is a whole number, so it validates; it used to be read as no
    // number at all, leaving the default of 3 in force.
    let rule = configured(&SelectorMaxClass, serde_json::json!(5.0));
    let four = style_with_selector(".a .b .c .d");
    assert!(rule.check_source(&ctx_with_source(&four)).is_empty());
    let six = style_with_selector(".a .b .c .d .e .f");
    assert_eq!(rule.check_source(&ctx_with_source(&six)).len(), 1);
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
