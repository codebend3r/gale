use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow an empty line before custom properties (`--*`).
///
/// Equivalent to Stylelint's `custom-property-empty-line-before` rule.
/// Supports primary options: "always", "never".
/// Supports secondary options:
///   - `except`: ["first-nested", "after-comment", "after-custom-property", "after-block"]
///   - `ignore`: ["after-comment", "inside-single-line-block", "first-nested", "after-custom-property"]
pub struct CustomPropertyEmptyLineBefore;

impl Rule for CustomPropertyEmptyLineBefore {
    fn name(&self) -> &'static str {
        "custom-property-empty-line-before"
    }

    fn description(&self) -> &'static str {
        "Require or disallow an empty line before custom properties"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        // Collect declarations from style rules OR at-rules with declaration
        // children (e.g. `@theme inline { --color-bg: ...; }`).
        let decls: Vec<&gale_css_parser::Declaration> = match node {
            CssNode::Style(rule) => rule.declarations.iter().collect(),
            CssNode::AtRule(at) => at
                .children
                .iter()
                .filter_map(|c| {
                    if let CssNode::Declaration(d) = c {
                        Some(d)
                    } else {
                        None
                    }
                })
                .collect(),
            _ => return vec![],
        };
        let opts = Options::from_ctx(ctx);
        let mut diags = Vec::new();

        for decl in decls.iter() {
            // Only check custom properties (starting with --)
            if !decl.property.starts_with("--") {
                continue;
            }

            let decl_start = decl.span.offset;
            if decl_start == 0 || decl_start > ctx.source.len() {
                continue;
            }

            let has_empty = has_empty_line_before(ctx.source, decl_start);

            // Check conditions
            let is_first = is_first_in_block_by_source(ctx.source, decl_start);
            let is_single_line = match node {
                CssNode::Style(rule) => is_single_line_block(ctx.source, rule),
                _ => false,
            };
            let after_comment = is_after_comment(ctx.source, decl_start);
            // Use source-based check for after-custom-property to avoid false
            // positives when a non-adjacent custom property (e.g. inside a
            // nested @include block) appears before this one in the AST's
            // declarations array but is not physically adjacent in the source.
            let after_custom_property = is_after_custom_property(ctx.source, decl_start);
            let after_block = is_after_block(ctx.source, decl_start);

            // Apply ignore options (skip this declaration entirely)
            if opts.ignore_inside_single_line_block && is_single_line {
                continue;
            }
            if opts.ignore_after_comment && after_comment {
                continue;
            }
            if opts.ignore_first_nested && is_first {
                continue;
            }
            if opts.ignore_after_custom_property && after_custom_property {
                continue;
            }

            // Determine base expectation
            let mut expectation = match opts.primary {
                PrimaryOption::Always => true, // expects empty line
                PrimaryOption::Never => false, // expects no empty line
            };

            // Apply exceptions (flip expectation)
            if opts.except_first_nested && is_first {
                expectation = !expectation;
            }
            if opts.except_after_comment && after_comment {
                expectation = !expectation;
            }
            if opts.except_after_custom_property && after_custom_property {
                expectation = !expectation;
            }
            if opts.except_after_block && after_block {
                expectation = !expectation;
            }

            // Report violations
            if expectation && !has_empty {
                diags.push(
                    Diagnostic::new(
                        self.name(),
                        "Expected empty line before custom property".to_string(),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(decl.span.offset, decl.span.length)),
                );
            } else if !expectation && has_empty {
                diags.push(
                    Diagnostic::new(
                        self.name(),
                        "Unexpected empty line before custom property".to_string(),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(decl.span.offset, decl.span.length)),
                );
            }
        }
        diags
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum PrimaryOption {
    Always,
    Never,
}

struct Options {
    primary: PrimaryOption,
    except_first_nested: bool,
    except_after_comment: bool,
    except_after_custom_property: bool,
    except_after_block: bool,
    ignore_after_comment: bool,
    ignore_inside_single_line_block: bool,
    ignore_first_nested: bool,
    ignore_after_custom_property: bool,
}

impl Options {
    fn from_ctx(ctx: &RuleContext) -> Self {
        let mut opts = Options {
            primary: PrimaryOption::Always,
            except_first_nested: false,
            except_after_comment: false,
            except_after_custom_property: false,
            except_after_block: false,
            ignore_after_comment: false,
            ignore_inside_single_line_block: false,
            ignore_first_nested: false,
            ignore_after_custom_property: false,
        };

        let Some(value) = ctx.options else {
            return opts;
        };

        match value {
            serde_json::Value::String(s) => {
                opts.primary = parse_primary(s);
            }
            serde_json::Value::Array(arr) => {
                if let Some(primary_str) = arr.first().and_then(|v| v.as_str()) {
                    opts.primary = parse_primary(primary_str);
                }
                if let Some(secondary) = arr.get(1) {
                    parse_secondary(&mut opts, secondary);
                }
            }
            _ => {}
        }

        opts
    }
}

fn parse_primary(s: &str) -> PrimaryOption {
    match s {
        "never" => PrimaryOption::Never,
        _ => PrimaryOption::Always,
    }
}

fn parse_secondary(opts: &mut Options, value: &serde_json::Value) {
    if let Some(except) = value.get("except").and_then(|v| v.as_array()) {
        for item in except {
            if let Some(s) = item.as_str() {
                match s {
                    "first-nested" => opts.except_first_nested = true,
                    "after-comment" => opts.except_after_comment = true,
                    "after-custom-property" => opts.except_after_custom_property = true,
                    "after-block" => opts.except_after_block = true,
                    _ => {}
                }
            }
        }
    }
    if let Some(ignore) = value.get("ignore").and_then(|v| v.as_array()) {
        for item in ignore {
            if let Some(s) = item.as_str() {
                match s {
                    "after-comment" => opts.ignore_after_comment = true,
                    "inside-single-line-block" => opts.ignore_inside_single_line_block = true,
                    "first-nested" => opts.ignore_first_nested = true,
                    "after-custom-property" => opts.ignore_after_custom_property = true,
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Source analysis helpers
// ---------------------------------------------------------------------------

/// Check if the text before a node has an empty line.
fn has_empty_line_before(source: &str, offset: usize) -> bool {
    if offset == 0 || offset > source.len() {
        return false;
    }
    let before = &source[..offset];

    // Find the start of the current line
    let last_newline = before.rfind('\n');
    let Some(nl_pos) = last_newline else {
        return false;
    };

    let before_lines = &before[..nl_pos];
    let mut found_blank = false;
    for line in before_lines.rsplit('\n') {
        let stripped = line.trim_matches(|c: char| c == ' ' || c == '\t' || c == '\r');
        if stripped.is_empty() {
            found_blank = true;
        } else {
            return found_blank;
        }
    }
    // If we reached the start without finding non-blank content, and found
    // at least one blank line, return true
    found_blank
}

/// Check if the declaration is the first thing after an opening brace.
///
/// Handles block comments and SCSS line comments between `{` and the declaration.
fn is_first_in_block_by_source(source: &str, offset: usize) -> bool {
    if offset == 0 || offset > source.len() {
        return false;
    }
    let before = &source[..offset];

    // Walk backwards through the source, skipping comments and whitespace,
    // to see if we eventually reach an opening brace.
    let bytes = before.as_bytes();
    let mut pos = before.len();
    loop {
        // Skip trailing whitespace
        while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t' | b'\n' | b'\r') {
            pos -= 1;
        }
        if pos == 0 {
            return false;
        }
        // Check for end of a block comment `*/`
        if pos >= 2 && &before[pos - 2..pos] == "*/" {
            // Find the matching `/*`
            if let Some(open) = before[..pos - 2].rfind("/*") {
                pos = open;
                continue;
            }
            return false;
        }
        // Check for SCSS line comment
        let line_start = before[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line = before[line_start..pos].trim();
        if line.starts_with("//") {
            pos = line_start;
            continue;
        }
        // If the line contains a `//` comment after a `{`, strip the comment
        if let Some(comment_pos) = line.find("//") {
            let before_comment = line[..comment_pos].trim();
            if before_comment.ends_with('{') {
                return true;
            }
        }
        // Not a comment — check if it's `{`
        return bytes[pos - 1] == b'{';
    }
}

/// Check if preceded by a comment.
///
/// Stylelint considers a node to be "after-comment" when the previous
/// line (before the declaration's line) ends with a comment. This includes:
/// - A standalone comment on its own line: `/* comment */\n decl`
/// - A trailing inline comment: `color: pink; /* comment */\n decl`
/// - SCSS line comments on their own line
///
/// NOT considered after-comment:
/// - Comment on the same line as the declaration
/// - Comment on the opening brace line: `a {/* comment */\n decl`
fn is_after_comment(source: &str, offset: usize) -> bool {
    if offset < 2 || offset > source.len() {
        return false;
    }
    let before = &source[..offset];

    // The declaration must be on a different line from the comment.
    let decl_line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let before_decl_line = &before[..decl_line_start];

    for line in before_decl_line.lines().rev() {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        if stripped.ends_with("*/") {
            // Not after-comment if the line starts with `{`
            let before_comment = if let Some(open) = stripped.rfind("/*") {
                stripped[..open].trim()
            } else {
                ""
            };
            if before_comment.ends_with('{') {
                return false;
            }
            return true;
        }
        // SCSS line comment on its own line
        if stripped.starts_with("//") {
            return true;
        }
        return false;
    }
    false
}

/// Check if preceded by another custom property declaration.
fn is_after_custom_property(source: &str, offset: usize) -> bool {
    if offset == 0 || offset > source.len() {
        return false;
    }
    let before = &source[..offset];
    let lines: Vec<&str> = before.lines().collect();
    let mut i = lines.len();

    // Walk backwards to find the previous meaningful line
    while i > 0 {
        i -= 1;
        let stripped = lines[i].trim();
        if stripped.is_empty() {
            continue;
        }
        // Check if it's a custom property declaration
        if stripped.starts_with("--") {
            return true;
        }
        // If preceded by a comment, keep looking.
        if stripped.ends_with("*/") || stripped.starts_with("/*") || stripped.starts_with("//") {
            continue;
        }
        // The previous non-empty, non-comment line ends with `;` — this is the
        // end of a declaration. It may be a multi-line value; scan back to the
        // start of that declaration to check if it's a custom property.
        if stripped.ends_with(';') {
            while i > 0 {
                i -= 1;
                let s = lines[i].trim();
                if s.is_empty() {
                    continue;
                }
                if s.starts_with("--") {
                    return true;
                }
                // Block boundary — stop
                if s.starts_with('{') || s.ends_with('{') || s == "}" {
                    return false;
                }
                // Another complete statement (ends with `;`) — this is a
                // separate declaration, not a continuation of the value.
                // Stop scanning to avoid crossing into unrelated declarations.
                if s.ends_with(';') {
                    return false;
                }
                // Otherwise it's a continuation line of the value — keep scanning
            }
        }
        return false;
    }
    false
}

/// Check if preceded by a block (a rule or at-rule ending with `}`).
fn is_after_block(source: &str, offset: usize) -> bool {
    if offset == 0 || offset > source.len() {
        return false;
    }
    let before = &source[..offset];

    for line in before.lines().rev() {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        // A block ends with `}`
        return stripped.ends_with('}');
    }
    false
}

/// Check if the style rule is a single-line block.
fn is_single_line_block(source: &str, rule: &gale_css_parser::StyleRule) -> bool {
    let start = rule.span.offset;
    let end = (start + rule.span.length).min(source.len());
    if start >= source.len() {
        return false;
    }
    !source[start..end].contains('\n')
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

    fn make_ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

    fn make_ctx_with_options<'a>(
        source: &'a str,
        options: &'a serde_json::Value,
    ) -> RuleContext<'a> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options: Some(options),
        }
    }

    #[test]
    fn always_reports_missing_empty_line() {
        let src = "a {\n  color: red;\n  --my-var: blue;\n}";
        let var_offset = src.find("--my-var").unwrap();
        let opts = serde_json::json!(["always"]);
        let node = CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![
                Declaration {
                    property: "color".to_string(),
                    value: "red".to_string(),
                    span: ParserSpan::new(src.find("color").unwrap(), "color: red;".len()),
                    important: false,
                },
                Declaration {
                    property: "--my-var".to_string(),
                    value: "blue".to_string(),
                    span: ParserSpan::new(var_offset, "--my-var: blue;".len()),
                    important: false,
                },
            ],
            span: ParserSpan::new(0, src.len()),
            ..Default::default()
        });
        let d = CustomPropertyEmptyLineBefore.check(&node, &make_ctx_with_options(src, &opts));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("custom property"));
    }

    #[test]
    fn always_allows_empty_line() {
        let src = "a {\n  color: red;\n\n  --my-var: blue;\n}";
        let var_offset = src.find("--my-var").unwrap();
        let opts = serde_json::json!(["always"]);
        let node = CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![
                Declaration {
                    property: "color".to_string(),
                    value: "red".to_string(),
                    span: ParserSpan::new(src.find("color").unwrap(), "color: red;".len()),
                    important: false,
                },
                Declaration {
                    property: "--my-var".to_string(),
                    value: "blue".to_string(),
                    span: ParserSpan::new(var_offset, "--my-var: blue;".len()),
                    important: false,
                },
            ],
            span: ParserSpan::new(0, src.len()),
            ..Default::default()
        });
        let d = CustomPropertyEmptyLineBefore.check(&node, &make_ctx_with_options(src, &opts));
        assert!(d.is_empty());
    }

    #[test]
    fn never_reports_empty_line() {
        let src = "a {\n  color: red;\n\n  --my-var: blue;\n}";
        let var_offset = src.find("--my-var").unwrap();
        let opts = serde_json::json!(["never"]);
        let node = CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![
                Declaration {
                    property: "color".to_string(),
                    value: "red".to_string(),
                    span: ParserSpan::new(src.find("color").unwrap(), "color: red;".len()),
                    important: false,
                },
                Declaration {
                    property: "--my-var".to_string(),
                    value: "blue".to_string(),
                    span: ParserSpan::new(var_offset, "--my-var: blue;".len()),
                    important: false,
                },
            ],
            span: ParserSpan::new(0, src.len()),
            ..Default::default()
        });
        let d = CustomPropertyEmptyLineBefore.check(&node, &make_ctx_with_options(src, &opts));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unexpected"));
    }
}
