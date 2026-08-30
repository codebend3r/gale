use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Only allow specified CSS functions.
///
/// Options: an array of function names (strings) or regex patterns
/// (strings that start and end with `/`). All other functions are flagged.
///
/// Matching is strict: only exact string matches or regex patterns match.
/// Vendor-prefixed functions are NOT implicitly matched by unprefixed names.
///
/// Equivalent to Stylelint's `function-allowed-list` rule.
pub struct FunctionAllowedList;

impl Rule for FunctionAllowedList {
    fn name(&self) -> &'static str {
        "function-allowed-list"
    }

    fn description(&self) -> &'static str {
        "Specify a list of allowed functions"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        // The primary option is an array of allowed function names/patterns.
        // It can arrive as:
        //   - A bare array: ["rgb", "hsl"]
        //   - Nested in options array: [["rgb", "hsl"]]
        //   - Nested with secondary: [["rgb", "hsl"], { ... }]
        let allowed_values: Vec<&str> = match ctx.options {
            Some(serde_json::Value::Array(arr)) => {
                // Check if first element is itself an array (nested format)
                if let Some(serde_json::Value::Array(inner)) = arr.first() {
                    inner.iter().filter_map(|v| v.as_str()).collect()
                } else {
                    // Could be bare array of strings, OR could be ["always", {...}] style
                    // For this rule, primary is always an array of strings
                    arr.iter().filter_map(|v| v.as_str()).collect()
                }
            }
            _ => return vec![],
        };

        if allowed_values.is_empty() {
            return vec![];
        }

        // Separate plain names from regex patterns
        let mut plain_names: Vec<String> = Vec::new();
        let mut regex_patterns: Vec<Regex> = Vec::new();

        for val in &allowed_values {
            if val.starts_with('/') && val.ends_with('/') && val.len() > 2 {
                // It's a regex pattern
                let pattern = &val[1..val.len() - 1];
                if let Ok(re) = Regex::new(pattern) {
                    regex_patterns.push(re);
                }
            } else {
                plain_names.push((*val).to_string());
            }
        }

        let mut diags = Vec::new();

        match node {
            CssNode::Style(rule) => {
                for decl in &rule.declarations {
                    let decl_start = decl.span.offset;
                    let decl_end = decl_start + decl.span.length;
                    let search_area = if decl.span.length > 0 && decl_end <= ctx.source.len() {
                        &ctx.source[decl_start..decl_end]
                    } else {
                        &decl.value
                    };
                    find_disallowed_functions(
                        search_area,
                        decl_start,
                        &plain_names,
                        &regex_patterns,
                        self,
                        &mut diags,
                    );
                }
            }
            CssNode::Declaration(decl) => {
                let decl_start = decl.span.offset;
                let decl_end = decl_start + decl.span.length;
                let search_area = if decl.span.length > 0 && decl_end <= ctx.source.len() {
                    &ctx.source[decl_start..decl_end]
                } else {
                    &decl.value
                };
                find_disallowed_functions(
                    search_area,
                    decl_start,
                    &plain_names,
                    &regex_patterns,
                    self,
                    &mut diags,
                );
            }
            _ => {}
        }

        diags
    }
}

fn is_function_allowed(fname: &str, plain_names: &[String], regex_patterns: &[Regex]) -> bool {
    // Strict matching: exact string match or regex pattern match only.
    // No vendor prefix stripping, no implicit case-insensitive matching.
    if plain_names.iter().any(|n| n == fname) {
        return true;
    }

    for re in regex_patterns {
        if re.is_match(fname) {
            return true;
        }
    }

    false
}

fn find_disallowed_functions(
    value: &str,
    base_offset: usize,
    plain_names: &[String],
    regex_patterns: &[Regex],
    rule: &FunctionAllowedList,
    diags: &mut Vec<Diagnostic>,
) {
    let bytes = value.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' && i > 0 {
            let end = i;
            let mut start = i;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric()
                    || bytes[start - 1] == b'-'
                    || bytes[start - 1] == b'_')
            {
                start -= 1;
            }
            if start < end {
                let fname = &value[start..end];
                if !is_function_allowed(fname, plain_names, regex_patterns) {
                    diags.push(
                        Diagnostic::new(rule.name(), format!("Unexpected function \"{fname}\""))
                            .severity(rule.default_severity())
                            .span(Span::new(base_offset + start, end - start)),
                    );
                }
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

    fn ctx_with_options(options: Option<serde_json::Value>) -> RuleContext<'static> {
        let opts: Option<&'static serde_json::Value> = options.map(|v| &*Box::leak(Box::new(v)));
        RuleContext {
            file_path: "t.css",
            source: "",
            syntax: Syntax::Css,
            options: opts,
        }
    }

    fn style_with_value(val: &str) -> CssNode {
        CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: val.to_string(),
                span: ParserSpan::new(0, val.len()),
                important: false,
            }],
            span: ParserSpan::new(0, 0),
            ..Default::default()
        })
    }

    #[test]
    fn allows_all_when_no_options() {
        let ctx = ctx_with_options(None);
        let d = FunctionAllowedList.check(&style_with_value("rgb(0, 0, 0)"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_listed_function() {
        let ctx = ctx_with_options(Some(serde_json::json!(["rgb", "hsl"])));
        let d = FunctionAllowedList.check(&style_with_value("rgb(0, 0, 0)"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_unlisted_function() {
        let ctx = ctx_with_options(Some(serde_json::json!(["rgb"])));
        let d = FunctionAllowedList.check(&style_with_value("hsl(0, 100%, 50%)"), &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("hsl"));
    }

    #[test]
    fn allows_non_function_values() {
        let ctx = ctx_with_options(Some(serde_json::json!(["rgb"])));
        let d = FunctionAllowedList.check(&style_with_value("red"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn case_sensitive() {
        let ctx = ctx_with_options(Some(serde_json::json!(["rgb"])));
        let d = FunctionAllowedList.check(&style_with_value("RGB(0, 0, 0)"), &ctx);
        // "rgb" does not match "RGB" -- strict matching
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn vendor_prefixed_function_not_matched_by_unprefixed() {
        let ctx = ctx_with_options(Some(serde_json::json!(["calc"])));
        let d = FunctionAllowedList.check(&style_with_value("-webkit-calc(100% - 10px)"), &ctx);
        // "calc" does not match "-webkit-calc" -- strict matching
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn regex_pattern_matching() {
        let ctx = ctx_with_options(Some(serde_json::json!(["/^rgb/"])));
        let d = FunctionAllowedList.check(&style_with_value("rgba(0, 0, 0, 0.5)"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn regex_pattern_rejects_non_matching() {
        let ctx = ctx_with_options(Some(serde_json::json!(["/^rgb/"])));
        let d = FunctionAllowedList.check(&style_with_value("hsl(0, 100%, 50%)"), &ctx);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn nested_array_format() {
        let ctx = ctx_with_options(Some(serde_json::json!([["rgb", "hsl"]])));
        let d = FunctionAllowedList.check(&style_with_value("rgb(0, 0, 0)"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn rule_name_is_correct() {
        assert_eq!(FunctionAllowedList.name(), "function-allowed-list");
    }
}
