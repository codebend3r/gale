use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before `@mixin` parentheses.
///
/// Primary option: `"never"` (default) or `"always"`.
///
/// ```scss
/// // Good (never)
/// @mixin foo($x) {}
///
/// // Good (always)
/// @mixin foo ($x) {}
/// ```
///
/// Equivalent to `scss/at-mixin-parentheses-space-before`.
pub struct ScssAtMixinParenthesesSpaceBefore;

impl Rule for ScssAtMixinParenthesesSpaceBefore {
    fn name(&self) -> &'static str {
        "scss/at-mixin-parentheses-space-before"
    }

    fn description(&self) -> &'static str {
        "Require or disallow a space before @mixin parentheses"
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

        // Scan source for @mixin declarations with parentheses
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Look for `@`
            if bytes[i] != b'@' {
                i += 1;
                continue;
            }

            let at_pos = i;
            i += 1;

            // Check for "mixin" keyword (case-insensitive)
            let remaining = &source[i..];
            if !remaining
                .get(..5)
                .map(|s| s.eq_ignore_ascii_case("mixin"))
                .unwrap_or(false)
            {
                continue;
            }

            i += 5;

            // Must be followed by whitespace (to not match e.g. @mixin-foo)
            if i >= len || (!bytes[i].is_ascii_whitespace()) {
                continue;
            }

            // Skip whitespace to find the mixin name
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }

            // Collect mixin name
            let name_start = i;
            while i < len
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
            {
                i += 1;
            }

            if i == name_start {
                // No name found
                continue;
            }

            // Check if there's a `(` (possibly with a space before it)
            if i >= len {
                continue;
            }

            if bytes[i] == b'(' {
                // No space before paren: `@mixin name(`
                if option == "always" {
                    diagnostics.push(
                        Diagnostic::new(
                            self.name(),
                            "Expected a space before parentheses in @mixin declaration",
                        )
                        .severity(self.default_severity())
                        .span(Span::new(at_pos, i - at_pos + 1)),
                    );
                }
            } else if bytes[i] == b' ' && i + 1 < len && bytes[i + 1] == b'(' {
                // One space before paren: `@mixin name (`
                if option == "never" {
                    diagnostics.push(
                        Diagnostic::new(
                            self.name(),
                            "Unexpected space before parentheses in @mixin declaration",
                        )
                        .severity(self.default_severity())
                        .span(Span::new(at_pos, i - at_pos + 2)),
                    );
                }
                i += 1; // skip past the space
            }
            // else: no parentheses (argumentless mixin) — skip
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
        let src = "@mixin foo($x) { }";
        let d = ScssAtMixinParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn never_reports_space() {
        let src = "@mixin foo ($x) { }";
        let d = ScssAtMixinParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unexpected space"));
    }

    #[test]
    fn always_allows_space() {
        let src = "@mixin foo ($x) { }";
        let opts = serde_json::json!("always");
        let ctx = scss_ctx_with_options(src, &opts);
        let d = ScssAtMixinParenthesesSpaceBefore.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn always_reports_no_space() {
        let src = "@mixin foo($x) { }";
        let opts = serde_json::json!("always");
        let ctx = scss_ctx_with_options(src, &opts);
        let d = ScssAtMixinParenthesesSpaceBefore.check_root(&[], &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Expected a space"));
    }

    #[test]
    fn skips_argumentless_mixin() {
        let src = "@mixin foo { }";
        let d = ScssAtMixinParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_non_scss() {
        let ctx = RuleContext {
            file_path: "t.css",
            source: "@mixin foo($x) { }",
            syntax: Syntax::Css,
            options: None,
        };
        assert!(
            ScssAtMixinParenthesesSpaceBefore
                .check_root(&[], &ctx)
                .is_empty()
        );
    }

    #[test]
    fn handles_multiple_mixins() {
        let src = "@mixin foo($x) { }\n@mixin bar ($y) { }";
        let d = ScssAtMixinParenthesesSpaceBefore.check_root(&[], &scss_ctx(src));
        // default "never": second one has space => 1 report
        assert_eq!(d.len(), 1);
    }
}
