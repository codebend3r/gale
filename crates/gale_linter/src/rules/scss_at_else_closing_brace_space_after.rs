use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the closing brace of `@else` when
/// followed by `@else if`.
///
/// Default: `"always-intermediate"`.
///
/// Equivalent to `scss/at-else-closing-brace-space-after`.
pub struct ScssAtElseClosingBraceSpaceAfter;

impl Rule for ScssAtElseClosingBraceSpaceAfter {
    fn name(&self) -> &'static str {
        "scss/at-else-closing-brace-space-after"
    }

    fn description(&self) -> &'static str {
        "Require or disallow a space after the closing brace of @else before @else"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
        if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
            return vec![];
        }

        let option = ctx.primary_option_str().unwrap_or("always-intermediate");
        let source = ctx.source;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut diagnostics = Vec::new();

        let mut i = 0;
        while i < len {
            if bytes[i] == b'@' && source[i..].starts_with("@else") {
                let after_else = i + 5;
                let mut j = after_else;
                while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }

                // Check if this is `@else if` — it has its own closing brace
                let is_else_if = j < len && source[j..].starts_with("if ");

                if is_else_if {
                    // Find closing brace of this `@else if`
                    if let Some(close_brace) = find_closing_brace(source, i) {
                        let after = close_brace + 1;
                        let mut k = after;
                        let mut space_count = 0;
                        while k < len && bytes[k] == b' ' {
                            space_count += 1;
                            k += 1;
                        }

                        let followed_by_else = k < len && source[k..].starts_with("@else");

                        if followed_by_else {
                            match option {
                                "always-intermediate" => {
                                    if space_count != 1 {
                                        diagnostics.push(
                                            Diagnostic::new(
                                                self.name(),
                                                "Expected single space after closing brace of @else if before @else".to_string(),
                                            )
                                            .severity(self.default_severity())
                                            .span(Span::new(close_brace, 1)),
                                        );
                                    }
                                }
                                "never-intermediate" => {
                                    if space_count > 0 {
                                        diagnostics.push(
                                            Diagnostic::new(
                                                self.name(),
                                                "Unexpected space after closing brace of @else if before @else".to_string(),
                                            )
                                            .severity(self.default_severity())
                                            .span(Span::new(close_brace, 1)),
                                        );
                                    }
                                }
                                _ => {}
                            }
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
    fn allows_space_between_else_if_and_else() {
        let ctx = scss_ctx_with_source("@if $a { a: b; } @else if $b { c: d; } @else { e: f; }");
        let d = ScssAtElseClosingBraceSpaceAfter.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn reports_no_space_between_else_if_and_else() {
        let ctx = scss_ctx_with_source("@if $a { a: b; } @else if $b { c: d; }@else { e: f; }");
        let d = ScssAtElseClosingBraceSpaceAfter.check_root(&[], &ctx);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_non_scss() {
        let ctx = RuleContext {
            file_path: "t.css",
            source: "@else if { }@else { }",
            syntax: Syntax::Css,
            options: None,
        };
        assert!(
            ScssAtElseClosingBraceSpaceAfter
                .check_root(&[], &ctx)
                .is_empty()
        );
    }
}
