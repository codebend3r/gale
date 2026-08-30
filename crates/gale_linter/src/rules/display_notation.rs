use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require specific display notation (short or long form).
///
/// Equivalent to Stylelint's `declaration-property-value-disallowed-list` for display,
/// or a dedicated `display-notation` rule.
///
/// In "short" mode (default): flags multi-keyword display values when a short
/// equivalent exists. In "long" mode: flags short values when a long equivalent exists.
///
/// Mappings:
/// - `block flow` <-> `block`
/// - `inline flow` <-> `inline`
/// - `run-in flow` <-> `run-in`
/// - `block flex` <-> `flex`
/// - `block grid` <-> `grid`
/// - `inline flex` <-> `inline-flex`
/// - `inline grid` <-> `inline-grid`
/// - `block flow-root` <-> `flow-root`
/// - `inline flow-root` <-> `inline-block`
/// - `block table` <-> `table`
/// - `inline table` <-> `inline-table`
pub struct DisplayNotation;

/// (multi-keyword form, single-keyword form) pairs.
const DISPLAY_MAPPINGS: &[(&str, &str)] = &[
    ("block flow", "block"),
    ("block flex", "flex"),
    ("block flow-root", "flow-root"),
    ("block grid", "grid"),
    ("block ruby", "ruby"),
    ("block table", "table"),
    ("inline flow", "inline"),
    ("inline flex", "inline-flex"),
    ("inline flow-root", "inline-block"),
    ("inline grid", "inline-grid"),
    ("inline table", "inline-table"),
    ("run-in flow", "run-in"),
];

impl Rule for DisplayNotation {
    fn name(&self) -> &'static str {
        "display-notation"
    }

    fn description(&self) -> &'static str {
        "Specify short or long form for display values"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Style(rule) = node else {
            return vec![];
        };

        let mode = ctx
            .primary_option_str()
            .or_else(|| ctx.options.and_then(|v| v.as_str()))
            .unwrap_or("single-keyword");

        let mut diags = Vec::new();
        for decl in &rule.declarations {
            if !decl.property.eq_ignore_ascii_case("display") {
                continue;
            }

            let value = decl.value.trim().to_ascii_lowercase();

            match mode {
                "single-keyword" | "short" => {
                    // Flag multi-keyword forms that have a single-keyword equivalent
                    for &(multi, single) in DISPLAY_MAPPINGS {
                        if value == multi {
                            diags.push(
                                Diagnostic::new(
                                    self.name(),
                                    format!("Expected \"{single}\" instead of \"{multi}\""),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(decl.span.offset, decl.span.length)),
                            );
                            break;
                        }
                    }
                }
                "multi-keyword" | "long" => {
                    // Flag single-keyword forms that have a multi-keyword equivalent
                    for &(multi, single) in DISPLAY_MAPPINGS {
                        if value == single {
                            diags.push(
                                Diagnostic::new(
                                    self.name(),
                                    format!("Expected \"{multi}\" instead of \"{single}\""),
                                )
                                .severity(self.default_severity())
                                .span(Span::new(decl.span.offset, decl.span.length)),
                            );
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        diags
    }
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

    fn style_with_decl(prop: &str, val: &str) -> CssNode {
        CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![Declaration {
                property: prop.to_string(),
                value: val.to_string(),
                span: ParserSpan::new(0, prop.len() + val.len() + 2),
                important: false,
            }],
            span: ParserSpan::new(0, 0),
            ..Default::default()
        })
    }

    #[test]
    fn single_keyword_mode_flags_multi_keyword_form() {
        // Default mode is "single-keyword"
        let d = DisplayNotation.check(&style_with_decl("display", "block flow"), &ctx());
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("\"block\""));
    }

    #[test]
    fn single_keyword_mode_allows_single_keyword_form() {
        let d = DisplayNotation.check(&style_with_decl("display", "block"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn multi_keyword_mode_flags_single_keyword_form() {
        let ctx = ctx_with_options(serde_json::json!("multi-keyword"));
        let d = DisplayNotation.check(&style_with_decl("display", "flex"), &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("\"block flex\""));
    }

    #[test]
    fn multi_keyword_mode_allows_multi_keyword_form() {
        let ctx = ctx_with_options(serde_json::json!("multi-keyword"));
        let d = DisplayNotation.check(&style_with_decl("display", "block flex"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_display_properties() {
        let d = DisplayNotation.check(&style_with_decl("color", "block flow"), &ctx());
        assert!(d.is_empty());
    }

    #[test]
    fn inline_block_mapping() {
        let ctx = ctx_with_options(serde_json::json!("multi-keyword"));
        let d = DisplayNotation.check(&style_with_decl("display", "inline-block"), &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("\"inline flow-root\""));
    }

    #[test]
    fn ruby_mapping() {
        let ctx = ctx_with_options(serde_json::json!("multi-keyword"));
        let d = DisplayNotation.check(&style_with_decl("display", "ruby"), &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("\"block ruby\""));
    }

    #[test]
    fn no_equivalent_values_are_skipped() {
        // Values like list-item, contents, none have no multi-keyword equivalent
        let ctx = ctx_with_options(serde_json::json!("multi-keyword"));
        assert!(
            DisplayNotation
                .check(&style_with_decl("display", "list-item"), &ctx)
                .is_empty()
        );
        assert!(
            DisplayNotation
                .check(&style_with_decl("display", "contents"), &ctx)
                .is_empty()
        );
        assert!(
            DisplayNotation
                .check(&style_with_decl("display", "none"), &ctx)
                .is_empty()
        );
    }

    #[test]
    fn inline_ruby_has_no_single_keyword_equivalent() {
        // "inline ruby" has no single-keyword equivalent, so single-keyword mode should not flag it
        let d = DisplayNotation.check(&style_with_decl("display", "inline ruby"), &ctx());
        assert!(d.is_empty(), "inline ruby has no single-keyword equivalent");
    }

    #[test]
    fn backward_compat_short_long_options() {
        // "short" and "long" should still work as aliases
        let ctx = ctx_with_options(serde_json::json!("short"));
        let d = DisplayNotation.check(&style_with_decl("display", "block flow"), &ctx);
        assert_eq!(d.len(), 1);

        let ctx = ctx_with_options(serde_json::json!("long"));
        let d = DisplayNotation.check(&style_with_decl("display", "flex"), &ctx);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn rule_name_is_correct() {
        assert_eq!(DisplayNotation.name(), "display-notation");
    }
}
