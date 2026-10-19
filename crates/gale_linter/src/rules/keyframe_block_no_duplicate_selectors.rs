use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "keyframe-block-no-duplicate-selectors";

declare_rule! {
  /// Disallow duplicate selectors within keyframe blocks.
  ///
  /// Equivalent to Stylelint's `keyframe-block-no-duplicate-selectors` rule.
  ///
  /// Checks `@keyframes` at-rules for child `Style` nodes that share the same
  /// selector (e.g. two `from` or two `50%` blocks).
  pub struct KeyframeBlockNoDuplicateSelectors => KeyframeBlockNoDuplicateSelectorsCheck {
    name: NAME,
    description: "Disallow duplicate selectors within keyframe blocks",
    default_severity: Warning,
  }
}

/// `keyframe-block-no-duplicate-selectors`, which takes no options.
pub struct KeyframeBlockNoDuplicateSelectorsCheck;

impl KeyframeBlockNoDuplicateSelectorsCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for KeyframeBlockNoDuplicateSelectorsCheck {
  /// Flags two keyframe blocks in one `@keyframes` sharing a selector.
  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at_rule) = node else {
      return vec![];
    };

    if at_rule.name != "keyframes" {
      return vec![];
    }

    let mut seen = HashSet::new();
    let mut diagnostics = Vec::new();

    for child in &at_rule.children {
      if let CssNode::Style(style_rule) = child {
        let selector = style_rule.selector.trim().to_ascii_lowercase();
        if !seen.insert(selector.clone()) {
          diagnostics.push(
            Diagnostic::new(
              NAME,
              format!("Unexpected duplicate keyframe selector \"{}\"", selector),
            )
            .span(style_rule.span),
          );
        }
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, StyleRule, Syntax};
  use gale_diagnostics::Span;

  use crate::testing::{configured, ctx, scss_ctx_with_source};

  #[test]
  fn reports_duplicate_keyframe_selectors() {
    let rule = configured(&KeyframeBlockNoDuplicateSelectors, None);
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: Span::new(0, 80),
      children: vec![
        CssNode::Style(StyleRule {
          selector: "from".to_string(),
          declarations: vec![Declaration {
            property: "opacity".to_string(),
            value: "0".to_string(),
            span: Span::new(25, 10),
            important: false,
          }],
          span: Span::new(19, 20),
          ..Default::default()
        }),
        CssNode::Style(StyleRule {
          selector: "to".to_string(),
          declarations: vec![Declaration {
            property: "opacity".to_string(),
            value: "1".to_string(),
            span: Span::new(48, 10),
            important: false,
          }],
          span: Span::new(40, 20),
          ..Default::default()
        }),
        CssNode::Style(StyleRule {
          selector: "from".to_string(),
          declarations: vec![Declaration {
            property: "opacity".to_string(),
            value: "0.5".to_string(),
            span: Span::new(68, 12),
            important: false,
          }],
          span: Span::new(61, 22),
          ..Default::default()
        }),
      ],
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert_eq!(
      diags[0].message,
      "Unexpected duplicate keyframe selector \"from\""
    );
  }

  #[test]
  fn ignores_unique_keyframe_selectors() {
    let rule = configured(&KeyframeBlockNoDuplicateSelectors, None);
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: Span::new(0, 60),
      children: vec![
        CssNode::Style(StyleRule {
          selector: "from".to_string(),
          declarations: vec![],
          span: Span::new(19, 15),
          ..Default::default()
        }),
        CssNode::Style(StyleRule {
          selector: "to".to_string(),
          declarations: vec![],
          span: Span::new(35, 13),
          ..Default::default()
        }),
      ],
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_non_keyframes_at_rules() {
    let rule = configured(&KeyframeBlockNoDuplicateSelectors, None);
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: Span::new(0, 40),
      children: vec![],
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn detects_duplicates_in_parsed_css_keyframes() {
    let rule = configured(&KeyframeBlockNoDuplicateSelectors, None);
    let css = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } from { opacity: 0.5; } }";
    let result = gale_css_parser::parse(css, Syntax::Css).expect("should parse CSS");
    let ctx = ctx();
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect duplicate 'from' in CSS keyframes"
    );
  }

  #[test]
  fn detects_duplicates_in_parsed_scss_keyframes() {
    let rule = configured(&KeyframeBlockNoDuplicateSelectors, None);
    let scss = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } from { opacity: 0.5; } }";
    let result = gale_css_parser::parse(scss, Syntax::Scss).expect("should parse SCSS");
    let ctx = scss_ctx_with_source(scss);
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect duplicate 'from' in SCSS keyframes"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        "keyframe-block-no-duplicate-selectors",
        serde_json::json!(true)
      )
      .is_empty()
    );
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "keyframe-block-no-duplicate-selectors",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        "keyframe-block-no-duplicate-selectors",
        serde_json::json!("always")
      ),
      ["Unexpected option value \"always\" for rule \"keyframe-block-no-duplicate-selectors\""]
    );
  }
}
