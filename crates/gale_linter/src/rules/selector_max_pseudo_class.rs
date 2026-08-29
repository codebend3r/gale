use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Limit the number of pseudo-classes in a selector.
///
/// Equivalent to Stylelint's `selector-max-pseudo-class` rule.
/// Default maximum: 1.
pub struct SelectorMaxPseudoClass;

const DEFAULT_MAX: usize = 1;

impl Rule for SelectorMaxPseudoClass {
    fn name(&self) -> &'static str {
        "selector-max-pseudo-class"
    }

    fn description(&self) -> &'static str {
        "Limit the number of pseudo-classes in a selector"
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
        for sel in rule.selector.split(',') {
            let sel = sel.trim();
            let count = count_pseudo_classes(sel);
            if count > max {
                diags.push(
                    Diagnostic::new(
                        self.name(),
                        format!(
                            "Expected selector \"{sel}\" to have no more than {max} pseudo-class(es), found {count}"
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

/// Count pseudo-classes in a selector (:name patterns, excluding ::pseudo-elements).
/// Counts pseudo-classes inside functional pseudo-class arguments (`:not()`,
/// `:is()`, `:where()`, `:has()`, etc.) — matching Stylelint v17 behavior
/// where selectors are evaluated as-written.
fn count_pseudo_classes(selector: &str) -> usize {
    let mut count = 0usize;
    let chars: Vec<char> = selector.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip attribute selectors
        if chars[i] == '[' {
            let mut depth = 1;
            i += 1;
            while i < len && depth > 0 {
                match chars[i] {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    '"' | '\'' => {
                        let quote = chars[i];
                        i += 1;
                        while i < len && chars[i] != quote {
                            if chars[i] == '\\' {
                                i += 1;
                            }
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        // Skip pseudo-elements (::name)
        if i + 1 < len && chars[i] == ':' && chars[i + 1] == ':' {
            i += 2;
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
                i += 1;
            }
            // Skip parenthesized argument of pseudo-element (e.g. ::slotted(...))
            // but do NOT recurse into it for pseudo-class counting
            if i < len && chars[i] == '(' {
                let mut depth = 1;
                i += 1;
                while i < len && depth > 0 {
                    if chars[i] == '(' {
                        depth += 1;
                    } else if chars[i] == ')' {
                        depth -= 1;
                    }
                    i += 1;
                }
            }
            continue;
        }
        if chars[i] == ':' {
            i += 1;
            let start = i;
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
                i += 1;
            }
            if i > start {
                count += 1;
            }
            // Do NOT skip parenthesized arguments — continue scanning so that
            // pseudo-classes inside (e.g. `:not(:hover)`) are also counted.
        } else {
            i += 1;
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

    fn ctx_with_options(options: serde_json::Value) -> RuleContext<'static> {
        RuleContext {
            file_path: "t.css",
            source: "",
            syntax: Syntax::Css,
            options: Some(Box::leak(Box::new(options))),
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
            span: ParserSpan::new(0, sel.len()),
            ..Default::default()
        })
    }

    #[test]
    fn reports_too_many_pseudo_classes() {
        // 2 pseudo-classes: :hover:focus
        let d = SelectorMaxPseudoClass.check(&style_with_selector("a:hover:focus"), &ctx());
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("found 2"));
    }

    #[test]
    fn allows_within_default_limit() {
        let d = SelectorMaxPseudoClass.check(&style_with_selector("a:hover"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn respects_configured_max() {
        let ctx = ctx_with_options(serde_json::json!(2));
        let d = SelectorMaxPseudoClass.check(&style_with_selector("a:hover:focus"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn does_not_count_pseudo_elements() {
        let d = SelectorMaxPseudoClass.check(&style_with_selector("a::before:hover"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn rule_name_is_correct() {
        assert_eq!(SelectorMaxPseudoClass.name(), "selector-max-pseudo-class");
    }

    #[test]
    fn counts_pseudo_classes_inside_not() {
        // Stylelint v17: `:not(:hover)` counts as 2 pseudo-classes (`:not` and `:hover`).
        let d = SelectorMaxPseudoClass.check(&style_with_selector("a:not(:hover)"), &ctx());
        assert_eq!(
            d.len(),
            1,
            "expected 1 diagnostic for 2 pseudo-classes with max 1"
        );
        assert!(d[0].message.contains("found 2"));
    }

    #[test]
    fn counts_pseudo_classes_inside_is() {
        // Stylelint v17: `a:is(:hover)` counts as 2 pseudo-classes (`:is` and `:hover`).
        let d = SelectorMaxPseudoClass.check(&style_with_selector("a:is(:hover)"), &ctx());
        assert_eq!(
            d.len(),
            1,
            "expected 1 diagnostic for 2 pseudo-classes with max 1"
        );
        assert!(d[0].message.contains("found 2"));
    }

    #[test]
    fn allows_pseudo_classes_inside_functional_within_limit() {
        // `:not(:hover)` = 2 pseudo-classes, max 2 should be OK
        let ctx = ctx_with_options(serde_json::json!(2));
        let d = SelectorMaxPseudoClass.check(&style_with_selector("a:not(:hover)"), &ctx);
        assert!(d.is_empty());
    }
}
