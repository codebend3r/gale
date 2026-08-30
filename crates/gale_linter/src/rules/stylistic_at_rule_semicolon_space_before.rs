use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before at-rule semicolons.
///
/// Equivalent to `@stylistic/at-rule-semicolon-space-before`.
pub struct StylisticAtRuleSemicolonSpaceBefore;

impl Rule for StylisticAtRuleSemicolonSpaceBefore {
    fn name(&self) -> &'static str {
        "@stylistic/at-rule-semicolon-space-before"
    }

    fn description(&self) -> &'static str {
        "Require or disallow a space before at-rule semicolons"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
        let option = context.primary_option_str().unwrap_or("never");
        let source = context.source;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut diagnostics = Vec::new();
        let mut i = 0;
        let mut in_at_rule = false;
        let mut brace_depth: usize = 0;

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

            if bytes[i] == b'@' {
                in_at_rule = true;
                i += 1;
                continue;
            }

            if bytes[i] == b'{' {
                brace_depth += 1;
                in_at_rule = false;
                i += 1;
                continue;
            }

            if bytes[i] == b'}' {
                brace_depth = brace_depth.saturating_sub(1);
                i += 1;
                continue;
            }

            // Check semicolons that terminate at-rules (not inside rule blocks)
            if bytes[i] == b';' && in_at_rule {
                let semi_pos = i;
                let has_space_before = semi_pos > 0 && bytes[semi_pos - 1] == b' ';

                match option {
                    "always" => {
                        if !has_space_before {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Expected a space before \";\"".to_string(),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(semi_pos, 1)),
                            );
                        }
                    }
                    "never" => {
                        if has_space_before {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Unexpected space before \";\"".to_string(),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(semi_pos - 1, 1)),
                            );
                        }
                    }
                    _ => {}
                }
                in_at_rule = false;
            }

            i += 1;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
        RuleContext {
            file_path: "test.css",
            source,
            syntax: Syntax::Css,
            options: Some(opt),
        }
    }

    fn ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "test.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

    #[test]
    fn never_allows_no_space_before_semicolon() {
        let opt = serde_json::Value::String("never".to_string());
        let source = "@import url(foo.css);";
        let d = StylisticAtRuleSemicolonSpaceBefore.check_root(&[], &ctx_with_option(source, &opt));
        assert!(d.is_empty());
    }

    #[test]
    fn never_reports_space_before_semicolon() {
        let source = "@import url(foo.css) ;";
        let d = StylisticAtRuleSemicolonSpaceBefore.check_root(&[], &ctx(source));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unexpected space"));
    }

    #[test]
    fn always_allows_space_before_semicolon() {
        let opt = serde_json::Value::String("always".to_string());
        let source = "@import url(foo.css) ;";
        let d = StylisticAtRuleSemicolonSpaceBefore.check_root(&[], &ctx_with_option(source, &opt));
        assert!(d.is_empty());
    }

    #[test]
    fn always_reports_missing_space_before_semicolon() {
        let opt = serde_json::Value::String("always".to_string());
        let source = "@import url(foo.css);";
        let d = StylisticAtRuleSemicolonSpaceBefore.check_root(&[], &ctx_with_option(source, &opt));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Expected a space"));
    }

    #[test]
    fn skips_comments() {
        let source = "/* @import url(foo.css) ; */ @import url(bar.css);";
        let d = StylisticAtRuleSemicolonSpaceBefore.check_root(&[], &ctx(source));
        assert!(d.is_empty());
    }
}
