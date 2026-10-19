use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/declaration-block-semicolon-space-before";

declare_rule! {
  /// Require or disallow a space before the semicolons of declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-space-before`.
  pub struct StylisticDeclarationBlockSemicolonSpaceBefore => StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/declaration-block-semicolon-space-before` as configured.
pub struct StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.  Gale reads no option as `"never"`.
  /// `"always-single-line"` and `"never-single-line"` are accepted but not yet
  /// applied.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticDeclarationBlockSemicolonSpaceBeforeCheck {
  /// Flags a declaration semicolon whose preceding space does not match the option.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let option = self.option;

    let mut diags = Vec::new();
    let source = ctx.source;

    for decl in &rule.declarations {
      let decl_end = decl.span.offset + decl.span.length;
      if decl_end >= source.len() {
        continue;
      }

      // Find the semicolon following this declaration
      let remaining = &source[decl_end..];
      let semi_rel = match remaining.find(';') {
        Some(pos) => pos,
        None => continue,
      };
      let semi_offset = decl_end + semi_rel;

      if semi_offset == 0 {
        continue;
      }

      let char_before = source.as_bytes()[semi_offset - 1];
      let has_space = char_before == b' ' || char_before == b'\t';

      let violation = match option {
        "always" => !has_space,
        "never" => has_space,
        _ => false,
      };

      if violation {
        let msg = match option {
          "always" => "Expected single space before \";\"",
          "never" => "Unexpected whitespace before \";\"",
          _ => continue,
        };
        diags.push(Diagnostic::new(NAME, msg).span(Span::new(semi_offset, 1)));
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule};

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_no_space_before_semicolon() {
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceBefore, None);
    let source = "a { color: red; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(4, 10),
        important: false,
      }],
      span: Span::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_space_before_semicolon_when_never() {
    let rule = configured(&StylisticDeclarationBlockSemicolonSpaceBefore, None);
    let source = "a { color: red ; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(4, 10),
        important: false,
      }],
      span: Span::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always-multi-line")),
      [
        "Invalid option value \"always-multi-line\" for rule \"@stylistic/declaration-block-semicolon-space-before\""
      ]
    );
  }
}
