use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

const NAME: &str = "keyframes-name-pattern";

declare_rule! {
  /// Enforces a naming pattern for keyframes names.
  ///
  /// Checks `@keyframes` at-rules, vendor-prefixed or not.
  /// Accepts a regex string as the primary option.
  /// Defaults to kebab-case pattern if no option is provided.
  ///
  /// Equivalent to Stylelint's `keyframes-name-pattern` rule.
  pub struct KeyframesNamePattern => KeyframesNamePatternCheck {
    name: NAME,
    description: "Specify a pattern for keyframes names",
    default_severity: Warning,
  }
}

/// `keyframes-name-pattern` as configured.
pub struct KeyframesNamePatternCheck {
  /// The pattern every keyframes name must match.
  pattern: Pattern,
  /// The `message` every rule accepts, which this rule fills in with the
  /// name itself.
  custom_message: Option<String>,
}

impl KeyframesNamePatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
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
    let custom_message = setting
      .secondary()
      .and_then(|secondary| secondary.get("message"))
      .and_then(|message| message.as_str())
      .map(String::from);
    Ok(Self {
      pattern,
      custom_message,
    })
  }
}

impl Check for KeyframesNamePatternCheck {
  /// Flags a `@keyframes` name that does not match the configured regex,
  /// defaulting to kebab-case, at the name.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let pattern_str = self.pattern.source.as_str();
    let custom_message = self.custom_message.as_deref();
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.is_keyframes() {
        continue;
      }
      let params = at.params();
      let name = params.trim();
      // Skip names with SCSS/Less interpolation — the actual runtime value
      // is unknown, so pattern matching would produce false positives.
      if name.is_empty() || name.contains("#{") || name.contains("@{") {
        continue;
      }
      let name_offset = at.params_start() + (params.len() - params.trim_start().len());

      // Strip optional quotes around the keyframes name
      let name = name
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| name.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(name);

      if self.pattern.is_match(name) {
        continue;
      }
      let message = if let Some(tmpl) = custom_message {
        tmpl.replace("${name}", name)
      } else {
        format!("Expected keyframes name \"{name}\" to match pattern \"{pattern_str}\"")
      };
      diags.push(
        Diagnostic::new(NAME, message)
          .span(Span::new(name_offset, name.len()))
          .message_args([name, pattern_str]),
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

  use crate::testing::lint;

  /// The reports for `@at-rule params {}` in CSS.
  fn check(options: Value, at_rule: &str, params: &str) -> Vec<Diagnostic> {
    lint(
      NAME,
      options,
      &format!("@{at_rule} {params} {{}}"),
      Syntax::Css,
    )
  }

  #[test]
  fn reports_camel_case_keyframes_at_the_name() {
    let d = check(json!(DEFAULT_PATTERN), "keyframes", "fadeIn");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fadeIn"));
    assert_eq!((d[0].span.offset, d[0].span.length), (11, 6));
  }

  #[test]
  fn allows_kebab_case_keyframes() {
    assert!(check(json!(DEFAULT_PATTERN), "keyframes", "fade-in").is_empty());
  }

  #[test]
  fn checks_prefixed_keyframes() {
    for at_rule in ["-webkit-keyframes", "-moz-keyframes", "KEYFRAMES"] {
      let d = check(json!(DEFAULT_PATTERN), at_rule, "slideIn");
      assert_eq!(d.len(), 1, "{at_rule}");
      assert!(d[0].message.contains("slideIn"));
    }
    let scss = "a {}\n@-webkit-keyframes slideIn {}";
    let d = lint(NAME, json!(DEFAULT_PATTERN), scss, Syntax::Scss);
    assert_eq!((d[0].span.offset, d[0].span.length), (24, 7));
  }

  #[test]
  fn ignores_non_keyframes_at_rule_and_interpolation() {
    assert!(check(json!(DEFAULT_PATTERN), "media", "(min-width: 768px)").is_empty());
    let scss = "@keyframes #{$name}In {}";
    assert!(lint(NAME, json!(DEFAULT_PATTERN), scss, Syntax::Scss).is_empty());
  }

  #[test]
  fn custom_pattern() {
    let opts = json!("^[a-z][a-zA-Z0-9]+$");
    // camelCase should pass
    assert!(check(opts.clone(), "keyframes", "fadeIn").is_empty());
    // kebab-case should fail
    assert_eq!(check(opts, "keyframes", "fade-in").len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(NAME, serde_json::json!(["/^[a-z]+$/i", { "message": "x" }])).is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"keyframes-name-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-z\" for rule \"keyframes-name-pattern\": "),
      "{broken:?}"
    );
  }
}
