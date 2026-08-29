use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of declarations within a single-line declaration block.
///
/// Equivalent to Stylelint's `declaration-block-single-line-max-declarations` rule.
/// Default maximum: 1. Detection-only.
pub struct DeclarationBlockSingleLineMaxDeclarations;

const MAX_DECLARATIONS: usize = 1;

impl Rule for DeclarationBlockSingleLineMaxDeclarations {
    fn name(&self) -> &'static str {
        "declaration-block-single-line-max-declarations"
    }

    fn description(&self) -> &'static str {
        "Limit the number of declarations within a single-line declaration block"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Style(rule) = node else {
            return vec![];
        };

        // Skip if span is unavailable (zero-length means parser didn't set it)
        if rule.span.length == 0 {
            return vec![];
        }

        // Read configured max from options (primary option is a number).
        let max = ctx
            .options
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(MAX_DECLARATIONS);

        // Determine if this rule block is single-line by checking the source span.
        let is_single_line = is_single_line_block(rule.span.offset, rule.span.length, ctx.source);

        if !is_single_line {
            return vec![];
        }

        let count = rule.declarations.len();
        if count > max {
            vec![Diagnostic::new(
                self.name(),
                format!(
                    "Expected no more than {max} declaration(s) in a single-line block, found {count}"
                ),
            )
            .severity(self.default_severity())
            .span(Span::new(rule.span.offset, rule.span.length))]
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
    use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

    fn ctx_with_source(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

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
        let source = "a { color: red; font-size: 16px; }";
        let node = style_with_decls(
            vec![("color", "red"), ("font-size", "16px")],
            0,
            source.len(),
        );
        let d = DeclarationBlockSingleLineMaxDeclarations.check(&node, &ctx_with_source(source));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("found 2"));
    }

    #[test]
    fn allows_single_decl_single_line() {
        let source = "a { color: red; }";
        let node = style_with_decls(vec![("color", "red")], 0, source.len());
        let d = DeclarationBlockSingleLineMaxDeclarations.check(&node, &ctx_with_source(source));
        assert!(d.is_empty());
    }

    #[test]
    fn allows_multiple_decls_multi_line() {
        let source = "a {\n  color: red;\n  font-size: 16px;\n}";
        let node = style_with_decls(
            vec![("color", "red"), ("font-size", "16px")],
            0,
            source.len(),
        );
        let d = DeclarationBlockSingleLineMaxDeclarations.check(&node, &ctx_with_source(source));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_zero_length_span() {
        // Simulates a parser-generated style node with no real span (e.g. from @tailwind directives)
        let source = "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n";
        let node = style_with_decls(
            vec![("color", "red"), ("font-size", "16px")],
            0,
            0, // zero-length span — parser didn't set it
        );
        let d = DeclarationBlockSingleLineMaxDeclarations.check(&node, &ctx_with_source(source));
        assert!(d.is_empty(), "should not report on zero-length span");
    }
}
