use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/declaration-block-semicolon-newline-before";

declare_rule! {
  /// Require a newline or disallow whitespace before the semicolons of declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-semicolon-newline-before`.
  pub struct StylisticDeclarationBlockSemicolonNewlineBefore => StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
    name: NAME,
    description: "Require a newline or disallow whitespace before the semicolons of declaration blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/declaration-block-semicolon-newline-before` as configured.
pub struct StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
  /// The primary option: `"always"`, `"always-multi-line"`,
  /// `"never-multi-line"` or gale's own `"never"`.
  option: &'static str,
}

impl StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
  /// The plugin's options: `"always"`, `"always-multi-line"` or
  /// `"never-multi-line"`; gale applies only `"always"` so far.  Gale also
  /// accepts `"never"`, which the plugin does not: it disallows whitespace before
  /// the semicolon, and is what gale reads no option as.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "always-multi-line", "never-multi-line", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticDeclarationBlockSemicolonNewlineBeforeCheck {
  /// Flags a declaration semicolon whose preceding newline does not match the option.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let option = self.option;

    let mut diags = Vec::new();
    let source = ctx.source;

    for decl in &rule.declarations {
      let decl_end = decl.span.offset + decl.span.length;
      // Look for the semicolon after the declaration
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
      let has_newline_before = char_before == b'\n' || (char_before == b'\r');

      let violation = match option {
        "always" => !has_newline_before,
        "never" => has_newline_before,
        _ => false,
      };

      if violation {
        let msg = match option {
          "always" => "Expected newline before \";\"",
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_no_newline_before_semicolon() {
    let rule = configured(&StylisticDeclarationBlockSemicolonNewlineBefore, None);
    let source = "a { color: red; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(4, 10),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_newline_before_semicolon_when_never() {
    let rule = configured(
      &StylisticDeclarationBlockSemicolonNewlineBefore,
      serde_json::json!("never"),
    );
    let source = "a { color: red\n; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(4, 10),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always-multi-line")).is_empty());
    // `"never"` is gale's own value; the plugin rejects it.
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/declaration-block-semicolon-newline-before\""
      ]
    );
  }
}
