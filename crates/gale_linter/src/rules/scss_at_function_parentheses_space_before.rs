use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before `@function` parentheses.
///
/// Primary option: `"never"` (default) or `"always"`.
///
/// ```scss
/// // Good (never)
/// @function foo($x) { @return $x; }
///
/// // Good (always)
/// @function foo ($x) { @return $x; }
/// ```
///
/// Equivalent to `scss/at-function-parentheses-space-before`.
pub struct ScssAtFunctionParenthesesSpaceBefore;

impl Rule for ScssAtFunctionParenthesesSpaceBefore {
    fn name(&self) -> &'static str {
        "scss/at-function-parentheses-space-before"
    }

    fn description(&self) -> &'static str {
        "Require or disallow a space before @function parentheses"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
        if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
            return vec![];
        }

        let option = ctx.primary_option_str().unwrap_or("never");
        let source = ctx.source;
        let mut diagnostics = Vec::new();

        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i] != b'@' {
                i += 1;
                continue;
            }

            let at_pos = i;
            i += 1;

            // Check for "function" keyword (case-insensitive)
            let remaining = &source[i..];
            if !remaining
                .get(..8)
                .map(|s| s.eq_ignore_ascii_case("function"))
                .unwrap_or(false)
            {
                continue;
            }

            i += 8;

            // Must be followed by whitespace
            if i >= len || !bytes[i].is_ascii_whitespace() {
                continue;
            }

            // Skip whitespace to find the function name
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }

            // Collect function name
            let name_start = i;
            while i < len
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
            {
                i += 1;
            }

            if i == name_start {
                continue;
            }

            if i >= len {
                continue;
            }

            if bytes[i] == b'(' {
                // No space before paren
                if option == "always" {
                    diagnostics.push(
                        Diagnostic::new(
                            self.name(),
                            "Expected a space before parentheses in @function declaration",
                        )
                        .severity(self.default_severity())
                        .span(Span::new(at_pos, i - at_pos + 1)),
                    );
                }
            } else if bytes[i] == b' ' && i + 1 < len && bytes[i + 1] == b'(' {
                // Space before paren
                if option == "never" {
                    diagnostics.push(
                        Diagnostic::new(
                            self.name(),
                            "Unexpected space before parentheses in @function declaration",
                        )
                        .severity(self.default_severity())
                        .span(Span::new(at_pos, i - at_pos + 2)),
                    );
                }
                i += 1;
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn scss_ctx_with_options<'a>(
        source: &'a str,
        options: &'a serde_json::Value,
    ) -> RuleContext<'a> {
        RuleContext {
            file_path: "t.scss",
            source,
            syntax: Syntax::Scss,
            options: Some(options),
        }
    }

    fn scss_ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.scss",
            source,
            syntax: Syntax::Scss,
            options: None,
        }
    }

    #[test]
    fn never_allows_no_space() {
        let src = "@function foo($x) { @return $x; }";
        let d = ScssAtFunctionParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn never_reports_space() {
        let src = "@function foo ($x) { @return $x; }";
        let d = ScssAtFunctionParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unexpected space"));
    }

    #[test]
    fn always_allows_space() {
        let src = "@function foo ($x) { @return $x; }";
        let opts = serde_json::json!("always");
        let ctx = scss_ctx_with_options(src, &opts);
        let d = ScssAtFunctionParenthesesSpaceBefore.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn always_reports_no_space() {
        let src = "@function foo($x) { @return $x; }";
        let opts = serde_json::json!("always");
        let ctx = scss_ctx_with_options(src, &opts);
        let d = ScssAtFunctionParenthesesSpaceBefore.check_root(&[], &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Expected a space"));
    }

    #[test]
    fn skips_non_scss() {
        let ctx = RuleContext {
            file_path: "t.css",
            source: "@function foo($x) { }",
            syntax: Syntax::Css,
            options: None,
        };
        assert!(
            ScssAtFunctionParenthesesSpaceBefore
                .check_root(&[], &ctx)
                .is_empty()
        );
    }

    #[test]
    fn handles_multiple_functions() {
        let src = "@function foo($x) { @return $x; }\n@function bar ($y) { @return $y; }";
        let d = ScssAtFunctionParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
        assert_eq!(d.len(), 1); // only bar has a space
    }
}
