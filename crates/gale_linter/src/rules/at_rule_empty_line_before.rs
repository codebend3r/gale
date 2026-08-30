use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow an empty line before at-rules.
///
/// Equivalent to Stylelint's `at-rule-empty-line-before` rule.
/// Supports primary: "always" | "never"
/// Supports secondary options:
///   - `except`: ["first-nested", "blockless-after-same-name-blockless", "blockless-after-blockless",
///                "after-same-name"]
///   - `ignore`: ["after-comment", "first-nested", "inside-block", "blockless-after-same-name-blockless",
///                "blockless-after-blockless"]
///   - `ignoreAtRules`: [string]
pub struct AtRuleEmptyLineBefore;

impl Rule for AtRuleEmptyLineBefore {
    fn name(&self) -> &'static str {
        "at-rule-empty-line-before"
    }

    fn description(&self) -> &'static str {
        "Require or disallow an empty line before at-rules"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
        let opts = Options::from_ctx(ctx);
        let mut diags = Vec::new();
        check_at_rule_nodes(self, nodes, ctx, &opts, &mut diags);
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
    except_blockless_after_same_name_blockless: bool,
    except_blockless_after_blockless: bool,
    except_after_same_name: bool,
    ignore_after_comment: bool,
    ignore_first_nested: bool,
    ignore_inside_block: bool,
    ignore_blockless_after_same_name_blockless: bool,
    ignore_blockless_after_blockless: bool,
    ignore_at_rules: Vec<String>,
}

impl Options {
    fn from_ctx(ctx: &RuleContext) -> Self {
        let mut opts = Options {
            primary: PrimaryOption::Always,
            except_first_nested: false,
            except_blockless_after_same_name_blockless: false,
            except_blockless_after_blockless: false,
            except_after_same_name: false,
            ignore_after_comment: false,
            ignore_first_nested: false,
            ignore_inside_block: false,
            ignore_blockless_after_same_name_blockless: false,
            ignore_blockless_after_blockless: false,
            ignore_at_rules: Vec::new(),
        };

        let Some(value) = ctx.options else {
            return opts;
        };

        match value {
            serde_json::Value::String(s) => {
                if s == "never" {
                    opts.primary = PrimaryOption::Never;
                }
            }
            serde_json::Value::Array(arr) => {
                if let Some(s) = arr.first().and_then(|v| v.as_str())
                    && s == "never"
                {
                    opts.primary = PrimaryOption::Never;
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

fn parse_secondary(opts: &mut Options, value: &serde_json::Value) {
    if let Some(except) = value.get("except").and_then(|v| v.as_array()) {
        for item in except {
            if let Some(s) = item.as_str() {
                match s {
                    "first-nested" => opts.except_first_nested = true,
                    "blockless-after-same-name-blockless" => {
                        opts.except_blockless_after_same_name_blockless = true
                    }
                    "blockless-after-blockless" => opts.except_blockless_after_blockless = true,
                    "after-same-name" => opts.except_after_same_name = true,
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
                    "first-nested" => opts.ignore_first_nested = true,
                    "inside-block" => opts.ignore_inside_block = true,
                    "blockless-after-same-name-blockless" => {
                        opts.ignore_blockless_after_same_name_blockless = true
                    }
                    "blockless-after-blockless" => opts.ignore_blockless_after_blockless = true,
                    _ => {}
                }
            }
        }
    }
    if let Some(ignore_rules) = value.get("ignoreAtRules") {
        if let Some(arr) = ignore_rules.as_array() {
            for item in arr {
                if let Some(s) = item.as_str() {
                    opts.ignore_at_rules.push(s.to_ascii_lowercase());
                }
            }
        } else if let Some(s) = ignore_rules.as_str() {
            opts.ignore_at_rules.push(s.to_ascii_lowercase());
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Whether an at-rule is "blockless" (has no body/children block).
/// Note: some parsers (raffia for SCSS) don't always populate children for
/// at-rules like @mixin, @function, etc. We also check the source text to
/// see if the at-rule span contains a block-opening `{` (not SCSS `#{`).
fn is_blockless_source(at_rule: &gale_css_parser::AtRule, source: &str) -> bool {
    if !at_rule.children.is_empty() {
        return false;
    }

    // Some at-rules are inherently blockless (they always end with `;`).
    let name_lower = at_rule.name.to_ascii_lowercase();
    if matches!(
        name_lower.as_str(),
        "import" | "use" | "forward" | "charset" | "namespace" | "layer"
    ) && at_rule.children.is_empty()
    {
        return true;
    }

    // Check the source text within this at-rule's span for a `{` that isn't
    // part of SCSS interpolation `#{`.
    // Note: some parsers report overly-long spans that extend beyond the actual
    // at-rule statement. Limit the search to the first line that ends with `;`
    // or `{` to avoid false matches from subsequent content.
    let start = at_rule.span.offset;
    let end = (start + at_rule.span.length).min(source.len());
    if start < source.len() && end > start {
        let bytes = source[start..end].as_bytes();
        for j in 0..bytes.len() {
            if bytes[j] == b'{' {
                // Skip if preceded by `#` (SCSS interpolation)
                if j > 0 && bytes[j - 1] == b'#' {
                    continue;
                }
                return false; // Has a real block
            }
            // If we hit a semicolon before any `{`, the at-rule ends with `;`
            // (blockless).
            if bytes[j] == b';' {
                return true;
            }
        }
    }
    true
}

/// Find the previous non-comment node.
fn prev_non_comment(nodes: &[CssNode], index: usize) -> Option<&CssNode> {
    if index == 0 {
        return None;
    }
    let mut k = index - 1;
    loop {
        if !matches!(&nodes[k], CssNode::Comment(_)) {
            return Some(&nodes[k]);
        }
        if k == 0 {
            return None;
        }
        k -= 1;
    }
}

fn check_at_rule_nodes(
    rule_impl: &AtRuleEmptyLineBefore,
    nodes: &[CssNode],
    ctx: &RuleContext,
    opts: &Options,
    diags: &mut Vec<Diagnostic>,
) {
    for (i, node) in nodes.iter().enumerate() {
        if let CssNode::AtRule(at_rule) = node {
            // Always recurse into children first
            check_at_rule_nodes(rule_impl, &at_rule.children, ctx, opts, diags);

            // Skip non-standard at-rules: Stylelint's isStandardSyntaxAtRule returns false
            // for at-rules with no params and no block (e.g. `@content;` inside mixins).
            if at_rule.params.is_empty()
                && at_rule.children.is_empty()
                && is_blockless_source(at_rule, ctx.source)
            {
                continue;
            }

            // Check if this at-rule is in the ignoreAtRules list.
            // Match both exact names and prefix matches (e.g. "else" matches
            // both "@else" and "@else if").
            if !opts.ignore_at_rules.is_empty() {
                let name_lower = at_rule.name.to_ascii_lowercase();
                if opts.ignore_at_rules.iter().any(|r| {
                    r == &name_lower
                        || (name_lower.starts_with(r.as_str())
                            && name_lower[r.len()..].starts_with(' '))
                }) {
                    continue;
                }
            }

            let offset = at_rule.span.offset;
            if offset == 0 || offset > ctx.source.len() {
                continue;
            }

            let before = &ctx.source[..offset];

            // Determine if this is first-nested (first thing in a block).
            // Use source-level check: the at-rule immediately follows an
            // opening brace `{` (possibly with comments/whitespace between).
            let is_first_nested = is_first_in_block(before);

            // ignore: first-nested
            if opts.ignore_first_nested && is_first_nested {
                continue;
            }

            // ignore: after-comment — check if the previous node is a comment
            if opts.ignore_after_comment && i > 0 && matches!(nodes[i - 1], CssNode::Comment(_)) {
                continue;
            }

            // Also check source-level for after-comment (comment right before this at-rule)
            if opts.ignore_after_comment && is_after_comment_source(before) {
                continue;
            }

            // Determine blockless properties
            let current_blockless = is_blockless_source(at_rule, ctx.source);
            let current_name = at_rule.name.to_ascii_lowercase();

            let prev = prev_non_comment(nodes, i);

            let is_blockless_after_same_name_blockless = current_blockless
                && prev
                    .and_then(|n| {
                        if let CssNode::AtRule(prev_at) = n {
                            Some(prev_at)
                        } else {
                            None
                        }
                    })
                    .map(|prev_at| {
                        is_blockless_source(prev_at, ctx.source)
                            && prev_at.name.eq_ignore_ascii_case(&current_name)
                    })
                    .unwrap_or(false);

            let is_blockless_after_blockless = current_blockless
                && prev
                    .and_then(|n| {
                        if let CssNode::AtRule(prev_at) = n {
                            Some(prev_at)
                        } else {
                            None
                        }
                    })
                    .map(|prev_at| is_blockless_source(prev_at, ctx.source))
                    .unwrap_or(false);

            let is_after_same_name = prev
                .and_then(|n| {
                    if let CssNode::AtRule(prev_at) = n {
                        Some(prev_at)
                    } else {
                        None
                    }
                })
                .map(|prev_at| prev_at.name.eq_ignore_ascii_case(&current_name))
                .unwrap_or(false);

            // ignore options (skip entirely)
            if opts.ignore_blockless_after_same_name_blockless
                && is_blockless_after_same_name_blockless
            {
                continue;
            }
            if opts.ignore_blockless_after_blockless && is_blockless_after_blockless {
                continue;
            }
            if opts.ignore_inside_block && !is_first_nested && i > 0 {
                // If we're inside a block (not at root), skip
                let trimmed = before.trim();
                if !trimmed.is_empty() && !trimmed.ends_with('{') {
                    // We're inside a block if the context has a brace
                    // Simple heuristic: skip if not at top level
                }
            }

            // Determine base expectation
            let mut expectation = match opts.primary {
                PrimaryOption::Always => true,
                PrimaryOption::Never => false,
            };

            // Apply except options (flip expectation ONCE if any condition matches).
            // Multiple matching conditions do not stack — the expectation is only
            // flipped a single time, matching Stylelint's behavior.
            let any_except = (opts.except_first_nested && is_first_nested)
                || (opts.except_blockless_after_same_name_blockless
                    && is_blockless_after_same_name_blockless)
                || (opts.except_blockless_after_blockless && is_blockless_after_blockless)
                || (opts.except_after_same_name && is_after_same_name);
            if any_except {
                expectation = !expectation;
            }

            let has_empty = has_empty_line_before(before);

            if expectation && !has_empty {
                diags.push(
                    Diagnostic::new(
                        rule_impl.name(),
                        "Expected empty line before at-rule".to_string(),
                    )
                    .severity(rule_impl.default_severity())
                    .span(Span::new(at_rule.span.offset, at_rule.span.length)),
                );
            } else if !expectation && has_empty {
                diags.push(
                    Diagnostic::new(
                        rule_impl.name(),
                        "Unexpected empty line before at-rule".to_string(),
                    )
                    .severity(rule_impl.default_severity())
                    .span(Span::new(at_rule.span.offset, at_rule.span.length)),
                );
            }
        }

        // Also recurse into style rules to find nested at-rules
        if let CssNode::Style(style) = node {
            // Collect child nodes (nested style rules + nested at-rules + declarations).
            // Declarations must be included so that prev_non_comment() correctly
            // identifies them as the previous sibling, preventing blockless-after-same-name-blockless
            // from firing when declarations separate two @include calls.
            let mut child_nodes: Vec<CssNode> = style
                .children
                .iter()
                .map(|sr| CssNode::Style(sr.clone()))
                .collect();
            child_nodes.extend(style.nested_at_rules.iter().cloned());
            child_nodes.extend(
                style
                    .declarations
                    .iter()
                    .map(|d| CssNode::Declaration(d.clone())),
            );
            // Sort by offset so order is correct
            child_nodes.sort_by_key(|n| match n {
                CssNode::Style(s) => s.span.offset,
                CssNode::AtRule(a) => a.span.offset,
                CssNode::Comment(c) => c.span.offset,
                CssNode::Declaration(d) => d.span.offset,
            });
            check_at_rule_nodes(rule_impl, &child_nodes, ctx, opts, diags);
        }
    }
}

fn has_empty_line_before(before: &str) -> bool {
    // Walk backwards from the end of `before` to find an empty line
    // (a line containing only whitespace).
    let bytes = before.as_bytes();
    let mut pos = bytes.len();

    // Skip trailing horizontal whitespace (the indentation before the at-rule)
    while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
        pos -= 1;
    }

    // Expect a newline
    if pos > 0 && bytes[pos - 1] == b'\n' {
        pos -= 1;
        if pos > 0 && bytes[pos - 1] == b'\r' {
            pos -= 1;
        }
    } else {
        return false;
    }

    // Now check the previous line: skip horizontal whitespace
    while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
        pos -= 1;
    }

    // Empty line if we hit another newline (or start of content)
    pos == 0 || bytes[pos - 1] == b'\n'
}

/// Byte-slice version of `find_line_comment_start`.  Operates on raw bytes so
/// it is safe on source files that contain non-ASCII (multi-byte) characters.
fn find_line_comment_start_bytes(line: &[u8]) -> Option<usize> {
    let len = line.len();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < len {
        let ch = line[i];
        if in_single_quote {
            if ch == b'\'' && (i == 0 || line[i - 1] != b'\\') {
                in_single_quote = false;
            }
        } else if in_double_quote {
            if ch == b'"' && (i == 0 || line[i - 1] != b'\\') {
                in_double_quote = false;
            }
        } else if ch == b'\'' {
            in_single_quote = true;
        } else if ch == b'"' {
            in_double_quote = true;
        } else if ch == b'/' && i + 1 < len && line[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find the position of an inline `//` comment in a line, skipping `//` inside strings.
/// Returns `None` if no inline comment is found.
fn find_line_comment_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < len {
        let ch = bytes[i];
        if in_single_quote {
            if ch == b'\'' && (i == 0 || bytes[i - 1] != b'\\') {
                in_single_quote = false;
            }
        } else if in_double_quote {
            if ch == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                in_double_quote = false;
            }
        } else if ch == b'\'' {
            in_single_quote = true;
        } else if ch == b'"' {
            in_double_quote = true;
        } else if ch == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Check if the at-rule is the first meaningful content in a block,
/// i.e. it immediately follows `{` (possibly with whitespace/comments).
///
/// All indexing is done on bytes (not chars) to avoid panics on non-ASCII
/// characters in source files.  This is safe because every pattern we search
/// for (`*/`, `/*`, `\n`, `//`, `{`) consists solely of ASCII bytes.
fn is_first_in_block(before: &str) -> bool {
    let bytes = before.as_bytes();
    let mut pos = bytes.len();

    loop {
        // Skip trailing whitespace
        while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t' | b'\n' | b'\r') {
            pos -= 1;
        }
        if pos == 0 {
            // Beginning of file — treat as first-nested
            return true;
        }
        // Check for end of a block comment `*/`
        if pos >= 2 && bytes[pos - 2] == b'*' && bytes[pos - 1] == b'/' {
            // Find the matching `/*` by scanning bytes backward.
            let search_end = pos.saturating_sub(2);
            let open = (0..search_end)
                .rev()
                .find(|&i| bytes[i] == b'/' && bytes[i + 1] == b'*');
            if let Some(o) = open {
                pos = o;
                continue;
            }
            return false;
        }
        // Find start of current line (byte-safe)
        let line_start = bytes[..pos]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line_bytes = &bytes[line_start..pos];
        // Trim leading whitespace
        let trimmed_offset = line_bytes
            .iter()
            .position(|&b| b != b' ' && b != b'\t')
            .unwrap_or(line_bytes.len());
        let trimmed = &line_bytes[trimmed_offset..];

        if trimmed.starts_with(b"//") {
            // Entire line is a comment — skip past it
            pos = line_start;
            continue;
        }
        // Check for inline `//` comment on the same line as code
        if let Some(comment_pos) = find_line_comment_start_bytes(line_bytes) {
            pos = line_start + comment_pos;
            // Re-skip whitespace before the comment
            while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t') {
                pos -= 1;
            }
            if pos == 0 {
                return true;
            }
        }
        return bytes[pos - 1] == b'{';
    }
}

/// Check if the text immediately before an at-rule ends with a comment line.
fn is_after_comment_source(before: &str) -> bool {
    // Find the line before the at-rule's line
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    // Look at lines in reverse
    for line in trimmed.lines().rev() {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        if stripped.ends_with("*/") || stripped.starts_with("//") {
            return true;
        }
        return false;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{AtRule as ParserAtRule, Span as ParserSpan, StyleRule, Syntax};

    fn make_ctx(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options: None,
        }
    }

    #[test]
    fn reports_missing_empty_line_before_at_rule() {
        let src = "a { color: red; }\n@media screen { }";
        let at_offset = src.find("@media").unwrap();
        let nodes = vec![
            CssNode::Style(StyleRule {
                selector: "a".to_string(),
                declarations: vec![],
                span: ParserSpan::new(0, 17),
                ..Default::default()
            }),
            CssNode::AtRule(ParserAtRule {
                name: "media".to_string(),
                params: "screen".to_string(),
                span: ParserSpan::new(at_offset, 18),
                children: vec![],
            }),
        ];
        let d = AtRuleEmptyLineBefore.check_root(&nodes, &make_ctx(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Expected empty line before at-rule"));
    }

    #[test]
    fn allows_empty_line_before_at_rule() {
        let src = "a { color: red; }\n\n@media screen { }";
        let at_offset = src.find("@media").unwrap();
        let nodes = vec![
            CssNode::Style(StyleRule {
                selector: "a".to_string(),
                declarations: vec![],
                span: ParserSpan::new(0, 17),
                ..Default::default()
            }),
            CssNode::AtRule(ParserAtRule {
                name: "media".to_string(),
                params: "screen".to_string(),
                span: ParserSpan::new(at_offset, 18),
                children: vec![],
            }),
        ];
        let d = AtRuleEmptyLineBefore.check_root(&nodes, &make_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn skips_first_nested_at_rule() {
        let src = "@media screen { }";
        let nodes = vec![CssNode::AtRule(ParserAtRule {
            name: "media".to_string(),
            params: "screen".to_string(),
            span: ParserSpan::new(0, src.len()),
            children: vec![],
        })];
        let d = AtRuleEmptyLineBefore.check_root(&nodes, &make_ctx(src));
        assert!(d.is_empty());
    }

    #[test]
    fn allows_grouped_imports_with_except_blockless() {
        let src = "@import \"a.css\";\n@import \"b.css\";";
        let second_offset = src.rfind("@import").unwrap();
        let opts =
            serde_json::json!(["always", { "except": ["blockless-after-same-name-blockless"] }]);
        let ctx = RuleContext {
            file_path: "t.css",
            source: src,
            syntax: Syntax::Css,
            options: Some(&opts),
        };
        let nodes = vec![
            CssNode::AtRule(ParserAtRule {
                name: "import".to_string(),
                params: "\"a.css\"".to_string(),
                span: ParserSpan::new(0, 16),
                children: vec![],
            }),
            CssNode::AtRule(ParserAtRule {
                name: "import".to_string(),
                params: "\"b.css\"".to_string(),
                span: ParserSpan::new(second_offset, 16),
                children: vec![],
            }),
        ];
        let d = AtRuleEmptyLineBefore.check_root(&nodes, &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn ignore_at_rules() {
        let src = "a { color: red; }\n@else { }";
        let at_offset = src.find("@else").unwrap();
        let opts = serde_json::json!(["always", { "ignoreAtRules": ["else"] }]);
        let ctx = RuleContext {
            file_path: "t.css",
            source: src,
            syntax: Syntax::Css,
            options: Some(&opts),
        };
        let nodes = vec![
            CssNode::Style(StyleRule {
                selector: "a".to_string(),
                declarations: vec![],
                span: ParserSpan::new(0, 17),
                ..Default::default()
            }),
            CssNode::AtRule(ParserAtRule {
                name: "else".to_string(),
                params: "".to_string(),
                span: ParserSpan::new(at_offset, 9),
                children: vec![],
            }),
        ];
        let d = AtRuleEmptyLineBefore.check_root(&nodes, &ctx);
        assert!(d.is_empty());
    }
}
