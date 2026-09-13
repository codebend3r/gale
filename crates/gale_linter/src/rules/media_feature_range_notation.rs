use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify context or prefix notation for media feature ranges.
///
/// Equivalent to Stylelint's `media-feature-range-notation` rule.
///
/// Primary option:
///   - `"context"` (default): Expect range notation (e.g. `width >= 768px`)
///   - `"prefix"`: Expect prefix notation (e.g. `min-width: 768px`)
pub struct MediaFeatureRangeNotation;

/// Prefixes that indicate the old min/max notation.
const RANGE_PREFIXES: &[&str] = &["min-", "max-"];

/// Media features that support range notation.
const RANGE_FEATURES: &[&str] = &[
  "width",
  "height",
  "device-width",
  "device-height",
  "aspect-ratio",
  "device-aspect-ratio",
  "color",
  "color-index",
  "monochrome",
  "resolution",
];

/// Range comparison operators used in context notation.
const RANGE_OPS: &[&str] = &[">=", "<=", ">", "<"];

impl Rule for MediaFeatureRangeNotation {
  fn name(&self) -> &'static str {
    "media-feature-range-notation"
  }

  fn description(&self) -> &'static str {
    "Specify context or prefix notation for media feature ranges"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "media" {
      return vec![];
    }

    // Skip media queries containing SCSS/Less variables, interpolation,
    // or SCSS math expressions — the actual values are unknown until
    // compilation, so range notation cannot be determined.
    if at.params.contains('$') || at.params.contains("#{") || at.params.contains("@{") {
      return vec![];
    }
    // In SCSS/Less, skip if params contain SCSS math operators (like `(124px + 300px)`)
    if ctx.syntax != gale_css_parser::Syntax::Css
      && (at.params.contains(" + ") || at.params.contains(" - ") || at.params.contains(" * "))
    {
      return vec![];
    }

    let notation = parse_notation(ctx.options);

    // If "prefix" notation, skip entirely — Gale does not currently detect
    // range→prefix violations (requires parsing range syntax which is rare).
    if notation == Notation::Prefix {
      return vec![];
    }

    // Determine the notation option string for the message
    let notation_str = ctx
      .options
      .and_then(|v| v.as_str())
      .or_else(|| {
        ctx
          .options
          .and_then(|v| v.as_array())
          .and_then(|a| a.first())
          .and_then(|v| v.as_str())
      })
      .unwrap_or("context");

    let mut diags = Vec::new();
    let mut seen_offsets = std::collections::HashSet::new();

    let at_src_end = (at.span.offset + at.span.length).min(ctx.source.len());
    let at_src = &ctx.source[at.span.offset..at_src_end];
    let at_src_lower = at_src.to_ascii_lowercase();
    for &prefix in RANGE_PREFIXES {
      for &feature in RANGE_FEATURES {
        let prefixed = format!("{prefix}{feature}");
        // Search for `(min-width` or `( min-width` — the prefixed feature
        // MUST follow an opening paren to be a media feature (not a CSS
        // property like `min-width: 100px` inside the rule body).
        let paren_search = format!("({prefixed}");
        let paren_off = at_src_lower.find(&paren_search).or_else(|| {
          // Try with whitespace between `(` and feature name
          at_src_lower.find(&prefixed).and_then(|p| {
            // Walk backwards from the match to find `(`
            let before = &at_src_lower[..p];
            let trimmed = before.trim_end();
            if trimmed.ends_with('(') {
              Some(trimmed.len() - 1)
            } else {
              None
            }
          })
        });
        if let Some(paren_off) = paren_off {
          let abs_offset = at.span.offset + paren_off;
          if seen_offsets.insert(abs_offset) {
            diags.push(
              Diagnostic::new(
                self.name(),
                format!("Expected \"{notation_str}\" media feature range notation"),
              )
              .severity(self.default_severity())
              .span(Span::new(abs_offset, 1)),
            );
          }
        }
      }
    }
    diags
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Notation {
  Context,
  Prefix,
}

fn parse_notation(options: Option<&serde_json::Value>) -> Notation {
  let Some(value) = options else {
    return Notation::Context;
  };
  match value {
    serde_json::Value::String(s) => {
      if s == "prefix" {
        Notation::Prefix
      } else {
        Notation::Context
      }
    }
    serde_json::Value::Array(arr) => {
      if let Some(s) = arr.first().and_then(|v| v.as_str())
        && s == "prefix"
      {
        return Notation::Prefix;
      }
      Notation::Context
    }
    _ => Notation::Context,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  /// Build a context whose `source` covers the at-rule text so the rule
  /// can scan the original (un-normalised) source.
  fn ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  /// Build a media at-rule node whose span covers `[0..source.len()]`.
  /// `params` may be the lightningcss-normalised form (e.g. `width >= 768px`),
  /// while `source` is the original text the rule will scan.
  fn media_with_source(params: &str, source_len: usize) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, source_len),
      children: vec![],
    })
  }

  #[test]
  fn reports_min_width_prefix() {
    // lightningcss normalises `(min-width: 768px)` → `(width >= 768px)` in params,
    // but the source text still has the original prefix notation.
    let source = "@media (min-width: 768px) {}";
    let node = media_with_source("(width >= 768px)", source.len());
    let ctx = ctx_with_source(source);
    let d = MediaFeatureRangeNotation.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("media feature range notation"));
  }

  #[test]
  fn allows_range_syntax() {
    let source = "@media (width >= 768px) {}";
    let node = media_with_source("(width >= 768px)", source.len());
    let ctx = ctx_with_source(source);
    let d = MediaFeatureRangeNotation.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_scss_variables() {
    let source = "@media (min-width: $breakpoint) {}";
    let node = media_with_source("(min-width: $breakpoint)", source.len());
    let ctx = ctx_with_source(source);
    let d = MediaFeatureRangeNotation.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn prefix_notation_allows_prefix() {
    let source = "@media (min-width: 768px) {}";
    let opts = serde_json::json!("prefix");
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let node = media_with_source("(width >= 768px)", source.len());
    let d = MediaFeatureRangeNotation.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn reports_max_height_prefix() {
    let source = "@media (max-height: 500px) {}";
    let node = media_with_source("(height <= 500px)", source.len());
    let ctx = ctx_with_source(source);
    let d = MediaFeatureRangeNotation.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("media feature range notation"));
  }
}
