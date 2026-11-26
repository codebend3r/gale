use crate::at_name_pattern::{AtNamePattern, AtNamePatternCheck};
use crate::rule::declare_rule;

const NAME: &str = "scss/at-mixin-pattern";

declare_rule! {
  /// Specify a pattern for SCSS `@mixin` names.
  ///
  /// Accepts a regex string as the primary option. Defaults to kebab-case.
  ///
  /// Equivalent to `scss/at-mixin-pattern`.
  pub struct ScssAtMixinPattern => AtNamePatternCheck<ScssAtMixinPattern> {
    name: NAME,
    description: "Specify a pattern for SCSS mixin names",
    default_severity: Warning,
  }
}

impl AtNamePattern for ScssAtMixinPattern {
  const NAME: &'static str = NAME;
  const AT_RULE: &'static str = "mixin";
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use gale_diagnostics::Diagnostic;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The reports for `source` in `syntax`, with the default pattern.
  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&ScssAtMixinPattern, None).check_source(&context(source, syntax))
  }

  #[test]
  fn allows_kebab_case() {
    assert!(check("@mixin my-mixin($a) { }", Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_non_matching_pattern_at_the_name() {
    let d = check("a { @mixin MyMixin($a) { } }", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MyMixin"));
    assert_eq!((d[0].span.offset, d[0].span.length), (11, 7));
  }

  #[test]
  fn skips_non_mixin() {
    assert!(check("@function MyFunc() { }", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(check("@mixin MyMixin { }", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("^[a-z]+$")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("/^[a-z]+$/i")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"scss/at-mixin-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-z\" for rule \"scss/at-mixin-pattern\": "),
      "{broken:?}"
    );
  }
}
