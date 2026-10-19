use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "keyframe-declaration-no-important";

declare_rule! {
  /// Reports `!important` within `@keyframes` declarations.
  ///
  /// Using `!important` inside keyframe declarations is ignored by browsers
  /// and is almost certainly a mistake.
  ///
  /// Equivalent to Stylelint's `keyframe-declaration-no-important` rule.
  pub struct KeyframeDeclarationNoImportant => KeyframeDeclarationNoImportantCheck {
    name: NAME,
    description: "Disallow !important within keyframe declarations",
    default_severity: Warning,
  }
}

/// `keyframe-declaration-no-important`, which takes no options.
pub struct KeyframeDeclarationNoImportantCheck;

impl KeyframeDeclarationNoImportantCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for KeyframeDeclarationNoImportantCheck {
  /// Checks the children of a `@keyframes` at-rule.
  fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let CssNode::AtRule(at_rule) = node
      && at_rule.name == "keyframes"
    {
      collect_important_in_children(&at_rule.children, &mut diagnostics);
    }

    diagnostics
  }
}

/// Recurses through keyframe blocks, flagging every `!important` declaration —
/// browsers ignore them there.
fn collect_important_in_children(children: &[CssNode], diagnostics: &mut Vec<Diagnostic>) {
  for child in children {
    match child {
      CssNode::Declaration(decl) if decl.important => {
        diagnostics.push(
          Diagnostic::new(
            NAME,
            format!(
              "Unexpected !important in keyframe declaration \"{}\"",
              decl.property
            ),
          )
          .span(Span::new(decl.span.offset, decl.span.length)),
        );
      }
      CssNode::Style(style_rule) => {
        for decl in &style_rule.declarations {
          if decl.important {
            diagnostics.push(
              Diagnostic::new(
                NAME,
                format!(
                  "Unexpected !important in keyframe declaration \"{}\"",
                  decl.property
                ),
              )
              .span(Span::new(decl.span.offset, decl.span.length)),
            );
          }
        }
      }
      _ => {}
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Declaration, Span as ParserSpan, Syntax};

  use crate::testing::{configured, ctx, scss_ctx_with_source};

  #[test]
  fn reports_important_in_keyframe() {
    let rule = configured(&KeyframeDeclarationNoImportant, None);
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, 50),
      children: vec![CssNode::Declaration(Declaration {
        property: "opacity".to_string(),
        value: "0".to_string(),
        span: ParserSpan::new(20, 15),
        important: true,
      })],
    });
    let diags = rule.check(&node, &ctx());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("!important"));
  }

  #[test]
  fn ignores_keyframe_without_important() {
    let rule = configured(&KeyframeDeclarationNoImportant, None);
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, 50),
      children: vec![CssNode::Declaration(Declaration {
        property: "opacity".to_string(),
        value: "0".to_string(),
        span: ParserSpan::new(20, 15),
        important: false,
      })],
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_non_keyframe_at_rule() {
    let rule = configured(&KeyframeDeclarationNoImportant, None);
    let node = CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: "(min-width: 768px)".to_string(),
      span: ParserSpan::new(0, 30),
      children: vec![],
    });
    let diags = rule.check(&node, &ctx());
    assert!(diags.is_empty());
  }

  #[test]
  fn detects_important_in_parsed_css_keyframes() {
    let rule = configured(&KeyframeDeclarationNoImportant, None);
    let css = "@keyframes fade { from { opacity: 0 !important; } }";
    let result = gale_css_parser::parse(css, Syntax::Css).expect("should parse CSS");
    let ctx = ctx();
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect !important in CSS keyframe"
    );
  }

  #[test]
  fn detects_important_in_parsed_scss_keyframes() {
    let rule = configured(&KeyframeDeclarationNoImportant, None);
    let scss = "@keyframes fade { from { opacity: 0 !important; } }";
    let result = gale_css_parser::parse(scss, Syntax::Scss).expect("should parse SCSS");
    let ctx = scss_ctx_with_source(scss);
    let mut all_diags = Vec::new();
    for node in &result.nodes {
      all_diags.extend(rule.check(node, &ctx));
    }
    assert_eq!(
      all_diags.len(),
      1,
      "should detect !important in SCSS keyframe"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options("keyframe-declaration-no-important", serde_json::json!(true)).is_empty()
    );
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "keyframe-declaration-no-important",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        "keyframe-declaration-no-important",
        serde_json::json!("always")
      ),
      ["Unexpected option value \"always\" for rule \"keyframe-declaration-no-important\""]
    );
  }
}
