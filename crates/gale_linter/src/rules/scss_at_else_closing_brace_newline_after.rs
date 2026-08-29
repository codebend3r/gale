use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline after the closing brace of `@else` statements.
///
/// Default: `"always-last-in-chain"` — require a newline after the closing
/// brace of the last `@else` in a chain.
///
/// Equivalent to `scss/at-else-closing-brace-newline-after`.
pub struct ScssAtElseClosingBraceNewlineAfter;

impl Rule for ScssAtElseClosingBraceNewlineAfter {
    fn name(&self) -> &'static str {
        "scss/at-else-closing-brace-newline-after"
    }

    fn description(&self) -> &'static str {
        "Require or disallow a newline after the closing brace of @else statements"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
        if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
            return vec![];
        }

        let option = ctx.primary_option_str().unwrap_or("always-last-in-chain");
        let source = ctx.source;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut diagnostics = Vec::new();

        let mut i = 0;
        while i < len {
            // Look for `@else` (but not `@else if`)
            if bytes[i] == b'@' && source[i..].starts_with("@else") {
                let after_else = i + 5;
                // Skip whitespace
                let mut j = after_else;
                while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }

                // If followed by `if`, this is `@else if` — skip
                let is_else_if = j < len && source[j..].starts_with("if ");

                if !is_else_if {
                    // This is a plain `@else` — find closing brace
                    if let Some(close_brace) = find_closing_brace(source, i) {
                        let after = close_brace + 1;
                        let mut k = after;
                        while k < len && (bytes[k] == b' ' || bytes[k] == b'\t') {
                            k += 1;
                        }

                        if option == "always-last-in-chain"
                            && k < len
                            && bytes[k] != b'\n'
                            && bytes[k] != b'\r'
                        {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Expected newline after closing brace of @else".to_string(),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(close_brace, 1)),
                            );
                        }

                        i = after;
                        continue;
                    }
                }
            }
            i += 1;
        }

        diagnostics
    }
}

fn find_closing_brace(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = start;

    while i < len && bytes[i] != b'{' {
        i += 1;
    }
    if i >= len {
        return None;
    }

    let mut depth = 1;
    i += 1;
    while i < len && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            i += 1;
        }
    }

    if depth == 0 { Some(i) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn scss_ctx_with_source(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.scss",
            source,
            syntax: Syntax::Scss,
            options: None,
        }
    }

    #[test]
    fn allows_newline_after_else() {
        let ctx = scss_ctx_with_source("@if $c { a: b; } @else { c: d; }\n.foo {}");
        let d = ScssAtElseClosingBraceNewlineAfter.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_eof_after_else() {
        let ctx = scss_ctx_with_source("@if $c { a: b; } @else { c: d; }");
        let d = ScssAtElseClosingBraceNewlineAfter.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_non_scss() {
        let ctx = RuleContext {
            file_path: "t.css",
            source: "@else { } .foo {}",
            syntax: Syntax::Css,
            options: None,
        };
        assert!(
            ScssAtElseClosingBraceNewlineAfter
                .check_root(&[], &ctx)
                .is_empty()
        );
    }
}
