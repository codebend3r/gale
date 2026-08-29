use gale_css_parser::{CssNode, Declaration};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports numbers with more than 4 decimal places.
///
/// Equivalent to Stylelint's `number-max-precision` rule (default: 4).
pub struct NumberMaxPrecision;

const MAX_PRECISION: usize = 4;

impl Rule for NumberMaxPrecision {
    fn name(&self) -> &'static str {
        "number-max-precision"
    }

    fn description(&self) -> &'static str {
        "Limit the number of decimal places allowed in numbers"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        let CssNode::Style(rule) = node else {
            return vec![];
        };

        // The primary option is a number, and it may arrive bare (`4`) or in
        // Stylelint's array form (`[4, { ... }]`) — `primary_option` unwraps both.
        let max = ctx
            .primary_option()
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(MAX_PRECISION);

        let mut diags = Vec::new();
        for decl in &rule.declarations {
            // Scan the raw source rather than `decl.value`: lightningcss
            // normalises numbers when it prints a value (1.123456789 becomes
            // 1.12346), which would both misreport the literal and, because the
            // rounded text is not findable in the source, collapse the column
            // onto the start of the declaration.
            let (text, base) = match decl_value_source(ctx.source, decl) {
                Some((text, base)) => (text, Some(base)),
                // No usable span (synthetic nodes in unit tests, unusual
                // parses): fall back to the printed value.
                None => (decl.value.as_str(), None),
            };

            let Some(found) = find_precision_issue(text, max) else {
                continue;
            };

            let span = match base {
                Some(base) => Span::new(base + found.offset, found.original.len()),
                None => Span::new(decl.span.offset, found.original.len()),
            };

            let (original, rounded) = (found.original, found.rounded);
            diags.push(
                Diagnostic::new(
                    self.name(),
                    format!("Expected \"{original}\" to be \"{rounded}\""),
                )
                .severity(self.default_severity())
                .span(span),
            );
        }
        diags
    }
}

/// Locate the value portion of a declaration inside the original source.
///
/// Returns the value text and its absolute byte offset, or `None` when the
/// declaration's span does not map onto `source` (zero-length spans, synthetic
/// nodes, offsets past the end).
fn decl_value_source<'a>(source: &'a str, decl: &Declaration) -> Option<(&'a str, usize)> {
    let start = decl.span.offset;
    let end = start.checked_add(decl.span.length)?;
    if decl.span.length == 0
        || end > source.len()
        || !source.is_char_boundary(start)
        || !source.is_char_boundary(end)
    {
        return None;
    }

    let slice = &source[start..end];
    // The value begins after the property/value separator.
    let rel = slice.find(':')? + 1;
    Some((&slice[rel..], start + rel))
}

/// A number in a value that carries more decimal places than allowed.
struct PrecisionIssue {
    /// The number exactly as written in the scanned text.
    original: String,
    /// The same number rounded to the configured precision.
    rounded: String,
    /// Byte offset of the number within the scanned text.
    offset: usize,
}

/// Find the first number in `value` that exceeds `max` decimal places.
///
/// Scans bytes rather than chars so the returned offset can be used directly as
/// a source position. This is safe for non-ASCII input because no byte of a
/// multi-byte UTF-8 sequence can equal an ASCII `.` or digit.
fn find_precision_issue(value: &str, max: usize) -> Option<PrecisionIssue> {
    let bytes = value.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        if bytes[i] != b'.' {
            continue;
        }

        // Walk back over the integer part.
        let mut num_start = i;
        while num_start > 0 && bytes[num_start - 1].is_ascii_digit() {
            num_start -= 1;
        }

        // Walk forward over the fractional part.
        let mut j = i + 1;
        let mut decimal_digits = 0;
        while j < len && bytes[j].is_ascii_digit() {
            decimal_digits += 1;
            j += 1;
        }

        if decimal_digits > max {
            let original = value[num_start..j].to_string();
            let rounded = round_to_precision(&original, max);
            return Some(PrecisionIssue {
                original,
                rounded,
                offset: num_start,
            });
        }
    }
    None
}

/// Round a decimal number string to `max` decimal places.
fn round_to_precision(num: &str, max: usize) -> String {
    if let Ok(f) = num.parse::<f64>() {
        let factor = 10f64.powi(max as i32);
        let rounded = (f * factor).round() / factor;
        if max == 0 {
            format!("{}", rounded as i64)
        } else {
            format!("{:.prec$}", rounded, prec = max)
        }
    } else {
        num.to_string()
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

    fn ctx_with(source: &'static str, options: &'static serde_json::Value) -> RuleContext<'static> {
        RuleContext {
            file_path: "t.css",
            source,
            syntax: Syntax::Css,
            options: Some(options),
        }
    }

    fn style_decl(val: &str) -> CssNode {
        CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![Declaration {
                property: "width".to_string(),
                value: val.to_string(),
                span: ParserSpan::new(0, 0),
                important: false,
            }],
            span: ParserSpan::new(0, 0),
            ..Default::default()
        })
    }

    /// A declaration whose span points into `source`.
    fn sourced_decl(source: &str, printed_value: &str) -> CssNode {
        let start = source
            .find("width")
            .expect("test source needs a declaration");
        let end = source.find(';').unwrap_or(source.len());
        CssNode::Style(StyleRule {
            selector: "a".to_string(),
            declarations: vec![Declaration {
                property: "width".to_string(),
                value: printed_value.to_string(),
                span: ParserSpan::new(start, end - start),
                important: false,
            }],
            span: ParserSpan::new(0, source.len()),
            ..Default::default()
        })
    }

    #[test]
    fn reports_excess_precision() {
        let d = NumberMaxPrecision.check(&style_decl("0.12345em"), &ctx());
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_within_precision() {
        assert!(
            NumberMaxPrecision
                .check(&style_decl("0.1234em"), &ctx())
                .is_empty()
        );
        assert!(
            NumberMaxPrecision
                .check(&style_decl("10px"), &ctx())
                .is_empty()
        );
    }

    #[test]
    fn reports_excess_in_multiple_values() {
        let d = NumberMaxPrecision.check(&style_decl("0.12345 0.6789"), &ctx());
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn honours_array_form_primary_option() {
        // `[2, { ... }]` is Stylelint's standard shape; the max must be read
        // from element 0, not from the array itself.
        static OPTS: std::sync::LazyLock<serde_json::Value> =
            std::sync::LazyLock::new(|| serde_json::json!([2, {}]));
        let source = "a { width: 1.234px; }";
        let d =
            NumberMaxPrecision.check(&sourced_decl(source, "1.234px"), &ctx_with(source, &OPTS));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("to be \"1.23\""), "{}", d[0].message);
    }

    #[test]
    fn array_form_can_disable_by_raising_max() {
        static OPTS: std::sync::LazyLock<serde_json::Value> =
            std::sync::LazyLock::new(|| serde_json::json!([6, {}]));
        let source = "a { width: 1.234px; }";
        assert!(
            NumberMaxPrecision
                .check(&sourced_decl(source, "1.234px"), &ctx_with(source, &OPTS))
                .is_empty()
        );
    }

    #[test]
    fn reports_the_source_literal_not_the_printed_value() {
        // lightningcss rounds long literals when printing a value; the warning
        // must still quote what the author wrote, at the author's column.
        static OPTS: std::sync::LazyLock<serde_json::Value> =
            std::sync::LazyLock::new(|| serde_json::json!(2));
        let source = "a { width: 1.123456789px; }";
        let d = NumberMaxPrecision.check(
            // What lightningcss hands us is already rounded to 1.12346.
            &sourced_decl(source, "1.12346px"),
            &ctx_with(source, &OPTS),
        );
        assert_eq!(d.len(), 1);
        assert!(
            d[0].message.contains("\"1.123456789\""),
            "should quote the source literal, got: {}",
            d[0].message
        );
        assert_eq!(
            d[0].span.offset,
            source.find("1.123456789").unwrap(),
            "should point at the number, not the declaration start"
        );
    }
}
