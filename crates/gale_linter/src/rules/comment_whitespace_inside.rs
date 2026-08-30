use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require whitespace on the inside of comment markers (`/* */`).
///
/// Equivalent to Stylelint's `comment-whitespace-inside` rule with "always" option.
/// Detection-only (no autofix).
pub struct CommentWhitespaceInside;

impl Rule for CommentWhitespaceInside {
    fn name(&self) -> &'static str {
        "comment-whitespace-inside"
    }

    fn description(&self) -> &'static str {
        "Require whitespace on the inside of comment markers"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Comment(comment) = node else {
            return vec![];
        };

        // Skip line comments (SCSS/Less `//` style)
        if comment.is_line {
            return vec![];
        }

        let offset = comment.span.offset;
        let length = comment.span.length;

        // Extract the raw comment text from the source to get the full `/* ... */`
        if offset + length > ctx.source.len() {
            return vec![];
        }
        let raw = &ctx.source[offset..offset + length];

        // Must be a block comment wrapped in /* */
        if !raw.starts_with("/*") || !raw.ends_with("*/") {
            return vec![];
        }

        // Skip comments embedded inside selector lists (preceding text ends with `,`).
        // postcss doesn't expose these as comment nodes so Stylelint never checks them.
        if offset > 0 {
            let before = ctx.source[..offset].trim_end();
            if before.ends_with(',') {
                return vec![];
            }
        }

        // Extract the inner content between /* and */, skipping any extra
        // asterisks or `!` that form part of the opener/closer (e.g. /** ... **/ or /*! ... */).
        // Stylelint treats `/**` and `/*!` the same as `/*` — the extra chars
        // are part of the comment marker, not the content.
        let inner_full = &raw[2..raw.len() - 2];
        let leading_special = inner_full
            .chars()
            .take_while(|c| *c == '*' || *c == '!')
            .count();
        let leading_stars = leading_special;
        let trailing_stars = inner_full.chars().rev().take_while(|c| *c == '*').count();
        let inner_start = leading_stars;
        let inner_end = inner_full.len().saturating_sub(trailing_stars);
        let inner = if inner_start >= inner_end {
            ""
        } else {
            &inner_full[inner_start..inner_end]
        };

        // Skip empty comments /**/ or /****/
        if inner.is_empty() {
            return vec![];
        }

        // Skip Stylelint/Gale command comments (disable/enable directives).
        let inner_trimmed = inner.trim();
        if inner_trimmed.starts_with("stylelint-") || inner_trimmed.starts_with("gale-") {
            return vec![];
        }

        let mut diags = Vec::new();

        // Check for whitespace after the opener (e.g. after /** or /*)
        let missing_start = !inner.starts_with(|c: char| c.is_whitespace());
        // Check for whitespace before the closer (e.g. before */ or **/)
        let missing_end = !inner.ends_with(|c: char| c.is_whitespace());

        if missing_start {
            diags.push(
                Diagnostic::new(self.name(), "Expected whitespace after \"/*\"".to_string())
                    .severity(self.default_severity())
                    .span(Span::new(offset, length)),
            );
        }

        if missing_end {
            diags.push(
                Diagnostic::new(self.name(), "Expected whitespace before \"*/\"".to_string())
                    .severity(self.default_severity())
                    .span(Span::new(offset, length)),
            );
        }

        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{Comment, Span as ParserSpan, Syntax};

    fn make_ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

    #[test]
    fn allows_comment_with_whitespace_inside() {
        let src = "/* comment */";
        let node = CssNode::Comment(Comment {
            is_line: false,
            text: " comment ".to_string(),
            span: ParserSpan::new(0, src.len()),
        });
        let d = CommentWhitespaceInside.check(&node, &make_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn reports_missing_whitespace_after_open() {
        let src = "/*comment */";
        let node = CssNode::Comment(Comment {
            is_line: false,
            text: "comment ".to_string(),
            span: ParserSpan::new(0, src.len()),
        });
        let d = CommentWhitespaceInside.check(&node, &make_ctx(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("after"));
    }

    #[test]
    fn reports_missing_whitespace_before_close() {
        let src = "/* comment*/";
        let node = CssNode::Comment(Comment {
            is_line: false,
            text: " comment".to_string(),
            span: ParserSpan::new(0, src.len()),
        });
        let d = CommentWhitespaceInside.check(&node, &make_ctx(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn reports_both_missing_whitespace() {
        let src = "/*comment*/";
        let node = CssNode::Comment(Comment {
            is_line: false,
            text: "comment".to_string(),
            span: ParserSpan::new(0, src.len()),
        });
        let d = CommentWhitespaceInside.check(&node, &make_ctx(src));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn skips_empty_comment() {
        let src = "/**/";
        let node = CssNode::Comment(Comment {
            is_line: false,
            text: "".to_string(),
            span: ParserSpan::new(0, src.len()),
        });
        let d = CommentWhitespaceInside.check(&node, &make_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_line_comments() {
        let src = "// comment";
        let node = CssNode::Comment(Comment {
            is_line: true,
            text: " comment".to_string(),
            span: ParserSpan::new(0, src.len()),
        });
        let ctx = RuleContext {
            file_path: "t.scss",
            source: src,
            syntax: Syntax::Scss,
            options: None,
        };
        let d = CommentWhitespaceInside.check(&node, &ctx);
        assert!(d.is_empty());
    }
}
