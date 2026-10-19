use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::js_number::to_js_string;
use crate::lint_env::LintEnv;
use crate::options::{Accept, InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-block-single-line-max-declarations";

declare_rule! {
  /// Limit the number of declarations within a single-line declaration block.
  ///
  /// Equivalent to Stylelint's `declaration-block-single-line-max-declarations` rule.
  /// Default maximum: 1. Detection-only.
  pub struct DeclarationBlockSingleLineMaxDeclarations => DeclarationBlockSingleLineMaxDeclarationsCheck {
    name: NAME,
    description: "Limit the number of declarations within a single-line declaration block",
    default_severity: Warning,
  }
}

/// `declaration-block-single-line-max-declarations` as configured.
pub struct DeclarationBlockSingleLineMaxDeclarationsCheck {
  /// The most declarations a single-line block may hold.
  max: f64,
}

impl DeclarationBlockSingleLineMaxDeclarationsCheck {
  /// Stylelint's option: a number.  Gale reads no option as 1.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_number(&[Accept::Number]);
    options.finish()?;
    Ok(Self {
      max: max.unwrap_or(MAX_DECLARATIONS),
    })
  }
}

/// The most declarations a single-line block may hold when the config
/// gives no number.
const MAX_DECLARATIONS: f64 = 1.0;

impl Check for DeclarationBlockSingleLineMaxDeclarationsCheck {
  /// Flags single-line blocks holding more than the configured number of
  /// declarations. Multi-line blocks are skipped.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Skip if span is unavailable (zero-length means parser didn't set it)
    if rule.span.length == 0 {
      return vec![];
    }

    let max = self.max;

    // Determine if this rule block is single-line by checking the source span.
    let is_single_line = is_single_line_block(rule.span.offset, rule.span.length, ctx.source);

    if !is_single_line {
      return vec![];
    }

    let count = rule.declarations.len();
    if count as f64 > max {
      vec![
        Diagnostic::new(
          NAME,
          format!(
            "Expected no more than {} declaration(s) in a single-line block, found {count}",
            to_js_string(max)
          ),
        )
        .span(Span::new(rule.span.offset, rule.span.length)),
      ]
    } else {
      vec![]
    }
  }
}

/// Check if the source text for the given span is all on one line.
fn is_single_line_block(offset: usize, length: usize, source: &str) -> bool {
  let end = offset + length;
  if end > source.len() {
    // If we can't check the source, fall back to not reporting.
    return false;
  }
  let block = &source[offset..end];
  !block.contains('\n')
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx_with_source, invalid_options};

  fn style_with_decls(decls: Vec<(&str, &str)>, span_offset: usize, span_len: usize) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: decls
        .into_iter()
        .map(|(p, v)| Declaration {
          property: p.to_string(),
          value: v.to_string(),
          span: ParserSpan::new(0, 0),
          important: false,
        })
        .collect(),
      span: ParserSpan::new(span_offset, span_len),
      ..Default::default()
    })
  }

  #[test]
  fn reports_multiple_decls_single_line() {
    let rule = configured(&DeclarationBlockSingleLineMaxDeclarations, None);
    let source = "a { color: red; font-size: 16px; }";
    let node = style_with_decls(
      vec![("color", "red"), ("font-size", "16px")],
      0,
      source.len(),
    );
    let d = rule.check(&node, &ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("found 2"));
  }

  #[test]
  fn allows_single_decl_single_line() {
    let rule = configured(&DeclarationBlockSingleLineMaxDeclarations, None);
    let source = "a { color: red; }";
    let node = style_with_decls(vec![("color", "red")], 0, source.len());
    let d = rule.check(&node, &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_multiple_decls_multi_line() {
    let rule = configured(&DeclarationBlockSingleLineMaxDeclarations, None);
    let source = "a {\n  color: red;\n  font-size: 16px;\n}";
    let node = style_with_decls(
      vec![("color", "red"), ("font-size", "16px")],
      0,
      source.len(),
    );
    let d = rule.check(&node, &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_zero_length_span() {
    let rule = configured(&DeclarationBlockSingleLineMaxDeclarations, None);
    // Simulates a parser-generated style node with no real span (e.g. from @tailwind directives)
    let source = "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n";
    let node = style_with_decls(
      vec![("color", "red"), ("font-size", "16px")],
      0,
      0, // zero-length span — parser didn't set it
    );
    let d = rule.check(&node, &ctx_with_source(source));
    assert!(d.is_empty(), "should not report on zero-length span");
  }

  #[test]
  fn reads_the_configured_maximum() {
    let source = "a { color: red; top: 0; }";
    let node = style_with_decls(vec![("color", "red"), ("top", "0")], 0, source.len());
    let ctx = ctx_with_source(source);
    let two = configured(
      &DeclarationBlockSingleLineMaxDeclarations,
      serde_json::json!(2),
    );
    assert!(two.check(&node, &ctx).is_empty());
    // The array form Stylelint configs use for secondary options.
    let one = configured(
      &DeclarationBlockSingleLineMaxDeclarations,
      serde_json::json!([1, { "severity": "warning" }]),
    );
    assert_eq!(one.check(&node, &ctx).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(3)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("3")),
      ["Invalid option value \"3\" for rule \"declaration-block-single-line-max-declarations\""]
    );
  }
}
