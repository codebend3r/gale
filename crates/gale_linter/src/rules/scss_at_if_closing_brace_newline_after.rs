use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline or disallow whitespace after the closing brace of
/// `@if` statements that are NOT followed by `@else`.
///
/// Default: `"always-last-in-chain"` — require a newline after the closing
/// brace of the last `@if`/`@else if` in a chain (i.e. when not followed
/// by `@else`).
///
/// Equivalent to `scss/at-if-closing-brace-newline-after`.
pub struct ScssAtIfClosingBraceNewlineAfter;

impl Rule for ScssAtIfClosingBraceNewlineAfter {
    fn name(&self) -> &'static str {
        "scss/at-if-closing-brace-newline-after"
    }

    fn description(&self) -> &'static str {
        "Require or disallow a newline after the closing brace of @if statements"
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

        // Find `@if` occurrences and check what follows their closing `}`
        let mut i = 0;
        while i < len {
            // Look for `@if` or `@else if`
            if bytes[i] == b'@' && i + 2 < len {
                let rest = &source[i..];
                let is_if = rest.starts_with("@if ") || rest.starts_with("@if(");
                let is_else_if = rest.starts_with("@else if ") || rest.starts_with("@else if(");

                if is_if || is_else_if {
                    // Find the matching closing `}`
                    if let Some(close_brace) = find_closing_brace(source, i) {
                        let after = close_brace + 1;
                        // Skip whitespace/spaces (not newlines) after `}`
                        let mut j = after;
                        while j < len && (bytes[j] == b' ' || bytes[j] == b'\t') {
                            j += 1;
                        }

                        // Check what follows
                        let followed_by_else = j < len && source[j..].starts_with("@else");

                        if option == "always-last-in-chain" && !followed_by_else {
                            // Must be followed by newline
                            if j < len && bytes[j] != b'\n' && bytes[j] != b'\r' {
                                diagnostics.push(
                                    Diagnostic::new(
                                        self.name(),
                                        "Expected newline after closing brace of @if".to_string(),
                                    )
                                    .severity(self.default_severity())
                                    .span(Span::new(close_brace, 1)),
                                );
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

/// Find the closing `}` that matches the first `{` found after `start`.
/// Handles SCSS interpolation `#{...}`, strings, and comments.
fn find_closing_brace(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = start;

    // Find opening brace (skip `#{` interpolation openers)
    loop {
        if i >= len {
            return None;
        }
        if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
            // Skip interpolation `#{...}`
            i += 2;
            skip_interpolation(bytes, len, &mut i);
            continue;
        }
        if bytes[i] == b'{' {
            break;
        }
        i += 1;
    }

    let mut depth = 1;
    i += 1;
    while i < len && depth > 0 {
        if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
            // Skip interpolation `#{...}` entirely
            i += 2;
            skip_interpolation(bytes, len, &mut i);
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < len && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < len => {
                if bytes[i + 1] == b'/' {
                    i += 2;
                    while i < len && bytes[i] != b'\n' {
                        i += 1;
                    }
                } else if bytes[i + 1] == b'*' {
                    i += 2;
                    while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    if i + 1 < len {
                        i += 1;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    None
}

/// Skip past the contents of a `#{...}` interpolation block.
/// `i` should point to the first byte after `#{`. On return, `i` points
/// to the byte after the closing `}`.
fn skip_interpolation(bytes: &[u8], len: usize, i: &mut usize) {
    let mut depth = 1;
    while *i < len && depth > 0 {
        match bytes[*i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        *i += 1;
    }
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
    fn allows_newline_after_if() {
        let ctx = scss_ctx_with_source("@if $cond { color: red; }\n.foo {}");
        let d = ScssAtIfClosingBraceNewlineAfter.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_else_on_same_line() {
        let ctx = scss_ctx_with_source("@if $cond { color: red; } @else { color: blue; }\n");
        let d = ScssAtIfClosingBraceNewlineAfter.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_interpolation_inside_if() {
        // @if $breakpoint != "base" {
        //   $breakpoint-name: -on-#{$breakpoint};
        // }
        let src = "@if $breakpoint != \"base\" {\n  $breakpoint-name: -on-#{$breakpoint};\n}\n";
        let ctx = scss_ctx_with_source(src);
        let d = ScssAtIfClosingBraceNewlineAfter.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_if_else_chain_with_interpolation() {
        let src = "@if $x { $name: #{$x}-foo; } @else if $y { $name: #{$y}-bar; } @else { $name: baz; }\n";
        let ctx = scss_ctx_with_source(src);
        let d = ScssAtIfClosingBraceNewlineAfter.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_non_scss() {
        let ctx = RuleContext {
            file_path: "t.css",
            source: "@if { } .foo {}",
            syntax: Syntax::Css,
            options: None,
        };
        assert!(
            ScssAtIfClosingBraceNewlineAfter
                .check_root(&[], &ctx)
                .is_empty()
        );
    }
}
