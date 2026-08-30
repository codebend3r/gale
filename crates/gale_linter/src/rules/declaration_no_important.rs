use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports use of `!important` in declarations.
///
/// Equivalent to Stylelint's `declaration-no-important` rule.
pub struct DeclarationNoImportant;

impl Rule for DeclarationNoImportant {
    fn name(&self) -> &'static str {
        "declaration-no-important"
    }

    fn description(&self) -> &'static str {
        "Disallow !important within declarations"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Style(rule) = node else {
            return vec![];
        };
        let mut diags = Vec::new();
        for decl in &rule.declarations {
            if decl.important {
                let decl_start = decl.span.offset;
                let decl_end = decl_start + decl.span.length;

                // Try to find `!important` in the source span to build a precise fix.
                let mut fix_opt = None;
                if decl_end <= ctx.source.len() && decl_start < decl_end {
                    let slice = &ctx.source[decl_start..decl_end];
                    if let Some(pos) = find_important(slice) {
                        let abs_pos = decl_start + pos;
                        let imp_text = &slice[pos..];
                        // The `!important` token may have whitespace before the `!`.
                        // Find the exact length: `!important` is 10 chars, but we
                        // also want to remove any preceding whitespace.
                        let imp_len = imp_text
                            .find(';')
                            .unwrap_or(imp_text.len())
                            .min("!important".len());
                        // Also remove leading whitespace before `!`.
                        let leading_ws = slice[..pos]
                            .bytes()
                            .rev()
                            .take_while(|b| *b == b' ' || *b == b'\t')
                            .count();
                        let fix_start = abs_pos - leading_ws;
                        let fix_len = imp_len + leading_ws;
                        fix_opt = Some(Fix::new(
                            "Remove !important",
                            vec![Edit::new(Span::new(fix_start, fix_len), "")],
                        ));
                    }
                }

                // Point the diagnostic span at the `!important` text when
                // possible so that `disable-line` comments on the same line
                // as `!important` (which may differ from the declaration
                // start line for multi-line declarations) correctly suppress
                // the diagnostic.
                let diag_span = if let Some(pos) = fix_opt.as_ref().and_then(|_| {
                    if decl_end <= ctx.source.len() && decl_start < decl_end {
                        let slice = &ctx.source[decl_start..decl_end];
                        find_important(slice).map(|p| decl_start + p)
                    } else {
                        None
                    }
                }) {
                    Span::new(pos, "!important".len())
                } else {
                    Span::new(decl.span.offset, decl.span.length)
                };

                let mut diag = Diagnostic::new(
                    self.name(),
                    format!("Unexpected !important in declaration \"{}\"", decl.property),
                )
                .severity(self.default_severity())
                .span(diag_span);

                if let Some(fix) = fix_opt {
                    diag = diag.fix(fix);
                }
                diags.push(diag);
            }
        }
        diags
    }
}

/// Find the byte offset of `!important` (case-insensitive) in a string slice.
fn find_important(s: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    lower.find("!important")
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

    fn style_node(prop: &str, val: &str, important: bool, offset: usize, len: usize) -> CssNode {
        CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![Declaration {
                property: prop.to_string(),
                value: val.to_string(),
                span: ParserSpan::new(offset, len),
                important,
            }],
            span: ParserSpan::new(0, 0),
            ..Default::default()
        })
    }

    #[test]
    fn reports_important() {
        let src = "a { color: red !important; }";
        let node = style_node("color", "red", true, 4, 22);
        let d = DeclarationNoImportant.check(&node, &ctx_with_source(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!important"));
        assert!(d[0].fix.is_some());
    }

    #[test]
    fn allows_no_important() {
        let src = "a { color: red; }";
        let node = style_node("color", "red", false, 4, 11);
        let d = DeclarationNoImportant.check(&node, &ctx_with_source(src));
        assert!(d.is_empty());
    }
}
