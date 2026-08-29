use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of attribute selectors in a selector.
///
/// Equivalent to Stylelint's `selector-max-attribute` rule.
/// Default maximum: 1.
pub struct SelectorMaxAttribute;

const DEFAULT_MAX: usize = 1;

impl Rule for SelectorMaxAttribute {
    fn name(&self) -> &'static str {
        "selector-max-attribute"
    }

    fn description(&self) -> &'static str {
        "Limit the number of attribute selectors in a selector"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Style(rule) = node else {
            return vec![];
        };

        let max = ctx
            .options
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_MAX);

        let mut diags = Vec::new();

        // Check each comma-separated selector individually
        for sel in rule.selector.split(',') {
            let sel = sel.trim();
            if sel.is_empty() {
                continue;
            }
            let count = count_attribute_selectors(sel);
            if count > max {
                diags.push(
                    Diagnostic::new(
                        self.name(),
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} attribute selector(s), found {count}",
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

/// Count attribute selectors `[...]` in a selector string.
/// Skips `[` inside quoted strings.
/// Counts attribute selectors inside pseudo-class function arguments
/// (`:not()`, `:is()`, `:where()`, `:has()`, etc.) — matching Stylelint v17
/// behavior where selectors are evaluated as-written.
fn count_attribute_selectors(selector: &str) -> usize {
    let mut count = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = selector.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '[' if !in_single_quote && !in_double_quote => count += 1,
            '\\' => {
                // skip escaped char
                chars.next();
            }
            _ => {}
        }
    }

    count
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
    fn reports_too_many_attribute_selectors() {
        let d = SelectorMaxAttribute.check(&style_with_selector("[type='text'][disabled]"), &ctx());
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("found 2"));
    }

    #[test]
    fn allows_within_limit() {
        let d = SelectorMaxAttribute.check(&style_with_selector("input[type='text']"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn allows_no_attributes() {
        let d = SelectorMaxAttribute.check(&style_with_selector("div.class"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn checks_each_selector_in_list_separately() {
        // Each individual selector has 1 attribute, which is within the default max of 1
        let d =
            SelectorMaxAttribute.check(&style_with_selector("[type='text'], [disabled]"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn counts_attributes_inside_not() {
        // Stylelint v17: attribute selectors inside :not() ARE counted (as-written).
        // `[list]` + 5 inside :not() = 6 total attribute selectors.
        let options = serde_json::json!(2);
        let ctx = RuleContext {
            file_path: "t.css",
            source: "",
            syntax: Syntax::Css,
            options: Some(&options),
        };
        let d = SelectorMaxAttribute.check(
            &style_with_selector(
                "[list]:not([type=\"date\"]):not([type=\"datetime-local\"]):not([type=\"month\"]):not([type=\"week\"]):not([type=\"time\"])"
            ),
            &ctx,
        );
        assert_eq!(
            d.len(),
            1,
            "expected 1 diagnostic for 6 attributes with max 2"
        );
        assert!(d[0].message.contains("found 6"));
    }

    #[test]
    fn counts_attributes_inside_has() {
        // Stylelint v17: attribute selectors inside :has() ARE counted.
        // `.foo:has([disabled][required])` has 2 attribute selectors.
        let d = SelectorMaxAttribute.check(
            &style_with_selector(".foo:has([disabled][required])"),
            &ctx(),
        );
        assert_eq!(
            d.len(),
            1,
            "expected 1 diagnostic for 2 attributes with max 1"
        );
        assert!(d[0].message.contains("found 2"));
    }

    #[test]
    fn respects_custom_max() {
        let options = serde_json::json!(2);
        let ctx = RuleContext {
            file_path: "t.css",
            source: "",
            syntax: Syntax::Css,
            options: Some(&options),
        };
        let d = SelectorMaxAttribute.check(&style_with_selector("[type='text'][disabled]"), &ctx);
        assert!(d.is_empty());
    }
}
