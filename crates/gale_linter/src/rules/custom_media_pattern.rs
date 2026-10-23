use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

/// Kebab-case, used when the config gives no pattern.
const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

const NAME: &str = "custom-media-pattern";

declare_rule! {
  /// Enforce a naming pattern for custom media queries.
  ///
  /// Equivalent to Stylelint's `custom-media-pattern` rule.  The primary
  /// option is the pattern the name after `--` must match; without one the
  /// rule falls back to kebab-case.  Detection-only.
  pub struct CustomMediaPattern => CustomMediaPatternCheck {
    name: NAME,
    description: "Specify a pattern for custom media query names",
    default_severity: Warning,
  }
}

/// `custom-media-pattern` as configured.
pub struct CustomMediaPatternCheck {
  /// The pattern custom media names must match.
  pattern: Pattern,
}

impl CustomMediaPatternCheck {
  /// Stylelint's option: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option.
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

impl Check for CustomMediaPatternCheck {
  /// Flags an `@custom-media` name that does not match the configured
  /// pattern, at the name itself, as Stylelint reports it.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let pattern_str = self.pattern.source.as_str();
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.name().eq_ignore_ascii_case("custom-media") {
        continue;
      }
      // @custom-media --name <media-query>
      // The params should start with the custom media name (--name).
      let params = at.params();
      let Some(full_name) = params.split_whitespace().next() else {
        continue;
      };
      let Some(name) = full_name.strip_prefix("--") else {
        continue;
      };
      if self.pattern.is_match(name) {
        continue;
      }
      let start = at.params_start() + (params.len() - params.trim_start().len());
      diags.push(
        Diagnostic::new(
          NAME,
          format!("Expected \"{full_name}\" to match pattern \"{pattern_str}\""),
        )
        .span(Span::new(start, full_name.len()))
        .message_args([full_name, pattern_str]),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::{Value, json};

  use crate::testing::{invalid_options, lint};

  fn check(options: Value, css: &str) -> Vec<Diagnostic> {
    lint(NAME, options, css, Syntax::Css)
  }

  #[test]
  fn reports_non_kebab_case() {
    let d = check(
      json!(DEFAULT_PATTERN),
      "@custom-media --myQuery (min-width: 768px);",
    );
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected \"--myQuery\" to match pattern \"^[a-z][a-z0-9]*(-[a-z0-9]+)*$\""
    );
  }

  #[test]
  fn uses_the_configured_pattern() {
    let options = json!("^(?!bp-)[a-z-]+$");
    let d = check(
      options.clone(),
      "@custom-media --bp-small (max-width: 30em);",
    );
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected \"--bp-small\" to match pattern \"^(?!bp-)[a-z-]+$\""
    );
    // Reported at the name, not the whole at-rule.
    assert_eq!(d[0].span, Span::new(14, "--bp-small".len()));
    let d = check(
      options.clone(),
      "@CUSTOM-MEDIA --bp-small (max-width: 30em);",
    );
    assert_eq!(d.len(), 1);

    assert!(check(options, "@custom-media --small (max-width: 30em);").is_empty());
  }

  #[test]
  fn allows_kebab_case() {
    let css = "@custom-media --my-query (min-width: 768px);";
    assert!(check(json!(DEFAULT_PATTERN), css).is_empty());
  }

  #[test]
  fn ignores_non_custom_media_at_rules() {
    assert!(check(json!(DEFAULT_PATTERN), "@media (min-width: 768px) {}").is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("^bp-")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"custom-media-pattern\""]
    );
  }
}
