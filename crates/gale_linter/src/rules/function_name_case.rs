use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Enforces consistent case for function names.
///
/// Equivalent to Stylelint's `function-name-case` rule.
/// Supports `"lower"` (default) and `"upper"` primary options.
///
/// Secondary options:
/// - `ignoreFunctions`: array of function name strings or regex patterns
///   (e.g. `"/^get.*$/"`) to ignore.
pub struct FunctionNameCase;

/// CSS functions whose canonical name contains uppercase letters (camelCase).
/// When enforcing lowercase, these are expected in their canonical form,
/// not lowercased. When enforcing uppercase, they should be fully uppercased.
const CAMEL_CASE_FUNCTIONS: &[&str] = &[
    "translateX",
    "translateY",
    "translateZ",
    "translate3d",
    "scaleX",
    "scaleY",
    "scaleZ",
    "scale3d",
    "rotateX",
    "rotateY",
    "rotateZ",
    "rotate3d",
    "skewX",
    "skewY",
    "perspective",
];

/// Returns the canonical form of a camelCase CSS function if the name matches
/// (case-insensitive), or `None` if it's not a known camelCase function.
fn camel_case_canonical(name: &str) -> Option<&'static str> {
    CAMEL_CASE_FUNCTIONS
        .iter()
        .find(|&&f| f.eq_ignore_ascii_case(name))
        .copied()
}

/// Check if a string is a Stylelint-style regex pattern like `/pattern/` or `/pattern/i`.
fn parse_regex_pattern(s: &str) -> Option<Regex> {
    if let Some(rest) = s.strip_prefix('/') {
        if let Some(end) = rest.rfind('/') {
            let pattern = &rest[..end];
            let flags = &rest[end + 1..];
            let full_pattern = if flags.contains('i') {
                format!("(?i){pattern}")
            } else {
                pattern.to_string()
            };
            Regex::new(&full_pattern).ok()
        } else {
            None
        }
    } else {
        None
    }
}

/// Returns the expected function name given the case mode.
/// For camelCase CSS functions, lowercase mode uses the canonical form,
/// and uppercase mode uses the fully uppercased form.
fn expected_name(name: &str, mode: &str) -> String {
    if let Some(canonical) = camel_case_canonical(name) {
        match mode {
            "upper" => canonical.to_ascii_uppercase(),
            _ => canonical.to_string(),
        }
    } else {
        match mode {
            "upper" => name.to_ascii_uppercase(),
            _ => name.to_ascii_lowercase(),
        }
    }
}

/// Returns true if the function name should be ignored based on the
/// `ignoreFunctions` option.
fn is_ignored(name: &str, ignore_fns: &[String]) -> bool {
    for pattern in ignore_fns {
        if let Some(re) = parse_regex_pattern(pattern) {
            if re.is_match(name) {
                return true;
            }
        } else {
            // Exact string match (case-sensitive, matching Stylelint behavior)
            if pattern == name {
                return true;
            }
        }
    }
    false
}

/// Strip vendor prefix from a function name, returning (prefix, base_name).
/// E.g. "-webkit-radial-gradient" -> ("-webkit-", "radial-gradient")
/// If no vendor prefix, returns ("", name).
fn strip_vendor_prefix(name: &str) -> (&str, &str) {
    if name.starts_with('-') {
        // Vendor prefixes: -webkit-, -moz-, -ms-, -o-
        // Find the second hyphen which ends the prefix
        if let Some(pos) = name[1..].find('-') {
            let prefix_end = pos + 2; // +1 for the skipped first char, +1 to include the hyphen
            return (&name[..prefix_end], &name[prefix_end..]);
        }
    }
    ("", name)
}

impl Rule for FunctionNameCase {
    fn name(&self) -> &'static str {
        "function-name-case"
    }

    fn description(&self) -> &'static str {
        "Specify lowercase or uppercase for function names"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Style(rule) = node else {
            return vec![];
        };
        let is_scss = matches!(
            ctx.syntax,
            gale_css_parser::Syntax::Scss
                | gale_css_parser::Syntax::Sass
                | gale_css_parser::Syntax::Less
        );

        // Primary option: "lower" (default) or "upper"
        let mode = ctx.primary_option_str().unwrap_or("lower");

        // Secondary options
        let ignore_fns: Vec<String> = ctx
            .secondary_options()
            .and_then(|v| v.get("ignoreFunctions"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let mut diags = Vec::new();
        for decl in &rule.declarations {
            // We need to work from the raw source text, because the parser
            // (lightningcss) normalises function names to lowercase, which
            // destroys the original casing we need to check.
            let decl_start = decl.span.offset;
            let decl_end = decl_start + decl.span.length;
            if decl_end > ctx.source.len() || decl_start >= decl_end {
                continue;
            }

            let raw_value_area = &ctx.source[decl_start..decl_end];

            // In SCSS/Less, skip values with interpolation or SCSS variables,
            // since the actual function name may be dynamic.
            if is_scss && (raw_value_area.contains("#{") || raw_value_area.contains('$')) {
                continue;
            }

            // Find the value part: after the first colon
            let value_start_in_area = match raw_value_area.find(':') {
                Some(pos) => pos + 1,
                None => continue,
            };
            let raw_value = &raw_value_area[value_start_in_area..];
            let value_abs_start = decl_start + value_start_in_area;

            // Extract function calls from the raw source value
            for (func_name, rel_offset) in extract_function_names_with_offsets(raw_value) {
                // Strip vendor prefix for the purpose of case checking
                let (vendor_prefix, base_name) = strip_vendor_prefix(&func_name);

                // Compute the expected form
                let expected_base = expected_name(base_name, mode);
                let expected_vendor = match mode {
                    "upper" => vendor_prefix.to_ascii_uppercase(),
                    _ => vendor_prefix.to_ascii_lowercase(),
                };
                let expected_full = format!("{expected_vendor}{expected_base}");

                if func_name == expected_full {
                    // Already in the expected case
                    continue;
                }

                // Check ignoreFunctions against the original function name
                if is_ignored(&func_name, &ignore_fns) {
                    continue;
                }

                // The absolute byte offset for the function name in the source
                let abs_offset = value_abs_start + rel_offset;

                let fix = Fix::new(
                    format!("Change function name to \"{expected_full}\""),
                    vec![Edit::new(
                        Span::new(abs_offset, func_name.len()),
                        &expected_full,
                    )],
                );

                let diag = Diagnostic::new(
                    self.name(),
                    format!("Expected \"{func_name}\" to be \"{expected_full}\""),
                )
                .severity(self.default_severity())
                .span(Span::new(abs_offset, func_name.len()))
                .fix(fix);

                diags.push(diag);
            }
        }
        diags
    }
}

/// Extract function names from a CSS value string, along with their byte
/// offsets within the string.
fn extract_function_names_with_offsets(value: &str) -> Vec<(String, usize)> {
    let mut results = Vec::new();
    let bytes = value.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for the start of an identifier (letter, hyphen for vendor prefix, or underscore)
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'-' || bytes[i] == b'_' {
            let start = i;
            while i < len
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
            {
                i += 1;
            }
            if i < len && bytes[i] == b'(' {
                let name = &value[start..i];
                results.push((name.to_string(), start));
            }
        } else {
            i += 1;
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn make_ctx_with_source_and_options<'a>(
        source: &'a str,
        options: Option<&'a serde_json::Value>,
    ) -> RuleContext<'a> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options,
        }
    }

    fn make_node_from_source(source: &str) -> CssNode {
        // Parse the source to get the correct spans
        let result = gale_css_parser::parse(source, Syntax::Css).unwrap();
        result.nodes.into_iter().next().unwrap()
    }

    #[test]
    fn reports_uppercase_function() {
        let source = "a { color: RGB(0, 0, 0); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        let d = FunctionNameCase.check(&node, &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("rgb"));
    }

    #[test]
    fn allows_lowercase_function() {
        let source = "a { color: rgb(0, 0, 0); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        assert!(FunctionNameCase.check(&node, &ctx).is_empty());
    }

    #[test]
    fn reports_mixed_case_calc() {
        let source = "a { margin: Calc(100% - 20px); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        let d = FunctionNameCase.check(&node, &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("calc"));
    }

    #[test]
    fn allows_camel_case_function_in_canonical_form() {
        let source = "a { transform: translateX(0); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        assert!(FunctionNameCase.check(&node, &ctx).is_empty());
    }

    #[test]
    fn reports_wrong_case_camel_case_function() {
        let source = "a { transform: TranslateX(0); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        let d = FunctionNameCase.check(&node, &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("translateX"));
    }

    #[test]
    fn upper_mode_reports_lowercase() {
        use serde_json::json;
        let opts: serde_json::Value = json!(["upper"]);
        let source = "a { margin: calc(5%); }";
        let ctx = make_ctx_with_source_and_options(source, Some(Box::leak(Box::new(opts))));
        let node = make_node_from_source(source);
        let d = FunctionNameCase.check(&node, &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("CALC"));
    }

    #[test]
    fn upper_mode_allows_uppercase() {
        use serde_json::json;
        let opts: serde_json::Value = json!(["upper"]);
        let source = "a { margin: CALC(5%); }";
        let ctx = make_ctx_with_source_and_options(source, Some(Box::leak(Box::new(opts))));
        let node = make_node_from_source(source);
        assert!(FunctionNameCase.check(&node, &ctx).is_empty());
    }

    #[test]
    fn upper_mode_allows_translatex_uppercase() {
        use serde_json::json;
        let opts: serde_json::Value = json!(["upper"]);
        let source = "a { transform: TRANSLATEX(0); }";
        let ctx = make_ctx_with_source_and_options(source, Some(Box::leak(Box::new(opts))));
        let node = make_node_from_source(source);
        assert!(FunctionNameCase.check(&node, &ctx).is_empty());
    }

    #[test]
    fn ignore_functions_exact_match() {
        use serde_json::json;
        let opts: serde_json::Value = json!(["lower", {"ignoreFunctions": ["someFunction"]}]);
        let source = "a { color: someFunction(); }";
        let ctx = make_ctx_with_source_and_options(source, Some(Box::leak(Box::new(opts))));
        let node = make_node_from_source(source);
        assert!(FunctionNameCase.check(&node, &ctx).is_empty());
    }

    #[test]
    fn ignore_functions_regex_pattern() {
        use serde_json::json;
        let opts: serde_json::Value = json!(["lower", {"ignoreFunctions": ["/^get.*$/"]}]);
        let source = "a { color: getDefaultColor(); }";
        let ctx = make_ctx_with_source_and_options(source, Some(Box::leak(Box::new(opts))));
        let node = make_node_from_source(source);
        assert!(FunctionNameCase.check(&node, &ctx).is_empty());
    }

    #[test]
    fn vendor_prefix_lowercase_mode() {
        let source = "a { background: -webkit-radial-gradient(red, green, blue); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        assert!(FunctionNameCase.check(&node, &ctx).is_empty());
    }

    #[test]
    fn vendor_prefix_uppercase_detected() {
        let source = "a { background: -WEBKIT-radial-gradient(red, green, blue); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        let d = FunctionNameCase.check(&node, &ctx);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn correct_column_for_function_name() {
        let source = "a { padding: Some-function(5px); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        let d = FunctionNameCase.check(&node, &ctx);
        assert_eq!(d.len(), 1);
        // "Some-function" starts at column 14 (1-indexed), byte offset 13
        assert_eq!(d[0].span.offset, 13);
    }

    #[test]
    fn emits_fix_for_uppercase_function() {
        let source = "a { color: RGB(0, 0, 0); }";
        let ctx = make_ctx_with_source_and_options(source, None);
        let node = make_node_from_source(source);
        let d = FunctionNameCase.check(&node, &ctx);
        assert_eq!(d.len(), 1);
        assert!(d[0].fix.is_some());
        let fix = d[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].new_text, "rgb");
    }
}
