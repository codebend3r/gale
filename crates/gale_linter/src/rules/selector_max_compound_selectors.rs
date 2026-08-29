use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of compound selectors in a selector.
///
/// Equivalent to Stylelint's `selector-max-compound-selectors` rule.
/// Default maximum: 3. Counts parts separated by combinators (space, `>`, `+`, `~`).
pub struct SelectorMaxCompoundSelectors;

const MAX_COMPOUND: usize = 3;

impl Rule for SelectorMaxCompoundSelectors {
    fn name(&self) -> &'static str {
        "selector-max-compound-selectors"
    }

    fn description(&self) -> &'static str {
        "Limit the number of compound selectors in a selector"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Style(rule) = node else {
            return vec![];
        };

        // Read configured max from options (primary option is a number).
        let max = ctx
            .options
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(MAX_COMPOUND);

        let selector = if matches!(
            ctx.syntax,
            gale_css_parser::Syntax::Scss
                | gale_css_parser::Syntax::Sass
                | gale_css_parser::Syntax::Less
        ) {
            strip_scss_line_comments(&rule.selector)
        } else {
            rule.selector.clone()
        };

        let mut diags = Vec::new();
        // Check each comma-separated selector
        for sel in selector.split(',') {
            let sel = sel.trim();
            let count = count_compound_selectors(sel);
            if count > max {
                diags.push(
                    Diagnostic::new(
                        self.name(),
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} compound selector(s), found {count}"
                        ),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(rule.span.offset, rule.span.length)),
                );
            }
        }
        diags
    }
}

/// Count compound selectors by splitting on combinators (whitespace, `>`, `+`, `~`).
/// Only skips content inside attribute selectors `[...]`, NOT inside pseudo-class
/// function arguments — matching Stylelint v17 behavior where selectors are
/// evaluated as-written.
fn count_compound_selectors(selector: &str) -> usize {
    if selector.is_empty() {
        return 0;
    }

    let mut count = 1usize;
    let chars: Vec<char> = selector.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_brackets = 0i32; // only tracks [ ]

    while i < len {
        // Skip attribute selectors only
        if chars[i] == '[' {
            in_brackets += 1;
            i += 1;
            continue;
        }
        if chars[i] == ']' {
            in_brackets -= 1;
            i += 1;
            continue;
        }

        if in_brackets > 0 {
            i += 1;
            continue;
        }

        // Explicit combinators
        if chars[i] == '>' || chars[i] == '+' || chars[i] == '~' {
            count += 1;
            i += 1;
            // Skip following whitespace
            while i < len && chars[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }

        // Descendant combinator (whitespace between simple selectors)
        if chars[i].is_ascii_whitespace() {
            // Skip all whitespace
            while i < len && chars[i].is_ascii_whitespace() {
                i += 1;
            }
            // If not at end and next char is not a combinator or closing paren, it's a descendant combinator
            if i < len && chars[i] != '>' && chars[i] != '+' && chars[i] != '~' && chars[i] != ')' {
                count += 1;
            }
            continue;
        }

        i += 1;
    }

    count
}

/// Strip `//` line comments from a selector string (SCSS/Less).
///
/// Handles both full-line comments (`// comment`) and inline comments
/// at the end of a line (e.g., `dt, // TODO: Deprecate`).
/// Also strips lines that become empty after comment removal.
fn strip_scss_line_comments(selector: &str) -> String {
    selector
        .lines()
        .map(|line| {
            // Find `//` that is not inside a string or url()
            if let Some(pos) = find_line_comment(line) {
                &line[..pos]
            } else {
                line
            }
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Find the byte position of a `//` line comment in a SCSS line,
/// ignoring `//` inside strings or `url()`.
fn find_line_comment(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        match bytes[i] {
            // Skip single- and double-quoted strings
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < len && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1; // skip escaped char
                    }
                    i += 1;
                }
                i += 1; // skip closing quote
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                return Some(i);
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

    fn ctx() -> RuleContext<'static> {
        RuleContext {
            file_path: "t.css",
            source: "",
            syntax: Syntax::Css,
            options: None,
        }
    }

    fn style_with_selector(sel: &str) -> CssNode {
        CssNode::Style(StyleRule {
            selector: sel.to_string(),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: "red".to_string(),
                span: ParserSpan::new(0, 0),
                important: false,
            }],
            span: ParserSpan::new(0, 0),
            ..Default::default()
        })
    }

    #[test]
    fn reports_too_many_compound_selectors() {
        // 4 compound selectors: .a .b .c .d
        let d = SelectorMaxCompoundSelectors.check(&style_with_selector(".a .b .c .d"), &ctx());
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("found 4"));
    }

    #[test]
    fn allows_within_limit() {
        // 3 compound selectors: .a .b .c
        let d = SelectorMaxCompoundSelectors.check(&style_with_selector(".a .b .c"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn counts_child_combinator() {
        // 4 compound selectors: .a > .b > .c > .d
        let d =
            SelectorMaxCompoundSelectors.check(&style_with_selector(".a > .b > .c > .d"), &ctx());
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn single_selector_ok() {
        let d = SelectorMaxCompoundSelectors.check(&style_with_selector(".foo"), &ctx());
        assert!(d.is_empty());
    }

    fn scss_ctx() -> RuleContext<'static> {
        RuleContext {
            file_path: "t.scss",
            source: "",
            syntax: Syntax::Scss,
            options: None,
        }
    }

    #[test]
    fn strips_inline_scss_comments() {
        // "dt, // TODO: Deprecate\n.form-group-header" should be treated as
        // "dt,\n.form-group-header" which is two selectors each with 1 compound.
        let sel = "dt, // TODO: Deprecate\n.form-group-header";
        let d = SelectorMaxCompoundSelectors.check(&style_with_selector(sel), &scss_ctx());
        assert!(d.is_empty(), "Expected no diagnostics, got: {:?}", d);
    }

    #[test]
    fn strips_full_line_scss_comments() {
        // Full-line SCSS comments should be stripped entirely
        let sel = "// This is a comment\n.foo .bar";
        let d = SelectorMaxCompoundSelectors.check(&style_with_selector(sel), &scss_ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn counts_compounds_inside_is() {
        // Stylelint v17: compound selectors inside :is() ARE counted.
        // `:is(.a .b .c .d)` has 4 compound selectors.
        let d =
            SelectorMaxCompoundSelectors.check(&style_with_selector(":is(.a .b .c .d)"), &ctx());
        assert_eq!(
            d.len(),
            1,
            "expected 1 diagnostic for 4 compounds inside :is()"
        );
        assert!(d[0].message.contains("found 4"));
    }

    #[test]
    fn counts_compounds_inside_has() {
        // Stylelint v17: `.foo:has(.bar > .baz > .qux > .quux)` has compounds
        // counted across the whole selector as-written.
        let d = SelectorMaxCompoundSelectors.check(
            &style_with_selector(".foo:has(.bar > .baz > .qux > .quux)"),
            &ctx(),
        );
        assert_eq!(d.len(), 1);
    }
}
