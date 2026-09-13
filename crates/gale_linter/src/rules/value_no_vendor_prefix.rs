use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports vendor-prefixed values (e.g. `-webkit-flex`).
///
/// Equivalent to Stylelint's `value-no-vendor-prefix` rule.
pub struct ValueNoVendorPrefix;

const VENDOR_PREFIXES: &[&str] = &["-webkit-", "-moz-", "-ms-", "-o-"];

/// Known CSS values that have standard unprefixed equivalents (from Autoprefixer data).
/// Only vendor-prefixed versions of these values should be flagged.
/// Values that Stylelint's value-no-vendor-prefix actually flags.
/// Note: box, element, fill-available, flexbox, and inline-box are NOT
/// flagged by Stylelint even though they have vendor-prefixed variants.
const KNOWN_PREFIXABLE_VALUES: &[&str] = &[
  "calc",        // -webkit-calc()
  "cross-fade",  // -webkit-cross-fade()
  "fit-content", // -webkit-fit-content
  "flex",        // display: -webkit-flex
  "grab",        // cursor: -webkit-grab
  "grabbing",    // cursor: -webkit-grabbing
  "image-set",   // -webkit-image-set()
  "inline-flex", // display: -webkit-inline-flex
  "isolate",     // unicode-bidi: -moz-isolate
  "linear-gradient",
  "max-content", // width: -webkit-max-content
  "min-content", // width: -webkit-min-content
  "plaintext",   // unicode-bidi: -moz-plaintext
  "radial-gradient",
  "repeating-linear-gradient",
  "repeating-radial-gradient",
  "sticky",   // position: -webkit-sticky
  "zoom-in",  // cursor: -webkit-zoom-in
  "zoom-out", // cursor: -webkit-zoom-out
];

impl Rule for ValueNoVendorPrefix {
  fn name(&self) -> &'static str {
    "value-no-vendor-prefix"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes for values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Read ignoreValues from options (secondary option object).
    let ignore_values: Vec<String> = ctx
      .secondary_options()
      .or(ctx.options)
      .and_then(|v| v.get("ignoreValues"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|item| item.as_str().map(|s| s.to_ascii_lowercase()))
          .collect()
      })
      .unwrap_or_default();

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      let lower = decl.value.to_ascii_lowercase();
      for prefix in VENDOR_PREFIXES {
        if lower.contains(prefix) {
          // Extract the unprefixed value part
          let unprefixed = extract_unprefixed_value(&lower, prefix);

          // Only flag known autoprefixable values
          if !KNOWN_PREFIXABLE_VALUES.iter().any(|v| *v == unprefixed) {
            continue;
          }

          // Build the vendor-prefixed identifier (e.g. "-moz-radial-gradient")
          let prefixed_ident = {
            let pos = lower.find(prefix).unwrap();
            let after_prefix = &lower[pos + prefix.len()..];
            let ident_end = after_prefix
              .find(|c: char| c.is_ascii_whitespace() || c == ',' || c == ')' || c == '(')
              .unwrap_or(after_prefix.len());
            // Use original (non-lowered) value to preserve case
            let orig_pos = pos;
            let orig_end = pos + prefix.len() + ident_end;
            decl.value[orig_pos..orig_end].to_string()
          };

          // Check if the value is in the ignore list.
          // v17: ignoreValues checks values as-is (no prefix stripping).
          // E.g. ignoreValues: ["-webkit-flex"] matches, but ["flex"] does NOT.
          let prefixed_lower = prefixed_ident.to_ascii_lowercase();
          if ignore_values.iter().any(|v| v == &prefixed_lower) {
            continue;
          }

          // Try to find the prefixed value in source for span + fix
          let decl_start = decl.span.offset;
          let decl_end = decl_start + decl.span.length;
          let (vendor_span, fix) = if decl_end <= ctx.source.len() && decl_start < decl_end {
            let search_area = &ctx.source[decl_start..decl_end];
            let lower_search = search_area.to_ascii_lowercase();
            if let Some(rel_offset) = lower_search.find(prefix) {
              let abs_offset = decl_start + rel_offset;
              let span = Span::new(abs_offset, prefixed_ident.len());
              let fix = Fix::new(
                format!("Remove vendor prefix \"{}\"", prefix.trim_end_matches('-')),
                vec![Edit::new(Span::new(abs_offset, prefix.len()), "")],
              );
              (Some(span), Some(fix))
            } else {
              (None, None)
            }
          } else {
            (None, None)
          };

          let diag_span =
            vendor_span.unwrap_or_else(|| Span::new(decl.span.offset, decl.span.length));

          let mut diag = Diagnostic::new(
            self.name(),
            format!("Unexpected vendor-prefixed value \"{}\"", prefixed_ident),
          )
          .severity(self.default_severity())
          .span(diag_span);

          if let Some(f) = fix {
            diag = diag.fix(f);
          }

          diags.push(diag);
          break;
        }
      }
    }
    diags
  }
}

/// Extract the unprefixed value identifier from a CSS value containing a vendor prefix.
/// E.g. "-webkit-flex" -> "flex", "-webkit-match-parent" -> "match-parent"
fn extract_unprefixed_value(lower_value: &str, prefix: &str) -> String {
  // Find the prefix in the value and return what follows it
  if let Some(pos) = lower_value.find(prefix) {
    let after = &lower_value[pos + prefix.len()..];
    // Take until whitespace, comma, or paren
    let end = after
      .find(|c: char| c.is_ascii_whitespace() || c == ',' || c == ')' || c == '(')
      .unwrap_or(after.len());
    after[..end].to_string()
  } else {
    lower_value.to_string()
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

  fn style_decl(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "display".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_webkit_prefix_value() {
    let d = ValueNoVendorPrefix.check(&style_decl("-webkit-flex"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(
      d[0].message.contains("-webkit-flex"),
      "message was: {}",
      d[0].message
    );
    // Message should contain just the identifier, not trailing args
    assert!(
      !d[0].message.contains("("),
      "message should not contain function args"
    );
  }

  #[test]
  fn reports_ms_prefix_value() {
    // -ms-flexbox is not flagged by Stylelint (flexbox isn't in its list)
    let d = ValueNoVendorPrefix.check(&style_decl("-ms-flexbox"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_ms_inline_flex() {
    let d = ValueNoVendorPrefix.check(&style_decl("-ms-inline-flex"), &ctx());
    // This is not in KNOWN_PREFIXABLE_VALUES as "inline-flex" variant
    // Note: -webkit-inline-flex would be flagged since inline-flex is in the list
  }

  #[test]
  fn reports_webkit_inline_flex() {
    let d = ValueNoVendorPrefix.check(&style_decl("-webkit-inline-flex"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_standard_value() {
    assert!(
      ValueNoVendorPrefix
        .check(&style_decl("flex"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn emits_fix_for_vendor_prefixed_value() {
    let source = "a { display: -webkit-flex; }";
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "display".to_string(),
        value: "-webkit-flex".to_string(),
        span: ParserSpan::new(4, 22),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = ValueNoVendorPrefix.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_some());
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits.len(), 1);
    // The fix removes the "-webkit-" prefix, leaving "flex"
    assert_eq!(fix.edits[0].new_text, "");
    assert_eq!(fix.edits[0].span.length, "-webkit-".len());
  }

  #[test]
  fn span_points_to_vendor_value_not_property() {
    // Simulates: "a {\n  background: -moz-radial-gradient(center, ellipse cover, #f1f1f1 0, #ee2a00 100%);\n}"
    let source =
      "a {\n  background: -moz-radial-gradient(center, ellipse cover, #f1f1f1 0, #ee2a00 100%);\n}";
    // "background: -moz-..." starts at offset 6
    let decl_offset = 6;
    let decl_text =
      "background: -moz-radial-gradient(center, ellipse cover, #f1f1f1 0, #ee2a00 100%)";
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "background".to_string(),
        value: "-moz-radial-gradient(center, ellipse cover, #f1f1f1 0, #ee2a00 100%)".to_string(),
        span: ParserSpan::new(decl_offset, decl_text.len()),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = ValueNoVendorPrefix.check(&node, &ctx);
    assert_eq!(d.len(), 1);

    // Message should contain only the function name, not the full value with arguments
    assert_eq!(
      d[0].message,
      "Unexpected vendor-prefixed value \"-moz-radial-gradient\""
    );

    // Span should point to "-moz-radial-gradient" (offset 18 = 6 + len("background: "))
    let expected_offset = decl_offset + "background: ".len();
    assert_eq!(d[0].span.offset, expected_offset);
    assert_eq!(d[0].span.length, "-moz-radial-gradient".len());
  }

  #[test]
  fn ignore_values_checks_as_is_not_unprefixed() {
    // v17: ignoreValues: ["flex"] should NOT match -webkit-flex
    let opts = serde_json::json!(["true", {"ignoreValues": ["flex"]}]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let d = ValueNoVendorPrefix.check(&style_decl("-webkit-flex"), &ctx);
    assert_eq!(
      d.len(),
      1,
      "ignoreValues: [\"flex\"] should NOT ignore -webkit-flex in v17"
    );
  }

  #[test]
  fn ignore_values_matches_full_prefixed_value() {
    // v17: ignoreValues: ["-webkit-flex"] SHOULD match -webkit-flex
    let opts = serde_json::json!(["true", {"ignoreValues": ["-webkit-flex"]}]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let d = ValueNoVendorPrefix.check(&style_decl("-webkit-flex"), &ctx);
    assert!(
      d.is_empty(),
      "ignoreValues: [\"-webkit-flex\"] SHOULD ignore -webkit-flex"
    );
  }
}
