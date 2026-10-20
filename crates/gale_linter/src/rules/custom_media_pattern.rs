use gale_css_parser::CssNode;
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
  /// pattern, at the name itself.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "custom-media" {
      return vec![];
    }

    let pattern_str = self.pattern.source.as_str();

    // @custom-media --name <media-query>
    // The params should start with the custom media name (--name).
    let Some(full_name) = at.params.split_whitespace().next() else {
      return vec![];
    };
    let Some(name) = full_name.strip_prefix("--") else {
      return vec![];
    };
    if self.pattern.is_match(name) {
      return vec![];
    }

    // Point at the name in the source when it can be found there, as
    // Stylelint does, else at the whole at-rule.
    let span = ctx
      .source_slice(at.span.offset, at.span.end())
      .and_then(|text| text.find(full_name))
      .map(|at_name| Span::new(at.span.offset + at_name, full_name.len()))
      .unwrap_or(Span::new(at.span.offset, at.span.length));

    vec![
      Diagnostic::new(
        NAME,
        format!("Expected \"{full_name}\" to match pattern \"{pattern_str}\""),
      )
      .span(span)
      .message_args([full_name, pattern_str]),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan};

  use crate::testing::{configured, ctx, ctx_with_source, invalid_options};

  fn custom_media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "custom-media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_non_kebab_case() {
    let rule = configured(&CustomMediaPattern, None);
    let d = rule.check(&custom_media("--myQuery (min-width: 768px)"), &ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected \"--myQuery\" to match pattern \"^[a-z][a-z0-9]*(-[a-z0-9]+)*$\""
    );
  }

  #[test]
  fn uses_the_configured_pattern() {
    let options = serde_json::json!("^(?!bp-)[a-z-]+$");
    let rule = configured(&CustomMediaPattern, options.clone());
    let source = "@custom-media --bp-small (max-width: 30em);";
    let ctx = ctx_with_source(source);
    let mut node = custom_media("--bp-small (max-width: 30em)");
    if let CssNode::AtRule(at) = &mut node {
      at.span = ParserSpan::new(0, source.len());
    }
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected \"--bp-small\" to match pattern \"^(?!bp-)[a-z-]+$\""
    );
    // Reported at the name, not the whole at-rule.
    assert_eq!(d[0].span, Span::new(14, "--bp-small".len()));

    let ok = custom_media("--small (max-width: 30em)");
    assert!(rule.check(&ok, &ctx).is_empty());
  }

  #[test]
  fn allows_kebab_case() {
    let rule = configured(&CustomMediaPattern, None);
    let d = rule.check(&custom_media("--my-query (min-width: 768px)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_custom_media_at_rules() {
    let rule = configured(&CustomMediaPattern, None);
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    let d = rule.check(&node, &ctx());
    assert!(d.is_empty());
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
