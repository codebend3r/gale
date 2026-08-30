use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify percentage or number notation for alpha values.
///
/// Equivalent to Stylelint's `alpha-value-notation` rule.
/// Supports primary options: "percentage", "number".
/// Supports secondary options: `exceptProperties`.
pub struct AlphaValueNotation;

impl Rule for AlphaValueNotation {
    fn name(&self) -> &'static str {
        "alpha-value-notation"
    }

    fn description(&self) -> &'static str {
        "Specify percentage or number notation for alpha values"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let opts = Options::from_ctx(ctx);
        let mut diags = Vec::new();

        match node {
            CssNode::Style(rule) => {
                for decl in &rule.declarations {
                    check_declaration(self, decl, &opts, ctx, &mut diags);
                }
            }
            CssNode::Declaration(decl) => {
                check_declaration(self, decl, &opts, ctx, &mut diags);
            }
            _ => {}
        }

        // Deduplicate: keyframe declarations may have overlapping source spans.
        diags.dedup_by(|a, b| a.span == b.span && a.message == b.message);
        diags
    }
}

fn check_declaration(
    rule: &AlphaValueNotation,
    decl: &gale_css_parser::Declaration,
    opts: &Options,
    ctx: &RuleContext,
    diags: &mut Vec<Diagnostic>,
) {
    let prop_lower = decl.property.to_ascii_lowercase();

    // Check if this property is in the exceptProperties list.
    let is_excepted = opts
        .except_properties
        .iter()
        .any(|p| p.to_ascii_lowercase() == prop_lower);

    let effective_primary = if is_excepted {
        match opts.primary {
            PrimaryOption::Percentage => PrimaryOption::Number,
            PrimaryOption::Number => PrimaryOption::Percentage,
        }
    } else {
        opts.primary
    };

    let decl_start = decl.span.offset;
    let decl_end = decl_start + decl.span.length;
    let search_area = if decl_end <= ctx.source.len() && decl_start < decl_end {
        &ctx.source[decl_start..decl_end]
    } else {
        &decl.value
    };

    // Check alpha values in color functions
    for (rel_offset, alpha_value) in find_alpha_issues_in_functions(search_area, effective_primary)
    {
        let abs_offset = if decl_end <= ctx.source.len() && decl_start < decl_end {
            decl_start + rel_offset
        } else {
            decl_start
        };
        let msg = match effective_primary {
            PrimaryOption::Percentage => {
                // Convert decimal to percentage: "0.3" -> "30%"
                let pct = format_as_percentage(&alpha_value);
                format!("Expected \"{alpha_value}\" to be \"{pct}\"")
            }
            PrimaryOption::Number => {
                // Convert percentage to decimal: "30%" -> "0.3"
                let num = format_as_number(&alpha_value);
                format!("Expected \"{alpha_value}\" to be \"{num}\"")
            }
        };
        diags.push(
            Diagnostic::new(rule.name(), msg)
                .severity(rule.default_severity())
                .span(Span::new(abs_offset, 1)),
        );
    }

    // Check alpha-accepting properties (opacity, shape-image-threshold, flood-opacity)
    if is_alpha_property(&prop_lower)
        && let Some(issue_offset) = check_property_alpha(search_area, effective_primary)
    {
        let abs_offset = if decl_end <= ctx.source.len() && decl_start < decl_end {
            decl_start + issue_offset
        } else {
            decl_start
        };
        let msg = match effective_primary {
            PrimaryOption::Percentage => format!(
                "Expected percentage notation for alpha value of \"{}\"",
                decl.property
            ),
            PrimaryOption::Number => format!(
                "Expected number notation for alpha value of \"{}\"",
                decl.property
            ),
        };
        diags.push(
            Diagnostic::new(rule.name(), msg)
                .severity(rule.default_severity())
                .span(Span::new(abs_offset, 1)),
        );
    }
}

/// Properties that accept alpha/opacity values directly.
fn is_alpha_property(prop: &str) -> bool {
    matches!(
        prop,
        "opacity" | "shape-image-threshold" | "flood-opacity" | "fill-opacity" | "stop-opacity"
    )
}

/// Check a property value for alpha notation issues.
/// We must use the source text (search_area) because lightningcss normalizes
/// opacity percentages to numbers (e.g. `opacity: 10%` becomes value `.1`).
/// Returns Some(offset_within_search_area) if there's an issue.
fn check_property_alpha(search_area: &str, expected: PrimaryOption) -> Option<usize> {
    // Extract the value part after the colon from the source text.
    // If there's no colon, the search_area is just the value (fallback path).
    let (after_colon, value_base_offset) = if let Some(colon_pos) = search_area.find(':') {
        (&search_area[colon_pos + 1..], colon_pos + 1)
    } else {
        (search_area, 0)
    };

    // Strip comments
    let cleaned = strip_all_comments(after_colon);
    let trimmed = cleaned.trim().trim_end_matches(';').trim();

    if trimmed.is_empty() {
        return None;
    }

    // Skip if value contains var() or env() or SCSS variables
    if contains_variable(trimmed) {
        return None;
    }

    let value_start = value_base_offset + (after_colon.len() - after_colon.trim_start().len());

    match expected {
        PrimaryOption::Percentage => {
            // We expect percentage — flag if value is a number (not percentage).
            if !trimmed.ends_with('%') && is_decimal_number(trimmed) {
                return Some(value_start);
            }
        }
        PrimaryOption::Number => {
            // We expect number — flag if value is a percentage.
            if trimmed.ends_with('%') {
                return Some(value_start);
            }
        }
    }
    None
}

/// Strip all block comments from a string.
fn strip_all_comments(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Skip until */
            i += 2;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Strip CSS comments from a value string.
fn strip_comments(s: &str) -> &str {
    // Simple: just trim trailing comments for property values.
    if let Some(pos) = s.find("/*") {
        s[..pos].trim()
    } else {
        s.trim()
    }
}

/// Check if a value contains CSS variables, SCSS variables, or Less variables.
fn contains_variable(s: &str) -> bool {
    s.contains("var(") || s.contains("env(") || s.contains('$') || s.contains('@')
}

// ---------------------------------------------------------------------------
// Color function alpha checking
// ---------------------------------------------------------------------------

/// All color functions that can have an alpha channel.
const COLOR_FUNCTIONS: &[&str] = &[
    "rgb", "rgba", "hsl", "hsla", "lch", "oklch", "lab", "oklab", "color", "hwb",
];

/// Find alpha value issues in color function calls.
/// Returns (byte_offset, alpha_value_string) for each issue found.
fn find_alpha_issues_in_functions(value: &str, expected: PrimaryOption) -> Vec<(usize, String)> {
    let mut results = Vec::new();
    let lower = value.to_ascii_lowercase();

    for &func in COLOR_FUNCTIONS {
        let pattern = format!("{func}(");
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(&pattern) {
            let abs_pos = search_from + pos;
            let args_start = abs_pos + pattern.len();
            if let Some(close_offset) = find_matching_paren(value, args_start) {
                let args = &value[args_start..close_offset];

                if let Some((alpha_val, alpha_offset)) = extract_alpha_from_args(args) {
                    let trimmed = alpha_val.trim();

                    // Skip if alpha contains variables
                    if !contains_variable(trimmed) && !trimmed.is_empty() {
                        let should_report = match expected {
                            PrimaryOption::Percentage => {
                                // Expect percentage, flag if number
                                !trimmed.ends_with('%') && is_decimal_number(trimmed)
                            }
                            PrimaryOption::Number => {
                                // Expect number, flag if percentage
                                trimmed.ends_with('%')
                            }
                        };

                        if should_report {
                            results.push((args_start + alpha_offset, trimmed.to_string()));
                        }
                    }
                }
                search_from = close_offset;
            } else {
                search_from = abs_pos + 1;
            }
        }
    }
    results
}

/// Format a decimal alpha value as a percentage string: "0.3" -> "30%"
fn format_as_percentage(value: &str) -> String {
    if let Ok(f) = value.parse::<f64>() {
        let pct = f * 100.0;
        // Round to avoid floating-point noise (e.g., 29.999999 -> 30)
        let rounded = (pct * 1000.0).round() / 1000.0;
        if rounded.fract() == 0.0 {
            format!("{}%", rounded as i64)
        } else {
            // Remove trailing zeros
            let s = format!("{}", rounded);
            format!("{s}%")
        }
    } else {
        format!("{value}%")
    }
}

/// Format a percentage alpha value as a decimal number string: "30%" -> "0.3"
fn format_as_number(value: &str) -> String {
    let trimmed = value.trim_end_matches('%');
    if let Ok(f) = trimmed.parse::<f64>() {
        let num = f / 100.0;
        // Format with minimal decimals
        let s = format!("{}", num);
        s
    } else {
        value.to_string()
    }
}

/// Find the matching closing parenthesis, handling nested parens.
fn find_matching_paren(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 1;
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Extract the alpha value from function arguments.
/// Returns (alpha_str, offset_within_args) if an alpha channel is present.
fn extract_alpha_from_args(args: &str) -> Option<(&str, usize)> {
    // Check for "from" keyword (relative color syntax) — we need to handle
    // nested function calls like `rgb(from rgb(127 127 127 / 25%) 0 0 0 / 50%)`
    // For relative color syntax, we look at the outermost `/` that is not inside parens.
    if args.contains('/') {
        // Find the last `/` that is not inside parentheses
        let mut depth = 0;
        let mut last_slash = None;
        for (i, b) in args.bytes().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'/' if depth == 0 => last_slash = Some(i),
                _ => {}
            }
        }

        if let Some(slash_pos) = last_slash {
            let after_slash = &args[slash_pos + 1..];
            let alpha_trimmed_start = after_slash.len() - after_slash.trim_start().len();
            let alpha_str = strip_trailing_comment(after_slash.trim());
            if !alpha_str.is_empty() {
                return Some((alpha_str, slash_pos + 1 + alpha_trimmed_start));
            }
        }
    } else if args.contains(',') {
        // Legacy syntax: func(r, g, b, a) or func(h, s%, l%, a)
        // Split by commas but only at depth 0
        let mut parts = Vec::new();
        let mut depth = 0;
        let mut start = 0;
        for (i, b) in args.bytes().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b',' if depth == 0 => {
                    parts.push((start, &args[start..i]));
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push((start, &args[start..]));

        if parts.len() == 4 {
            let (offset, alpha_part) = parts[3];
            let trimmed_start = alpha_part.len() - alpha_part.trim_start().len();
            let alpha_str = strip_trailing_comment(alpha_part.trim());
            if !alpha_str.is_empty() {
                return Some((alpha_str, offset + trimmed_start));
            }
        }
    }
    None
}

/// Strip trailing CSS comments from a value.
fn strip_trailing_comment(s: &str) -> &str {
    if let Some(pos) = s.find("/*") {
        s[..pos].trim()
    } else {
        s.trim()
    }
}

fn is_decimal_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut has_digit = false;
    let mut has_dot = false;
    for (i, c) in s.chars().enumerate() {
        if c == '.' {
            if has_dot {
                return false;
            }
            has_dot = true;
        } else if c.is_ascii_digit() {
            has_digit = true;
        } else if c == '-' || c == '+' {
            if i != 0 {
                return false;
            }
        } else {
            return false;
        }
    }
    has_digit
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum PrimaryOption {
    Percentage,
    Number,
}

struct Options {
    primary: PrimaryOption,
    except_properties: Vec<String>,
}

impl Options {
    fn from_ctx(ctx: &RuleContext) -> Self {
        let mut opts = Options {
            primary: PrimaryOption::Percentage,
            except_properties: Vec::new(),
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
                if let Some(secondary) = arr.get(1)
                    && let Some(props) =
                        secondary.get("exceptProperties").and_then(|v| v.as_array())
                {
                    for item in props {
                        if let Some(s) = item.as_str() {
                            opts.except_properties.push(s.to_string());
                        }
                    }
                }
            }
            _ => {}
        }

        opts
    }
}

fn parse_primary(s: &str) -> PrimaryOption {
    match s {
        "number" => PrimaryOption::Number,
        _ => PrimaryOption::Percentage,
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

    fn ctx_with_options(options: &serde_json::Value) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.css",
            source: "",
            syntax: Syntax::Css,
            options: Some(options),
        }
    }

    fn style_with_decl(prop: &str, val: &str) -> CssNode {
        CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![Declaration {
                property: prop.to_string(),
                value: val.to_string(),
                span: ParserSpan::new(0, 0),
                important: false,
            }],
            span: ParserSpan::new(0, 0),
            ..Default::default()
        })
    }

    #[test]
    fn percentage_mode_reports_decimal_alpha_in_rgba() {
        let opts = serde_json::json!(["percentage"]);
        let d = AlphaValueNotation.check(
            &style_with_decl("color", "rgba(0, 0, 0, 0.5)"),
            &ctx_with_options(&opts),
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn percentage_mode_allows_percentage_alpha() {
        let opts = serde_json::json!(["percentage"]);
        let d = AlphaValueNotation.check(
            &style_with_decl("color", "rgba(0, 0, 0, 50%)"),
            &ctx_with_options(&opts),
        );
        assert!(d.is_empty());
    }

    #[test]
    fn number_mode_reports_percentage_alpha() {
        let opts = serde_json::json!(["number"]);
        let d = AlphaValueNotation.check(
            &style_with_decl("color", "rgba(0, 0, 0, 50%)"),
            &ctx_with_options(&opts),
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn number_mode_allows_decimal_alpha() {
        let opts = serde_json::json!(["number"]);
        let d = AlphaValueNotation.check(
            &style_with_decl("color", "rgba(0, 0, 0, 0.5)"),
            &ctx_with_options(&opts),
        );
        assert!(d.is_empty());
    }

    #[test]
    fn number_mode_reports_percentage_opacity() {
        let opts = serde_json::json!(["number"]);
        let d =
            AlphaValueNotation.check(&style_with_decl("opacity", "50%"), &ctx_with_options(&opts));
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn number_mode_allows_decimal_opacity() {
        let opts = serde_json::json!(["number"]);
        let d =
            AlphaValueNotation.check(&style_with_decl("opacity", "0.5"), &ctx_with_options(&opts));
        assert!(d.is_empty());
    }

    #[test]
    fn percentage_mode_reports_decimal_opacity() {
        let opts = serde_json::json!(["percentage"]);
        let d =
            AlphaValueNotation.check(&style_with_decl("opacity", "0.5"), &ctx_with_options(&opts));
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn except_properties_flips_for_opacity() {
        let opts = serde_json::json!(["percentage", {"exceptProperties": ["opacity"]}]);
        // With percentage mode + exceptProperties: ["opacity"],
        // opacity should expect number notation instead.
        let d =
            AlphaValueNotation.check(&style_with_decl("opacity", "0.5"), &ctx_with_options(&opts));
        assert!(d.is_empty()); // 0.5 is number notation, which is expected for excepted property
    }

    #[test]
    fn allows_no_alpha_function() {
        let d = AlphaValueNotation.check(&style_with_decl("color", "rgb(0, 0, 0)"), &ctx());
        assert!(d.is_empty());
    }
}
