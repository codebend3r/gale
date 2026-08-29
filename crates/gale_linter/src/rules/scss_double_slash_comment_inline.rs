use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow or require double-slash comments to be inline (on the same line as code).
///
/// Primary option: `"never"` (default) disallows inline `//` comments.
/// `"always"` requires that `//` comments be inline.
///
/// An inline comment is one where there is non-whitespace content before `//`
/// on the same line.
///
/// Equivalent to `scss/double-slash-comment-inline`.
pub struct ScssDoubleSlashCommentInline;

impl Rule for ScssDoubleSlashCommentInline {
    fn name(&self) -> &'static str {
        "scss/double-slash-comment-inline"
    }

    fn description(&self) -> &'static str {
        "Disallow or require double-slash comments to be inline"
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
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut diagnostics = Vec::new();

        let mut i = 0;
        while i < len {
            // Skip string literals
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Skip block comments
            if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < len {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Found a `//` comment
            if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                let comment_start = i;

                // Determine if this is inline: check if there is non-whitespace
                // content before `//` on the same line
                let is_inline = has_non_whitespace_before_on_line(source, comment_start);

                // Find the end of this comment (end of line)
                let end = source[i..].find('\n').map(|p| i + p).unwrap_or(len);
                let comment_len = end - comment_start;

                match option {
                    "never" => {
                        if is_inline {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Unexpected inline double-slash comment",
                                )
                                .severity(self.default_severity())
                                .span(Span::new(comment_start, comment_len)),
                            );
                        }
                    }
                    "always" => {
                        if !is_inline {
                            diagnostics.push(
                                Diagnostic::new(
                                    self.name(),
                                    "Expected double-slash comment to be inline",
                                )
                                .severity(self.default_severity())
                                .span(Span::new(comment_start, comment_len)),
                            );
                        }
                    }
                    _ => {}
                }

                i = end;
                continue;
            }

            i += 1;
        }

        diagnostics
    }
}

/// Returns `true` if there is non-whitespace content before `pos` on the same line.
fn has_non_whitespace_before_on_line(source: &str, pos: usize) -> bool {
    let bytes = source.as_bytes();
    let mut j = pos;
    while j > 0 {
        j -= 1;
        match bytes[j] {
            b'\n' | b'\r' => return false,
            b' ' | b'\t' => continue,
            _ => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn scss_ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.scss",
            source,
            syntax: Syntax::Scss,
            options: None,
        }
    }

    fn scss_ctx_with_option<'a>(source: &'a str, opts: &'a serde_json::Value) -> RuleContext<'a> {
        RuleContext {
            file_path: "t.scss",
            source,
            syntax: Syntax::Scss,
            options: Some(opts),
        }
    }

    #[test]
    fn never_allows_standalone_comment() {
        let src = "// standalone comment\n.foo { color: red; }";
        let d = ScssDoubleSlashCommentInline.check_root(&[], &scss_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn never_reports_inline_comment() {
        let src = ".foo { color: red; // inline comment\n}";
        let d = ScssDoubleSlashCommentInline.check_root(&[], &scss_ctx(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unexpected inline"));
    }

    #[test]
    fn always_allows_inline_comment() {
        let opts = serde_json::json!("always");
        let src = ".foo { color: red; // inline comment\n}";
        let d = ScssDoubleSlashCommentInline.check_root(&[], &scss_ctx_with_option(src, &opts));
        assert!(d.is_empty());
    }

    #[test]
    fn always_reports_standalone_comment() {
        let opts = serde_json::json!("always");
        let src = "// standalone comment\n.foo { color: red; }";
        let d = ScssDoubleSlashCommentInline.check_root(&[], &scss_ctx_with_option(src, &opts));
        assert_eq!(d.len(), 1);
        assert!(
            d[0].message
                .contains("Expected double-slash comment to be inline")
        );
    }

    #[test]
    fn never_allows_indented_standalone() {
        let src = ".foo {\n  // indented standalone\n  color: red;\n}";
        let d = ScssDoubleSlashCommentInline.check_root(&[], &scss_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_non_scss() {
        let ctx = RuleContext {
            file_path: "t.css",
            source: ".foo { color: red; } // inline",
            syntax: Syntax::Css,
            options: None,
        };
        assert!(
            ScssDoubleSlashCommentInline
                .check_root(&[], &ctx)
                .is_empty()
        );
    }

    #[test]
    fn skips_double_slash_in_string() {
        let src = ".foo { content: \"// not a comment\"; }";
        let d = ScssDoubleSlashCommentInline.check_root(&[], &scss_ctx(src));
        assert!(d.is_empty());
    }
}
