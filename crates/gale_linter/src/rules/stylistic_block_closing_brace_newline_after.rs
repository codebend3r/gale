use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a newline after the closing brace of blocks.
///
/// Equivalent to Stylelint's `@stylistic/block-closing-brace-newline-after` rule.
pub struct StylisticBlockClosingBraceNewlineAfter;

impl Rule for StylisticBlockClosingBraceNewlineAfter {
    fn name(&self) -> &'static str {
        "@stylistic/block-closing-brace-newline-after"
    }

    fn description(&self) -> &'static str {
        "Require or disallow a newline after the closing brace of blocks"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
        let option = context.primary_option_str().unwrap_or("always");
        let source = context.source;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut diagnostics = Vec::new();
        let mut i = 0;

        while i < len {
            // Skip strings
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                i += 1;
                while i < len && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }

            // Skip comments
            if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue;
            }

            // Skip SCSS line comments
            if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            // Skip SCSS interpolation #{...}
            if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
                i += 2;
                let mut interp_depth = 1;
                while i < len && interp_depth > 0 {
                    if bytes[i] == b'{' {
                        interp_depth += 1;
                    } else if bytes[i] == b'}' {
                        interp_depth -= 1;
                    }
                    if interp_depth > 0 {
                        i += 1;
                    }
                }
                if i < len {
                    i += 1;
                }
                continue;
            }

            if bytes[i] == b'}' {
                let brace_pos = i;
                i += 1;

                // Skip whitespace after the closing brace (but not newlines yet for checking)
                let after_pos = i;

                // Check if there's anything after the brace (not EOF)
                if after_pos >= len {
                    // EOF after brace is fine
                    continue;
                }

                // Skip `};` — a common SCSS pattern (e.g., end of @function body
                // or SCSS map assignment). Stylelint does not flag these.
                if bytes[after_pos] == b';' {
                    continue;
                }

                // Determine if there's a newline immediately after (allowing spaces/tabs before it)
                let mut j = after_pos;
                let mut found_newline = false;
                let mut found_non_ws = false;
                while j < len {
                    if bytes[j] == b'\n' || bytes[j] == b'\r' {
                        found_newline = true;
                        break;
                    } else if bytes[j] == b' ' || bytes[j] == b'\t' {
                        j += 1;
                    } else {
                        found_non_ws = true;
                        break;
                    }
                }

                // Determine if the block is single-line by looking backward for the opening brace
                let is_single_line = is_block_single_line(source, brace_pos);

                // Check if the next non-whitespace content after } is @else or @else if
                // which is valid SCSS: `} @else {` or `} @else if ... {`
                let is_followed_by_else = is_next_else(bytes, after_pos);

                match option {
                    "always" => {
                        // Expect newline after every closing brace (unless followed by another }, EOF, or @else)
                        if found_non_ws
                            && !is_next_closing_brace(bytes, after_pos)
                            && !is_followed_by_else
                        {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Expected newline after \"}\"".to_string(),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(brace_pos, 1)),
                            );
                        }
                    }
                    "never" => {
                        if found_newline {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Unexpected newline after \"}\"".to_string(),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(brace_pos, 1)),
                            );
                        }
                    }
                    "always-single-line" => {
                        if is_single_line
                            && found_non_ws
                            && !is_next_closing_brace(bytes, after_pos)
                            && !is_followed_by_else
                        {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Expected newline after \"}\" of a single-line block"
                                        .to_string(),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(brace_pos, 1)),
                            );
                        }
                    }
                    "always-multi-line" => {
                        if !is_single_line
                            && found_non_ws
                            && !is_next_closing_brace(bytes, after_pos)
                            && !is_followed_by_else
                        {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Expected newline after \"}\" of a multi-line block"
                                        .to_string(),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(brace_pos, 1)),
                            );
                        }
                    }
                    _ => {}
                }
            } else {
                i += 1;
            }
        }

        diagnostics
    }
}

/// Check if the next non-whitespace content after a position is `@else` (SCSS).
fn is_next_else(bytes: &[u8], pos: usize) -> bool {
    let mut j = pos;
    while j < bytes.len()
        && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r')
    {
        j += 1;
    }
    // Check for @else
    if j + 4 < bytes.len()
        && bytes[j] == b'@'
        && bytes[j + 1] == b'e'
        && bytes[j + 2] == b'l'
        && bytes[j + 3] == b's'
        && bytes[j + 4] == b'e'
    {
        // Make sure it's `@else` followed by space, `{`, or end (not `@elsewhere`)
        if j + 5 >= bytes.len()
            || bytes[j + 5] == b' '
            || bytes[j + 5] == b'\t'
            || bytes[j + 5] == b'{'
            || bytes[j + 5] == b'\n'
        {
            return true;
        }
    }
    false
}

fn is_next_closing_brace(bytes: &[u8], pos: usize) -> bool {
    let mut j = pos;
    while j < bytes.len() {
        if bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r' {
            j += 1;
        } else {
            return bytes[j] == b'}';
        }
    }
    false
}

fn is_block_single_line(source: &str, closing_brace_pos: usize) -> bool {
    let bytes = source.as_bytes();
    let mut depth = 1;
    let mut j = closing_brace_pos;
    while j > 0 {
        j -= 1;
        if bytes[j] == b'}' {
            depth += 1;
        } else if bytes[j] == b'{' {
            depth -= 1;
            if depth == 0 {
                // Found the matching opening brace
                let segment = &source[j..closing_brace_pos];
                return !segment.contains('\n');
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "test.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

    fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
        RuleContext {
            file_path: "test.css",
            source,
            syntax: Syntax::Css,
            options: Some(opt),
        }
    }

    #[test]
    fn allows_newline_after_brace() {
        let source = "a { color: red; }\nb { color: blue; }";
        let d = StylisticBlockClosingBraceNewlineAfter.check_root(&[], &ctx(source));
        assert!(d.is_empty());
    }

    #[test]
    fn reports_missing_newline_after_brace() {
        let opt = serde_json::Value::String("always".to_string());
        let source = "a { color: red; } b { color: blue; }";
        let d =
            StylisticBlockClosingBraceNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
        assert!(!d.is_empty());
        assert!(d[0].message.contains("Expected newline"));
    }

    #[test]
    fn never_reports_newline_after_brace() {
        let opt = serde_json::Value::String("never".to_string());
        let source = "a { color: red; }\nb { }";
        let d =
            StylisticBlockClosingBraceNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
        assert!(!d.is_empty());
        assert!(d[0].message.contains("Unexpected newline"));
    }

    #[test]
    fn allows_else_after_closing_brace() {
        let opt = serde_json::Value::String("always".to_string());
        let source = "@if $cond { color: red; } @else { color: blue; }";
        let d =
            StylisticBlockClosingBraceNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
        // The first } before @else should NOT be flagged
        assert!(
            d.is_empty(),
            "Should not flag }} before @else, got: {:?}",
            d.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn allows_else_if_after_closing_brace() {
        let opt = serde_json::Value::String("always".to_string());
        let source = "@if $a { color: red; } @else if $b { color: blue; } @else { color: green; }";
        let d =
            StylisticBlockClosingBraceNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
        assert!(
            d.is_empty(),
            "Should not flag }} before @else if, got: {:?}",
            d.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn allows_semicolon_after_closing_brace() {
        // SCSS pattern: `@function foo() { ... };` or map closings
        let opt = serde_json::Value::String("always".to_string());
        let source = "@function foo() {\n  @return 1;\n};\n";
        let d =
            StylisticBlockClosingBraceNewlineAfter.check_root(&[], &ctx_with_option(source, &opt));
        assert!(
            d.is_empty(),
            "Should not flag }}; pattern, got: {:?}",
            d.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}
