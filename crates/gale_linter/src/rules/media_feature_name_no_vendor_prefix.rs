use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::autoprefixable;
use crate::pattern;
use crate::rule::{Rule, RuleContext};

/// Disallow vendor prefixes in media feature names.
///
/// Equivalent to Stylelint's `media-feature-name-no-vendor-prefix` rule.
/// E.g., `@media (-webkit-min-device-pixel-ratio: 2)` is flagged, and the
/// fix removes the vendor prefix, keeping the rest of the name as written.
pub struct MediaFeatureNameNoVendorPrefix;

/// Stylelint's `FEATURES`: the prefixed media features it flags, in the
/// order its regex tries them.  Only these are flagged, not every vendor
/// prefix in a media query.
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

  /// Flags each vendor-prefixed media feature in every `@media` query as
  /// written (nested ones included), skipping `ignoreMediaFeatureNames`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore = ctx
      .secondary_options()
      .and_then(|v| v.get("ignoreMediaFeatureNames"));
    let mut diags = Vec::new();
    for at in &ctx.scanned_rules().at_rules {
      if !at.name.eq_ignore_ascii_case("media") || !autoprefixable::media_feature_name(&at.params) {
        continue;
      }
      // Stylelint matches against the at-rule's text and reports each match
      // at its first occurrence there.
      let prelude = ctx
        .source_slice(at.offset, at.params_offset + at.params.len())
        .unwrap_or("");
      for (index, feature) in find_features(&at.params) {
        if pattern::option_matches(ignore, feature) {
          continue;
        }
        let reported = prelude
          .find(feature)
          .map_or(at.params_offset + index, |i| at.offset + i);
        let fixed = strip_first_prefix(feature);
        diags.push(
          Diagnostic::new(self.name(), "Unexpected vendor-prefix")
            .severity(self.default_severity())
            .span(Span::new(reported, feature.len()))
            .fix(Fix::new(
              format!("Change \"{feature}\" to \"{fixed}\""),
              vec![Edit::new(
                Span::new(at.params_offset + index, feature.len()),
                fixed,
              )],
            )),
        );
      }
    }
    diags
  }
}

/// Every prefixed feature in `text`, case-insensitively, as the global
/// regex `FEATURES.join('|')` finds them: leftmost first, the first listed
/// feature winning at a position.
fn find_features(text: &str) -> Vec<(usize, &str)> {
  let lower = text.to_ascii_lowercase();
  let mut found = Vec::new();
  let mut pos = 0;
  while pos < lower.len() {
    // Multibyte characters never start a feature; step over their bytes.
    let Some(rest) = lower.get(pos..) else {
      pos += 1;
      continue;
    };
    let hit = VENDOR_PREFIXED_FEATURES
      .iter()
      .find(|feature| rest.starts_with(*feature));
    match hit {
      Some(feature) => {
        found.push((pos, &text[pos..pos + feature.len()]));
        pos += feature.len();
      }
      None => pos += 1,
    }
  }
  found
}

/// `feature` without its first `-moz-`, `-o-` or `-webkit-` (any case),
/// as Stylelint's `/-moz-|-o-|-webkit-/i` replacement removes it.
fn strip_first_prefix(feature: &str) -> String {
  let lower = feature.to_ascii_lowercase();
  let first = ["-moz-", "-o-", "-webkit-"]
    .iter()
    .filter_map(|prefix| lower.find(prefix).map(|i| (i, prefix.len())))
    .min();
  match first {
    Some((i, len)) => format!("{}{}", &feature[..i], &feature[i + len..]),
    None => feature.to_string(),
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use gale_css_parser::Syntax;
  use gale_diagnostics::apply_fixes;

  use crate::{LintRunner, RuleRegistry};

  /// Lint `css` with only this rule enabled, configured with `options`.
  fn lint(css: &str, options: serde_json::Value) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "media-feature-name-no-vendor-prefix".to_string();
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec![rule.clone()],
      HashMap::from([(rule, options)]),
    );
    runner.lint_source(css, "test.css", Syntax::Css).diagnostics
  }

  /// `css` after applying the rule's fixes until nothing changes, the way
  /// `gale --fix` does.
  fn fix(css: &str, options: serde_json::Value) -> String {
    let mut current = css.to_string();
    for _ in 0..10 {
      let (next, applied) = apply_fixes(&current, &lint(&current, options.clone()));
      if applied == 0 || next == current {
        break;
      }
      current = next;
    }
    current
  }

  #[test]
  fn strips_the_prefix_keeping_the_rest_as_written() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix("@media (-wEbKiT-mIn-DeViCe-PiXeL-rAtIo: 1) {}", on.clone()),
      "@media (mIn-DeViCe-PiXeL-rAtIo: 1) {}"
    );
    assert_eq!(
      fix(
        "@media (/* a */MIN--moz-device-pixel-ratio: 1) {}",
        on.clone()
      ),
      "@media (/* a */MIN-device-pixel-ratio: 1) {}"
    );
    assert_eq!(
      fix(
        "@media (-webkit-min-device-pixel-ratio: 0) and (-webkit-max-device-pixel-ratio: 2) {}",
        on.clone()
      ),
      "@media (min-device-pixel-ratio: 0) and (max-device-pixel-ratio: 2) {}"
    );
    let warnings = lint("@media (min--moz-device-pixel-ratio: 1) {}", on);
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (8, 27));
  }

  #[test]
  fn leaves_other_features_and_ignored_names_alone() {
    let on = serde_json::json!(true);
    for css in [
      "@media (-ms-min-resolution: 96dpi) {}",
      "@media (-ms-device-pixel-ratio: 2) {}",
      "@media (min-device-pixel-ratio: 2) {}",
    ] {
      assert!(lint(css, on.clone()).is_empty(), "{css}");
    }
    let options = serde_json::json!([true, {
      "ignoreMediaFeatureNames": ["-webkit-min-device-pixel-ratio", "/^-o/"]
    }]);
    assert!(
      lint(
        "@media (-webkit-min-device-pixel-ratio: 1) {}",
        options.clone()
      )
      .is_empty()
    );
    assert!(lint("@media (-o-device-pixel-ratio > 1) {}", options.clone()).is_empty());
    assert_eq!(
      fix("@media (-WEBKIT-MIN-DEVICE-PIXEL-RATIO: 1) {}", options),
      "@media (MIN-DEVICE-PIXEL-RATIO: 1) {}"
    );
  }

  #[test]
  fn reads_media_rules_nested_in_style_rules() {
    let css = "a { @media (-webkit-min-device-pixel-ratio: 1) { b: c } }";
    assert_eq!(
      fix(css, serde_json::json!(true)),
      "a { @media (min-device-pixel-ratio: 1) { b: c } }"
    );
  }

  #[test]
  fn reads_past_multibyte_text() {
    let css = "@media /* é */ (-webkit-device-pixel-ratio: 2) {}";
    assert_eq!(
      fix(css, serde_json::json!(true)),
      "@media /* é */ (device-pixel-ratio: 2) {}"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    let css = "@media (-webkit-device-pixel-ratio: 2) {}";
    assert_eq!(lint(css, options.clone()).len(), 1);
    assert_eq!(fix(css, options), css);
  }
}
