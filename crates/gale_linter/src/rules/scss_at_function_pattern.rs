use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

const NAME: &str = "scss/at-function-pattern";

declare_rule! {
  /// Specify a pattern for SCSS `@function` names.
  ///
  /// Accepts a regex string as the primary option. Defaults to kebab-case.
  ///
  /// Equivalent to `scss/at-function-pattern`.
  pub struct ScssAtFunctionPattern => ScssAtFunctionPatternCheck {
    name: NAME,
    description: "Specify a pattern for SCSS function names",
    default_severity: Warning,
  }
}

/// `scss/at-function-pattern` as configured.
pub struct ScssAtFunctionPatternCheck {
  /// The pattern every name must match.
  pattern: Pattern,
}

impl ScssAtFunctionPatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option, as Stylelint reports it.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.finish()?;
    let pattern = match pattern {
      Some(pattern) => pattern,
      None => Pattern::compile(DEFAULT_PATTERN).expect("the default pattern compiles"),
    };
    Ok(Self { pattern })
  }
}

impl Check for ScssAtFunctionPatternCheck {
  /// Flags an `@function` name that does not match the configured regex,
  /// defaulting to kebab-case, at the name as stylelint-scss finds it.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }
    let pattern_str = self.pattern.source.as_str();
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if at.name() != "function" {
        continue;
      }
      // The function name is the first word in params (before `(`)
      let first_token = at.params().split_whitespace().next().unwrap_or("");
      let func_name = first_token.split('(').next().unwrap_or("");
      if func_name.is_empty() || self.pattern.is_match(func_name) {
        continue;
      }
      diags.push(
        Diagnostic::new(
          NAME,
          format!("Expected @function name \"{func_name}\" to match pattern \"{pattern_str}\""),
        )
        .span(at.word_span(func_name)),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// The reports for `source` in `syntax`, with the default pattern.
  fn check(source: &str, syntax: Syntax) -> Vec<Diagnostic> {
    configured(&ScssAtFunctionPattern, None).check_source(&context(source, syntax))
  }

  #[test]
  fn allows_kebab_case() {
    assert!(check("@function my-func($a) { @return 1; }", Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_non_matching_pattern_at_the_name() {
    let d = check("a {}\n@function MyFunc($a) { @return 1; }", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MyFunc"));
    assert_eq!((d[0].span.offset, d[0].span.length), (15, 6));
  }

  #[test]
  fn skips_non_function_at_rule() {
    assert!(check("@mixin MyMixin { }", Syntax::Scss).is_empty());
  }

  #[test]
  fn skips_non_scss() {
    assert!(check("@function MyFunc { }", Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("^[a-z]+$")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("/^[a-z]+$/i")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"scss/at-function-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0]
        .starts_with("Invalid option value \"^[a-z\" for rule \"scss/at-function-pattern\": "),
      "{broken:?}"
    );
  }
}
