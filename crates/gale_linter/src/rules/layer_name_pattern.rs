use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

const NAME: &str = "layer-name-pattern";

declare_rule! {
  /// Enforces a naming pattern for CSS layer names.
  ///
  /// Checks `@layer` at-rules. Layer names can be dotted (`base.utilities`)
  /// and comma-separated (`@layer reset, base`). Each segment of a dotted
  /// name is validated individually.
  ///
  /// Accepts a regex string as the primary option.
  /// Defaults to kebab-case pattern if no option is provided.
  ///
  /// Equivalent to Stylelint's `layer-name-pattern` rule.
  pub struct LayerNamePattern => LayerNamePatternCheck {
    name: NAME,
    description: "Specify a pattern for layer names",
    default_severity: Warning,
  }
}

/// `layer-name-pattern` as configured.
pub struct LayerNamePatternCheck {
  /// The pattern every segment of a layer name must match.
  pattern: Pattern,
}

impl LayerNamePatternCheck {
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
    Ok(Self { pattern })
  }
}

impl Check for LayerNamePatternCheck {
  /// Flags `@layer` names that do not match the configured regex, validating each
  /// segment of a dotted name separately.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if !at.name.eq_ignore_ascii_case("layer") {
      return vec![];
    }

    let params = at.params.trim();
    if params.is_empty() {
      return vec![];
    }

    let pattern_str = self.pattern.source.as_str();

    let mut diags = Vec::new();

    // Layer names can be comma-separated: `@layer reset, base, utilities`
    for layer_name in params.split(',') {
      let layer_name = layer_name.trim();
      if layer_name.is_empty() {
        continue;
      }

      // Layer names can be dotted: `base.utilities` — validate each segment
      for segment in layer_name.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
          continue;
        }
        if !self.pattern.is_match(segment) {
          diags.push(
            Diagnostic::new(
              NAME,
              format!(
                "Expected layer name segment \"{segment}\" to match pattern \"{pattern_str}\""
              ),
            )
            .span(Span::new(at.span.offset, at.span.length))
            .message_args([segment, pattern_str]),
          );
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan};

  use crate::testing::{configured, ctx, ctx_with_options};

  fn layer(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "layer".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_camel_case_layer_name() {
    let rule = configured(&LayerNamePattern, None);
    let d = rule.check(&layer("myLayer"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myLayer"));
  }

  #[test]
  fn allows_kebab_case_layer_name() {
    let rule = configured(&LayerNamePattern, None);
    let d = rule.check(&layer("my-layer"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_bad_segment_in_dotted_name() {
    let rule = configured(&LayerNamePattern, None);
    let d = rule.check(&layer("base.myUtils"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myUtils"));
  }

  #[test]
  fn allows_kebab_case_dotted_name() {
    let rule = configured(&LayerNamePattern, None);
    let d = rule.check(&layer("base.utilities"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_bad_names_in_comma_list() {
    let rule = configured(&LayerNamePattern, None);
    let d = rule.check(&layer("reset, myBase, utilities"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myBase"));
  }

  #[test]
  fn allows_all_kebab_case_comma_list() {
    let rule = configured(&LayerNamePattern, None);
    let d = rule.check(&layer("reset, base, utilities"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_non_layer_at_rule() {
    let rule = configured(&LayerNamePattern, None);
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
  fn custom_pattern() {
    let opts = serde_json::json!("^[A-Z][a-zA-Z0-9]+$");
    let rule = configured(&LayerNamePattern, opts.clone());
    let c = ctx_with_options(&opts);
    // PascalCase should pass
    assert!(rule.check(&layer("MyLayer"), &c).is_empty());
    // kebab-case should fail
    let d = rule.check(&layer("my-layer"), &c);
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
      ["Invalid option value \"5\" for rule \"layer-name-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-z\" for rule \"layer-name-pattern\": "),
      "{broken:?}"
    );
  }
}
