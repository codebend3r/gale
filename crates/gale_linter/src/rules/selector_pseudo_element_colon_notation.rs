use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Enforces a specific colon notation (`::` or `:`) for pseudo-elements that
/// support both syntaxes (`:before`, `:after`, `:first-line`, `:first-letter`).
///
/// Equivalent to Stylelint's `selector-pseudo-element-colon-notation` rule.
/// Primary option: `"double"` (default) or `"single"`.
pub struct SelectorPseudoElementColonNotation;

const LEGACY_PSEUDO_ELEMENTS: &[&str] = &["before", "after", "first-line", "first-letter"];

impl Rule for SelectorPseudoElementColonNotation {
  fn name(&self) -> &'static str {
    "selector-pseudo-element-colon-notation"
  }

  fn description(&self) -> &'static str {
    "Specify single or double colon notation for applicable pseudo-elements"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let source = ctx.source;
    if source.is_empty() {
      return vec![];
    }

    // Determine mode from primary option: "single" or "double" (default)
    let mode = ctx.primary_option_str().unwrap_or("double");
    let want_single = mode == "single";

    // The parsed selector from lightningcss normalizes to `::`, so we must
    // look at the original source text to detect actual notation.
    // IMPORTANT: only look at the SELECTOR source portion (before `{`) to
    // avoid flagging `:before`/`:after` that appear in child rules.
    let rule_src_start = rule.span.offset;
    let rule_src_end = (rule.span.offset + rule.span.length).min(source.len());
    let rule_src = if rule_src_start < rule_src_end {
      &source[rule_src_start..rule_src_end]
    } else {
      ""
    };
    // The selector ends at the first `{`.
    let selector_src_len = rule_src.find('{').unwrap_or(rule_src.len());
    let selector_src = &rule_src[..selector_src_len];
    let selector_src_lower = selector_src.to_ascii_lowercase();

    // Check if the selector (normalized or in source) contains any relevant pseudo-elements.
    // lightningcss normalizes :before → ::before, but raffia (SCSS) may not.
    let sel_lower = rule.selector.to_ascii_lowercase();
    let has_relevant_pseudo = LEGACY_PSEUDO_ELEMENTS.iter().any(|p| {
      sel_lower.contains(&format!("::{p}"))
        || sel_lower.contains(&format!(":{p}"))
        || selector_src_lower.contains(&format!("::{p}"))
        || selector_src_lower.contains(&format!(":{p}"))
    });
    if !has_relevant_pseudo {
      return vec![];
    }

    let mut diags = Vec::new();

    for pseudo in LEGACY_PSEUDO_ELEMENTS {
      let single_pattern = format!(":{pseudo}");
      let double_pattern = format!("::{pseudo}");
      let lower_search = &selector_src_lower;

      if want_single {
        // Looking for `::pseudo` that should be `:pseudo`
        let mut pos = 0;
        while let Some(idx) = lower_search[pos..].find(&double_pattern) {
          let abs_idx = pos + idx;
          // Make sure it's not `:::pseudo` (triple colon)
          let is_triple = abs_idx > 0 && lower_search.as_bytes()[abs_idx - 1] == b':';
          let end_idx = abs_idx + double_pattern.len();
          let at_boundary = end_idx >= lower_search.len()
            || !lower_search.as_bytes()[end_idx].is_ascii_alphanumeric();

          if !is_triple && at_boundary {
            let fix_offset = rule_src_start + abs_idx;
            // Report position at the second colon to match Stylelint
            let report_offset = fix_offset + 1;
            diags.push(
              Diagnostic::new(
                self.name(),
                "Expected single colon pseudo-element notation".to_string(),
              )
              .severity(self.default_severity())
              .span(Span::new(report_offset, double_pattern.len() - 1))
              .fix(Fix::new(
                format!("Replace ::{pseudo} with :{pseudo}"),
                vec![Edit::new(
                  Span::new(fix_offset, double_pattern.len()),
                  format!(":{pseudo}"),
                )],
              )),
            );
          }
          pos = abs_idx + 1;
        }
      } else {
        // Looking for `:pseudo` that should be `::pseudo` (default "double" mode)
        let mut pos = 0;
        while let Some(idx) = lower_search[pos..].find(&single_pattern) {
          let abs_idx = pos + idx;
          let is_double = abs_idx > 0 && lower_search.as_bytes()[abs_idx - 1] == b':';
          let end_idx = abs_idx + single_pattern.len();
          let at_boundary = end_idx >= lower_search.len()
            || !lower_search.as_bytes()[end_idx].is_ascii_alphanumeric();

          if !is_double && at_boundary {
            let fix_offset = rule_src_start + abs_idx;
            diags.push(
              Diagnostic::new(
                self.name(),
                "Expected double colon pseudo-element notation".to_string(),
              )
              .severity(self.default_severity())
              .span(Span::new(fix_offset, single_pattern.len()))
              .fix(Fix::new(
                format!("Replace :{pseudo} with ::{pseudo}"),
                vec![Edit::new(
                  Span::new(fix_offset, single_pattern.len()),
                  format!("::{pseudo}"),
                )],
              )),
            );
            break;
          }
          pos = abs_idx + 1;
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn style_with_selector_and_source(
    sel: &str,
    source: &'static str,
  ) -> (CssNode, RuleContext<'static>) {
    let node = CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    (node, ctx)
  }

  fn style_with_selector_source_options<'a>(
    sel: &str,
    source: &'a str,
    options: &'a serde_json::Value,
  ) -> (CssNode, RuleContext<'a>) {
    let node = CssNode::Style(StyleRule {
      selector: sel.to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(options),
    };
    (node, ctx)
  }

  #[test]
  fn reports_single_colon_before() {
    // lightningcss normalizes selector to "a::before" but source has single colon
    let source = "a:before { color: red; }";
    let (node, ctx) = style_with_selector_and_source("a::before", source);
    let d = SelectorPseudoElementColonNotation.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(
      d[0]
        .message
        .contains("double colon pseudo-element notation")
    );
  }

  #[test]
  fn allows_double_colon_before() {
    let source = "a::before { color: red; }";
    let (node, ctx) = style_with_selector_and_source("a::before", source);
    let d = SelectorPseudoElementColonNotation.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_single_colon_after() {
    let source = "a:after { color: red; }";
    let (node, ctx) = style_with_selector_and_source("a::after", source);
    let d = SelectorPseudoElementColonNotation.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn single_mode_reports_double_colon() {
    let source = "a::before { color: red; }";
    let opts = serde_json::json!("single");
    let (node, ctx) = style_with_selector_source_options("a::before", source, &opts);
    let d = SelectorPseudoElementColonNotation.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("single colon"));
  }

  #[test]
  fn single_mode_allows_single_colon() {
    let source = "a:before { color: red; }";
    let opts = serde_json::json!("single");
    let (node, ctx) = style_with_selector_source_options("a::before", source, &opts);
    let d = SelectorPseudoElementColonNotation.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn single_mode_reports_double_colon_after() {
    let source = "a::after { color: red; }";
    let opts = serde_json::json!("single");
    let (node, ctx) = style_with_selector_source_options("a::after", source, &opts);
    let d = SelectorPseudoElementColonNotation.check(&node, &ctx);
    assert_eq!(d.len(), 1);
  }
}
