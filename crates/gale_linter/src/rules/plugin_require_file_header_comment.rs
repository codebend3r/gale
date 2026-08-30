use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Require that CSS files begin with a comment matching a specified pattern.
///
/// This is a `check_root` rule that inspects the first node in the file.
///
/// Options (secondary): an object with a `pattern` key containing a regex
/// pattern (as a string, optionally wrapped in `/regex/flags`).
///
/// Example config:
/// ```json
/// ["plugin/require-file-header-comment", [true, {
///   "pattern": "/Copyright|License/i"
/// }]]
/// ```
pub struct PluginRequireFileHeaderComment;

/// A compiled pattern for matching comment text.
enum HeaderPattern {
    Regex(Regex),
    Literal(String),
}

impl HeaderPattern {
    fn from_str(s: &str) -> Self {
        if let Some(inner) = s.strip_prefix('/').and_then(|s| {
            if let Some(pos) = s.rfind('/') {
                Some((&s[..pos], &s[pos + 1..]))
            } else {
                None
            }
        }) {
            let (pattern, flags) = inner;
            let regex_str = if flags.contains('i') {
                format!("(?i){}", pattern)
            } else {
                pattern.to_string()
            };
            match Regex::new(&regex_str) {
                Ok(re) => HeaderPattern::Regex(re),
                Err(_) => HeaderPattern::Literal(s.to_string()),
            }
        } else {
            HeaderPattern::Literal(s.to_string())
        }
    }

    fn matches(&self, text: &str) -> bool {
        match self {
            HeaderPattern::Regex(re) => re.is_match(text),
            HeaderPattern::Literal(s) => text.contains(s.as_str()),
        }
    }
}

fn parse_pattern(context: &RuleContext) -> Option<HeaderPattern> {
    let secondary = context.secondary_options()?;
    let pattern_val = secondary.get("pattern")?;
    let pattern_str = pattern_val.as_str()?;
    Some(HeaderPattern::from_str(pattern_str))
}

impl Rule for PluginRequireFileHeaderComment {
    fn name(&self) -> &'static str {
        "plugin/require-file-header-comment"
    }

    fn description(&self) -> &'static str {
        "Require a file header comment matching a pattern"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
        let pattern = match parse_pattern(context) {
            Some(p) => p,
            None => return vec![],
        };

        // Find the first non-whitespace node
        let first_node = nodes.first();

        match first_node {
            Some(CssNode::Comment(comment)) => {
                if pattern.matches(&comment.text) {
                    // Comment matches pattern — all good
                    vec![]
                } else {
                    vec![
                        Diagnostic::new(
                            self.name(),
                            "File header comment does not match the required pattern",
                        )
                        .severity(self.default_severity())
                        .span(Span::new(comment.span.offset, comment.span.length)),
                    ]
                }
            }
            _ => {
                // No comment at the start of file, or file is empty
                // Use span at the start of the file
                let span = Span::new(0, 0);
                let mut diag = Diagnostic::new(
                    self.name(),
                    "Missing file header comment matching the required pattern",
                )
                .severity(self.default_severity())
                .span(span);

                // Provide autofix: insert a default header comment at the top
                diag = diag.fix(Fix::new(
                    "Insert file header comment",
                    vec![Edit::new(
                        Span::new(0, 0),
                        "/* TODO: Add file header comment */\n".to_string(),
                    )],
                ));

                vec![diag]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{Comment, Span as ParserSpan, StyleRule, Syntax};
    use serde_json::json;

    fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
        RuleContext {
            file_path: "test.css",
            source: "",
            syntax: Syntax::Css,
            options: Some(opts),
        }
    }

    #[test]
    fn passes_with_matching_comment() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "/Copyright|License/i"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = vec![CssNode::Comment(Comment {
            text: " Copyright 2024 Acme Corp ".to_string(),
            span: ParserSpan::new(0, 30),
            is_line: false,
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn passes_with_license_comment() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "/Copyright|License/i"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = vec![CssNode::Comment(Comment {
            text: " MIT License ".to_string(),
            span: ParserSpan::new(0, 16),
            is_line: false,
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn fails_with_wrong_comment() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "/Copyright|License/i"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = vec![CssNode::Comment(Comment {
            text: " This is just a comment ".to_string(),
            span: ParserSpan::new(0, 28),
            is_line: false,
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not match"));
    }

    #[test]
    fn fails_with_no_comment() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "/Copyright/"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = vec![CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![],
            span: ParserSpan::new(0, 10),
            children: vec![],
            nested_at_rules: vec![],
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing file header comment"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn fails_with_empty_file() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "/Copyright/"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes: Vec<CssNode> = vec![];
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn no_options_returns_empty() {
        let rule = PluginRequireFileHeaderComment;
        let ctx = RuleContext {
            file_path: "test.css",
            source: "",
            syntax: Syntax::Css,
            options: None,
        };
        let nodes: Vec<CssNode> = vec![];
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn literal_pattern() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "Copyright"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = vec![CssNode::Comment(Comment {
            text: " Copyright 2024 ".to_string(),
            span: ParserSpan::new(0, 20),
            is_line: false,
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn case_insensitive_regex() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "/copyright/i"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = vec![CssNode::Comment(Comment {
            text: " COPYRIGHT 2024 ".to_string(),
            span: ParserSpan::new(0, 20),
            is_line: false,
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_inserts_comment() {
        let rule = PluginRequireFileHeaderComment;
        let opts = json!([true, {
            "pattern": "/Copyright/"
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = vec![CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![],
            span: ParserSpan::new(0, 10),
            children: vec![],
            nested_at_rules: vec![],
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].span.offset, 0);
        assert!(
            fix.edits[0]
                .new_text
                .contains("TODO: Add file header comment")
        );
    }
}
