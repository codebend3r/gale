use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow vendor prefixes in media feature names.
///
/// Equivalent to Stylelint's `media-feature-name-no-vendor-prefix` rule.
/// E.g., `@media (-webkit-min-device-pixel-ratio: 2)` should be flagged.
pub struct MediaFeatureNameNoVendorPrefix;

/// Specific vendor-prefixed media features that Stylelint flags.
/// This matches Stylelint's approach: only known autoprefixable features are
/// flagged, NOT every occurrence of a vendor prefix in a media query.
const VENDOR_PREFIXED_FEATURES: &[&str] = &[
  "-webkit-device-pixel-ratio",
  "-webkit-min-device-pixel-ratio",
  "-webkit-max-device-pixel-ratio",
  "-o-device-pixel-ratio",
  "-o-min-device-pixel-ratio",
  "-o-max-device-pixel-ratio",
  "-moz-device-pixel-ratio",
  "min--moz-device-pixel-ratio",
  "max--moz-device-pixel-ratio",
];

impl Rule for MediaFeatureNameNoVendorPrefix {
  fn name(&self) -> &'static str {
    "media-feature-name-no-vendor-prefix"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes for media feature names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(rule) = node else {
      return vec![];
    };

    // Only check @media rules
    if !rule.name.eq_ignore_ascii_case("media") {
      return vec![];
    }

    let params_lower = rule.params.to_ascii_lowercase();
    let mut diags = Vec::new();

    for feature in VENDOR_PREFIXED_FEATURES {
      if let Some(feature_pos_in_params) = params_lower.find(feature) {
        // Calculate the byte offset of the feature name in the source.
        // The at-rule span starts at `@media`, and the params start after
        // `@media `. We need to find where the params appear in the source.
        let feature_len = feature.len();

        // Try to find the feature name in the source text for accurate positioning
        let (feat_offset, feat_len) = if rule.span.offset + rule.span.length <= ctx.source.len() {
          let rule_src = &ctx.source[rule.span.offset..rule.span.offset + rule.span.length];
          let rule_lower = rule_src.to_ascii_lowercase();
          if let Some(pos) = rule_lower.find(feature) {
            (rule.span.offset + pos, feature_len)
          } else {
            // Fallback: compute from params offset
            // @media + space = 7 bytes, then params start
            let params_offset = rule.span.offset + 7; // "@media "
            (params_offset + feature_pos_in_params, feature_len)
          }
        } else {
          // Fallback if source is unavailable
          (rule.span.offset, rule.span.length)
        };

        diags.push(
          Diagnostic::new(self.name(), "Unexpected vendor-prefix".to_string())
            .severity(self.default_severity())
            .span(Span::new(feat_offset, feat_len)),
        );
        break; // one diagnostic per at-rule
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule as CssAtRule, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn media_rule(params: &str) -> CssNode {
    CssNode::AtRule(CssAtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_webkit_device_pixel_ratio() {
    let d = MediaFeatureNameNoVendorPrefix
      .check(&media_rule("(-webkit-min-device-pixel-ratio: 2)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("vendor-prefix"));
  }

  #[test]
  fn reports_moz_prefix() {
    let d =
      MediaFeatureNameNoVendorPrefix.check(&media_rule("(-moz-device-pixel-ratio: 2)"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_standard_media_features() {
    let d = MediaFeatureNameNoVendorPrefix.check(&media_rule("(min-width: 768px)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_media_at_rule() {
    let node = CssNode::AtRule(CssAtRule {
      name: "keyframes".to_string(),
      params: "-webkit-fade".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    });
    let d = MediaFeatureNameNoVendorPrefix.check(&node, &ctx());
    assert!(d.is_empty());
  }
}
