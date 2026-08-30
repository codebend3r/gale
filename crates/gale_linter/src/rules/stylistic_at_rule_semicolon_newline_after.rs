use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline after the semicolon of at-rules.
///
/// Equivalent to Stylelint's `@stylistic/at-rule-semicolon-newline-after` rule.
pub struct StylisticAtRuleSemicolonNewlineAfter;

impl Rule for StylisticAtRuleSemicolonNewlineAfter {
    fn name(&self) -> &'static str {
        "@stylistic/at-rule-semicolon-newline-after"
    }

    fn description(&self) -> &'static str {
        "Require a newline after the semicolon of at-rules"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
        let _option = context.primary_option_str().unwrap_or("always");
        let source = context.source;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut diagnostics = Vec::new();
        let mut i = 0;
        let mut in_at_rule = false;

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
                // At-rules with blocks don't end with semicolons
                in_at_rule = false;
                i += 1;
                continue;
            }

            if bytes[i] == b';' && in_at_rule {
                let semi_pos = i;
                in_at_rule = false;
                i += 1;

                // Check what follows the semicolon
                if i >= len {
                    // EOF after semicolon is fine
                    continue;
                }

                // Skip spaces/tabs, then expect newline
                let mut j = i;
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

                // SCSS line comment after `;` counts as having a newline
                if j + 1 < len && bytes[j] == b'/' && bytes[j + 1] == b'/' {
                    found_newline = true;
                }
                // If the next non-ws character is `}`, the at-rule is the last
                // statement in the block — no newline needed.
                if found_non_ws && !found_newline && j < len && bytes[j] != b'}' {
                    diagnostics.push(
                        Diagnostic::new(
                            self.name(),
                            "Expected newline after \";\" of at-rule".to_string(),
                        )
                        .severity(self.default_severity())
                        .span(Span::new(semi_pos, 1)),
                    );
                }
            } else {
                if bytes[i] == b';' || bytes[i] == b'}' {
                    in_at_rule = false;
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

    fn ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "test.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

    #[test]
    fn allows_newline_after_at_rule_semicolon() {
        let source = "@import url(\"foo.css\");\na { }";
        let d = StylisticAtRuleSemicolonNewlineAfter.check_root(&[], &ctx(source));
        assert!(d.is_empty());
    }

    #[test]
    fn reports_missing_newline_after_at_rule_semicolon() {
        let source = "@import url(\"foo.css\"); a { }";
        let d = StylisticAtRuleSemicolonNewlineAfter.check_root(&[], &ctx(source));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Expected newline"));
    }

    #[test]
    fn allows_eof_after_semicolon() {
        let source = "@charset \"UTF-8\";";
        let d = StylisticAtRuleSemicolonNewlineAfter.check_root(&[], &ctx(source));
        assert!(d.is_empty());
    }
}
