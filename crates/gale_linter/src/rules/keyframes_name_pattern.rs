use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

const NAME: &str = "keyframes-name-pattern";

declare_rule! {
  /// Enforces a naming pattern for keyframes names.
  ///
  /// Checks `@keyframes` and `@-webkit-keyframes` at-rules.
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
  /// defaulting to kebab-case.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    let lower_name = at.name.to_ascii_lowercase();
    if lower_name != "keyframes" && lower_name != "-webkit-keyframes" {
      return vec![];
    }

    let pattern_str = self.pattern.source.as_str();
    let custom_message = self.custom_message.as_deref();

    let name = at.params.trim();
    if name.is_empty() {
      return vec![];
    }

    // Skip names with SCSS/Less interpolation — the actual runtime value
    // is unknown, so pattern matching would produce false positives.
    if name.contains("#{") || name.contains("@{") {
      return vec![];
    }

    // Strip optional quotes around the keyframes name
    let name = name
      .strip_prefix('"')
      .and_then(|s| s.strip_suffix('"'))
      .or_else(|| name.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
      .unwrap_or(name);

    if !self.pattern.is_match(name) {
      let message = if let Some(tmpl) = custom_message {
        tmpl.replace("${name}", name)
      } else {
        format!("Expected keyframes name \"{name}\" to match pattern \"{pattern_str}\"")
      };

      // Compute the offset of the keyframes name in the source.
      // The name starts after `@keyframes ` (or `@-webkit-keyframes `).
      let name_offset = if at.span.offset < ctx.source.len() {
        let source_slice = ctx.source_from(at.span.offset).unwrap_or("");
        // Find where at.params starts in the source
        let at_keyword = format!("@{}", at.name);
        if let Some(kw_pos) = source_slice.find(&at_keyword) {
          let after_kw = &source_slice[kw_pos + at_keyword.len()..];
          let leading_ws = after_kw.len() - after_kw.trim_start().len();
          at.span.offset + kw_pos + at_keyword.len() + leading_ws
        } else {
          at.span.offset
        }
      } else {
        at.span.offset
      };

      return vec![
        Diagnostic::new(NAME, message)
          .span(Span::new(name_offset, name.len()))
          .message_args([name, pattern_str]),
      ];
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::AtRule;

  use crate::testing::{configured, ctx};

  fn keyframes(name: &str, params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: name.to_string(),
      params: params.to_string(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_camel_case_keyframes() {
    let rule = configured(&KeyframesNamePattern, None);
    let d = rule.check(&keyframes("keyframes", "fadeIn"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("fadeIn"));
  }

  #[test]
  fn allows_kebab_case_keyframes() {
    let rule = configured(&KeyframesNamePattern, None);
    let d = rule.check(&keyframes("keyframes", "fade-in"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn checks_webkit_keyframes() {
    let rule = configured(&KeyframesNamePattern, None);
    let d = rule.check(&keyframes("-webkit-keyframes", "slideIn"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("slideIn"));
  }

  #[test]
  fn ignores_non_keyframes_at_rule() {
    let rule = configured(&KeyframesNamePattern, None);
    let d = rule.check(&keyframes("media", "(min-width: 768px)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn custom_pattern() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let rule = configured(&KeyframesNamePattern, opts.clone());
    let c = ctx();
    // camelCase should pass
    assert!(rule.check(&keyframes("keyframes", "fadeIn"), &c).is_empty());
    // kebab-case should fail
    let d = rule.check(&keyframes("keyframes", "fade-in"), &c);
    assert_eq!(d.len(), 1);
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
