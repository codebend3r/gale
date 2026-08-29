use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require an empty line before comments (except first-nested).
///
/// Equivalent to Stylelint's `comment-empty-line-before` rule with "always" option.
/// Detection-only (no autofix).
///
/// Uses source-level scanning to find all block comments (`/* ... */`), including
/// those inside style rule blocks and at-rule blocks that the parser may not
/// include in the AST.
pub struct CommentEmptyLineBefore;

impl Rule for CommentEmptyLineBefore {
    fn name(&self) -> &'static str {
        "comment-empty-line-before"
    }

    fn description(&self) -> &'static str {
        "Require an empty line before comments"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
        let source = ctx.source;
        if source.is_empty() {
            return vec![];
        }

        // Read options
        let secondary = ctx.secondary_options().or(ctx.options);

        let except_first_nested = secondary
            .and_then(|v| v.get("except"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|item| item.as_str() == Some("first-nested")))
            .unwrap_or(false);

        let ignore_after_comment = secondary
            .and_then(|v| v.get("ignore"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|item| item.as_str() == Some("after-comment"))
            })
            .unwrap_or(false);

        let ignore_stylelint_commands = secondary
            .and_then(|v| v.get("ignore"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|item| item.as_str() == Some("stylelint-commands"))
            })
            .unwrap_or(false);

        // ignoreComments: array of string or regex patterns.  When a comment's
        // text matches any of these patterns the rule is skipped for that comment.
        let ignore_comment_patterns: Vec<String> = secondary
            .and_then(|v| v.get("ignoreComments"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let mut diags = Vec::new();
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i + 1 < len {
            // Skip string literals
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                i += 1;
                while i < len && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                continue;
            }

            // Skip SCSS line comments — they are not checked by this rule
            if bytes[i] == b'/' && bytes[i + 1] == b'/' {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            // Found a block comment
            if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let comment_start = i;
                i += 2;
                // Find end of comment
                let comment_end = loop {
                    if i + 1 >= len {
                        break len;
                    }
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        break i + 2;
                    }
                    i += 1;
                };
                i = comment_end;

                let comment_text = &source[comment_start + 2..comment_end.saturating_sub(2)];

                // Skip stylelint/gale command comments
                if ignore_stylelint_commands && is_stylelint_command(comment_text) {
                    continue;
                }

                // Skip comments matching ignoreComments patterns.
                // Patterns may be plain strings (exact match) or regex patterns.
                // JS config regex literals `/pattern/` are converted to plain
                // strings by the config parser, so we try regex first then fall
                // back to exact string match.
                if !ignore_comment_patterns.is_empty() {
                    let trimmed_comment = comment_text.trim();
                    let should_ignore = ignore_comment_patterns.iter().any(|pat| {
                        let re_str = if pat.starts_with('/') && pat.ends_with('/') {
                            &pat[1..pat.len() - 1]
                        } else {
                            pat.as_str()
                        };
                        // Try as regex first; if it's not a valid regex or doesn't
                        // look like one, fall back to exact match.
                        if let Ok(re) = regex::Regex::new(re_str) {
                            re.is_match(trimmed_comment)
                        } else {
                            trimmed_comment == pat
                        }
                    });
                    if should_ignore {
                        continue;
                    }
                }

                let before = &source[..comment_start];

                // Skip inline comments (comments on the same line as other content)
                let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
                let prefix = &before[line_start..];
                if !prefix.trim().is_empty() {
                    continue;
                }

                // Skip comments embedded in selectors (e.g., between selector
                // parts in a comma-separated list).  Heuristic: if the last
                // non-whitespace character before the comment is `,`, the
                // comment is inside a selector list — Stylelint's PostCSS AST
                // does not represent these as standalone comment nodes so they
                // are not checked.
                let trimmed_before_comma = before.trim_end();
                if trimmed_before_comma.ends_with(',') {
                    continue;
                }

                // except/ignore: first-nested — the first thing in a block after `{`
                let trimmed_all_ws = before.trim();
                if trimmed_all_ws.is_empty() || trimmed_all_ws.ends_with('{') {
                    // If except_first_nested is set, we flip the expectation
                    // (which for "always" means we'd NOT require empty line).
                    // In the default "always" mode with except: ["first-nested"],
                    // first-nested comments are skipped.
                    if except_first_nested {
                        // For "always" + except: ["first-nested"], don't report
                        continue;
                    }
                    // If the comment is first in the file or first in a block
                    // and except_first_nested is NOT set, skip it since there's
                    // nothing to separate from (this is the default behavior)
                    continue;
                }

                // ignore: ["after-comment"] — skip if the previous non-whitespace
                // content is also a block comment ending with `*/`
                if ignore_after_comment {
                    let trimmed_before = before.trim_end();
                    if trimmed_before.ends_with("*/") {
                        continue;
                    }
                    // Also check for SCSS // comment on the previous line
                    let prev_line_start = trimmed_before.rfind('\n').map(|p| p + 1).unwrap_or(0);
                    let prev_line = trimmed_before[prev_line_start..].trim();
                    if prev_line.starts_with("//") {
                        continue;
                    }
                }

                // Check for empty line before the comment.
                // An "empty line" is any line containing only whitespace.
                let has_empty_line = {
                    let b4 = before.trim_end_matches(|c: char| c == ' ' || c == '\t');
                    if let Some(last_nl) = b4.rfind('\n') {
                        let prev = &b4[..last_nl];
                        if let Some(prev_nl) = prev.rfind('\n') {
                            prev[prev_nl + 1..].trim().is_empty()
                        } else {
                            prev.trim().is_empty()
                        }
                    } else {
                        false
                    }
                };
                if !has_empty_line {
                    diags.push(
                        Diagnostic::new(
                            self.name(),
                            "Expected empty line before comment".to_string(),
                        )
                        .severity(self.default_severity())
                        .span(Span::new(comment_start, comment_end - comment_start)),
                    );
                }

                continue;
            }

            i += 1;
        }

        diags
    }
}

/// Check whether a comment text contains a stylelint/gale command directive.
fn is_stylelint_command(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("stylelint-disable")
        || t.starts_with("stylelint-enable")
        || t.starts_with("gale-disable")
        || t.starts_with("gale-enable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn make_ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

    #[test]
    fn reports_missing_empty_line_before_comment() {
        let src = "a { color: red; }\n/* comment */";
        let d = CommentEmptyLineBefore.check_root(&[], &make_ctx(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("comment"));
    }

    #[test]
    fn allows_empty_line_before_comment() {
        let src = "a { color: red; }\n\n/* comment */";
        let d = CommentEmptyLineBefore.check_root(&[], &make_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn allows_first_nested_comment() {
        let src = "/* first comment */";
        let d = CommentEmptyLineBefore.check_root(&[], &make_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_inline_comment_after_declaration() {
        let src = "a { color: red; }\nz-index: 1; /* inline comment */";
        let d = CommentEmptyLineBefore.check_root(&[], &make_ctx(src));
        assert!(
            d.is_empty(),
            "Inline comments should not be flagged: {:?}",
            d
        );
    }

    #[test]
    fn reports_comment_inside_block_without_empty_line() {
        let src = ".foo {\n  color: red;\n  /* comment */\n}";
        let d = CommentEmptyLineBefore.check_root(&[], &make_ctx(src));
        assert_eq!(
            d.len(),
            1,
            "Should flag comment after declaration without empty line"
        );
    }

    #[test]
    fn allows_comment_inside_block_with_empty_line() {
        let src = ".foo {\n  color: red;\n\n  /* comment */\n}";
        let d = CommentEmptyLineBefore.check_root(&[], &make_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_first_nested_inside_block() {
        let src = ".foo {\n  /* first comment in block */\n  color: red;\n}";
        let d = CommentEmptyLineBefore.check_root(&[], &make_ctx(src));
        assert!(
            d.is_empty(),
            "First-nested comment in block should not be flagged"
        );
    }

    #[test]
    fn ignore_after_comment_option() {
        let src = ".foo {\n\n  /* first */\n  /* second */\n}";
        let opts = serde_json::json!(["always", {"except": ["first-nested"], "ignore": ["after-comment", "stylelint-commands"]}]);
        let ctx = RuleContext {
            file_path: "t.css",
            source: src,
            syntax: Syntax::Css,
            options: Some(&opts),
        };
        let d = CommentEmptyLineBefore.check_root(&[], &ctx);
        assert!(
            d.is_empty(),
            "Comment after comment should be skipped with ignore option: {:?}",
            d
        );
    }
}
