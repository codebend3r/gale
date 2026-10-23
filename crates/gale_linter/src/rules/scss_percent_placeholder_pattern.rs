use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*-?$";

const NAME: &str = "scss/percent-placeholder-pattern";

declare_rule! {
  /// Specify a pattern for `%placeholder` selectors.
  ///
  /// Accepts a regex string as the primary option. Defaults to kebab-case.
  /// The `%` prefix is stripped before matching.
  ///
  /// Equivalent to `scss/percent-placeholder-pattern`.
  pub struct ScssPercentPlaceholderPattern => ScssPercentPlaceholderPatternCheck {
    name: NAME,
    description: "Specify a pattern for %placeholder selectors",
    default_severity: Warning,
  }
}

/// `scss/percent-placeholder-pattern` as configured.
pub struct ScssPercentPlaceholderPatternCheck {
  /// The pattern every name must match.
  pattern: Pattern,
}

impl ScssPercentPlaceholderPatternCheck {
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

impl Check for ScssPercentPlaceholderPatternCheck {
  /// Flags a `%placeholder` name that does not match the configured regex,
  /// defaulting to kebab-case.
  /// The selector is read as PostCSS stores it, without its comments.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let pattern_str = self.pattern.source.as_str();

    let mut diags = Vec::new();

    for rule in tree.walk_rules() {
      // Check each selector in the comma-separated list
      for part in rule.selector().split(',') {
        let sel = part.trim();
        // Look for %placeholder within the selector
        for segment in sel.split_whitespace() {
          let segment = segment.trim_start_matches('&');
          if let Some(rest) = segment.strip_prefix('%') {
            // If the placeholder contains SCSS interpolation #{}, skip validation
            // since the final name is dynamic
            if rest.contains("#{") {
              continue;
            }

            // Strip any trailing pseudo-class or combinator chars
            let placeholder_name = rest
              .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
              .next()
              .unwrap_or("");

            if placeholder_name.is_empty() {
              continue;
            }

            if !self.pattern.is_match(placeholder_name) {
              diags.push(
                Diagnostic::new(
                  NAME,
                  format!(
                    "Expected %{} to match pattern \"{}\"",
                    placeholder_name, pattern_str
                  ),
                )
                .span(rule.span()),
              );
            }
          }
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{
    CheckSource, configured, ctx_with_source, invalid_options, scss_ctx_with_source,
  };

  /// An empty rule with the selector `selector`.
  fn style(selector: &str) -> String {
    format!("{selector} {{}}")
  }

  #[test]
  fn allows_kebab_case_placeholder() {
    let rule = configured(&ScssPercentPlaceholderPattern, None);
    assert!(
      rule
        .check_source(&scss_ctx_with_source(&style("%my-placeholder")))
        .is_empty()
    );
  }

  #[test]
  fn allows_trailing_hyphen() {
    let rule = configured(&ScssPercentPlaceholderPattern, None);
    assert!(
      rule
        .check_source(&scss_ctx_with_source(&style("%responsive-container-")))
        .is_empty()
    );
  }

  #[test]
  fn skips_interpolated_placeholder() {
    let rule = configured(&ScssPercentPlaceholderPattern, None);
    assert!(
      rule
        .check_source(&scss_ctx_with_source(&style(
          "%responsive-container-#{$breakpoint}"
        )))
        .is_empty()
    );
  }

  #[test]
  fn reports_non_matching_placeholder() {
    let rule = configured(&ScssPercentPlaceholderPattern, None);
    let d = rule.check_source(&scss_ctx_with_source(&style("%MyPlaceholder")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("MyPlaceholder"));
  }

  #[test]
  fn skips_non_placeholder_selectors() {
    let rule = configured(&ScssPercentPlaceholderPattern, None);
    assert!(
      rule
        .check_source(&scss_ctx_with_source(&style(".my-class")))
        .is_empty()
    );
  }

  #[test]
  fn skips_non_scss() {
    let rule = configured(&ScssPercentPlaceholderPattern, None);
    assert!(
      rule
        .check_source(&ctx_with_source(&style("%MyPlaceholder")))
        .is_empty()
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("^[a-z]+$")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("/^[a-z]+$/i")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"scss/percent-placeholder-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with(
        "Invalid option value \"^[a-z\" for rule \"scss/percent-placeholder-pattern\": "
      ),
      "{broken:?}"
    );
  }
}
