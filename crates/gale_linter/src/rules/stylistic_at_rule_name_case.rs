use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify lowercase or uppercase for at-rule names.
///
/// Equivalent to `@stylistic/at-rule-name-case`.
pub struct StylisticAtRuleNameCase;

impl Rule for StylisticAtRuleNameCase {
    fn name(&self) -> &'static str {
        "@stylistic/at-rule-name-case"
    }

    fn description(&self) -> &'static str {
        "Specify lowercase or uppercase for at-rule names"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
        let option = context.primary_option_str().unwrap_or("lower");
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

            // Skip block comments
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

            if bytes[i] == b'@' {
                let at_pos = i;
                i += 1;

                // Read the at-rule name
                let name_start = i;
                while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                    i += 1;
                }
                let name_end = i;

                if name_start == name_end {
                    continue;
                }

                let name = &source[name_start..name_end];

                let is_wrong = match option {
                    "lower" => name.chars().any(|c| c.is_ascii_uppercase()),
                    "upper" => name.chars().any(|c| c.is_ascii_lowercase()),
                    _ => false,
                };

                if is_wrong {
                    let expected = if option == "lower" {
                        "lowercase"
                    } else {
                        "uppercase"
                    };
                    diagnostics.push(
                        Diagnostic::new(
                            self.name(),
                            format!("Expected at-rule name \"@{}\" to be {}", name, expected),
                        )
                        .severity(self.default_severity())
                        .span(Span::new(at_pos, name_end - at_pos)),
                    );
                }
            } else {
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

    fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
        RuleContext {
            file_path: "test.css",
            source,
            syntax: Syntax::Css,
            options: Some(opt),
        }
    }

    #[test]
    fn allows_lowercase_at_rule_names() {
        let source = "@media screen { } @keyframes foo { }";
        let d = StylisticAtRuleNameCase.check_root(&[], &ctx(source));
        assert!(d.is_empty());
    }

    #[test]
    fn reports_uppercase_at_rule_name_with_lower_option() {
        let opt = serde_json::Value::String("lower".to_string());
        let source = "@Media screen { }";
        let d = StylisticAtRuleNameCase.check_root(&[], &ctx_with_option(source, &opt));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("lowercase"));
    }

    #[test]
    fn allows_uppercase_with_upper_option() {
        let opt = serde_json::Value::String("upper".to_string());
        let source = "@MEDIA screen { }";
        let d = StylisticAtRuleNameCase.check_root(&[], &ctx_with_option(source, &opt));
        assert!(d.is_empty());
    }

    #[test]
    fn reports_lowercase_at_rule_name_with_upper_option() {
        let opt = serde_json::Value::String("upper".to_string());
        let source = "@media screen { }";
        let d = StylisticAtRuleNameCase.check_root(&[], &ctx_with_option(source, &opt));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("uppercase"));
    }

    #[test]
    fn skips_comments() {
        let source = "/* @Media */ @media screen { }";
        let d = StylisticAtRuleNameCase.check_root(&[], &ctx(source));
        assert!(d.is_empty());
    }
}
