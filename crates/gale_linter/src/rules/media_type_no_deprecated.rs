use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

/// Deprecated media types (sorted for binary search).
static DEPRECATED_MEDIA_TYPES: &[&str] = &[
  "aural",
  "braille",
  "embossed",
  "handheld",
  "projection",
  "tty",
  "tv",
];

/// Case-insensitive lookup in the sorted deprecated media type table.
fn is_deprecated_media_type(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  DEPRECATED_MEDIA_TYPES
    .binary_search(&lower.as_str())
    .is_ok()
}

/// Extract media types from the `params` string of a `@media` at-rule.
///
/// Media types appear as bare identifiers before any `and`/`or`/`not`/`only`
/// keywords and parenthesized feature expressions. For example:
///   - `"screen and (min-width: 768px)"` → `["screen"]`
///   - `"tty, projection"` → `["tty", "projection"]`
///   - `"not handheld"` → `["handheld"]`
fn extract_media_types(params: &str) -> Vec<String> {
  let mut types = Vec::new();
  // Split on commas to handle media query lists, then process each query.
  for query in params.split(',') {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    for token in &tokens {
      let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
      if t.is_empty() || t.starts_with('(') {
        continue;
      }
      let lower = t.to_ascii_lowercase();
      // Skip logical keywords
      if matches!(lower.as_str(), "and" | "or" | "not" | "only") {
        continue;
      }
      // If it looks like a media type (no parens), check it
      if is_deprecated_media_type(&lower) {
        types.push(t.to_string());
      }
    }
  }
  types
}

const NAME: &str = "media-type-no-deprecated";

declare_rule! {
  pub struct MediaTypeNoDeprecated => MediaTypeNoDeprecatedCheck {
    name: NAME,
    description: "Disallow deprecated media types",
    default_severity: Warning,
  }
}

/// `media-type-no-deprecated` as configured.
pub struct MediaTypeNoDeprecatedCheck {
  /// `ignoreMediaTypes`: media types (names or `/regex/` entries) that may
  /// be used.
  ignore_media_types: Matchers,
}

impl MediaTypeNoDeprecatedCheck {
  /// Stylelint's options: `true`, then `ignoreMediaTypes`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_media_types = options.matchers("ignoreMediaTypes");
    options.finish()?;
    Ok(Self { ignore_media_types })
  }
}

impl Check for MediaTypeNoDeprecatedCheck {
  /// Flags media types that have been deprecated, such as `tv`.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };

    if !at.name.eq_ignore_ascii_case("media") {
      return vec![];
    }

    let deprecated_types = extract_media_types(&at.params);
    deprecated_types
      .into_iter()
      .filter(|media_type| !self.ignore_media_types.matches(media_type))
      .map(|media_type| {
        Diagnostic::new(
          NAME,
          format!("Unexpected deprecated media type \"{}\"", media_type),
        )
        .span(Span::new(at.span.offset, at.span.length))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, CssNode, Span};

  use crate::testing::{configured, ctx};

  fn media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: Span::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_deprecated_tty() {
    let rule = configured(&MediaTypeNoDeprecated, None);
    let d = rule.check(&media("tty"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("tty"));
  }

  #[test]
  fn reports_deprecated_handheld() {
    let rule = configured(&MediaTypeNoDeprecated, None);
    let d = rule.check(&media("handheld and (min-width: 768px)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("handheld"));
  }

  #[test]
  fn reports_multiple_deprecated() {
    let rule = configured(&MediaTypeNoDeprecated, None);
    let d = rule.check(&media("tty, projection"), &ctx());
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn allows_screen() {
    let rule = configured(&MediaTypeNoDeprecated, None);
    assert!(
      rule
        .check(&media("screen and (min-width: 768px)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_all() {
    let rule = configured(&MediaTypeNoDeprecated, None);
    assert!(rule.check(&media("all"), &ctx()).is_empty());
  }

  #[test]
  fn ignores_non_media_at_rule() {
    let rule = configured(&MediaTypeNoDeprecated, None);
    let node = CssNode::AtRule(AtRule {
      name: "keyframes".to_string(),
      params: "tty".to_string(),
      span: Span::new(0, 0),
      children: vec![],
    });
    assert!(rule.check(&node, &ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreMediaTypes": ["tty", "/^proj/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreTypes": ["tty"] }])),
      ["Invalid option name \"ignoreTypes\" for rule \"media-type-no-deprecated\""]
    );
  }

  #[test]
  fn ignore_media_types_skips_listed_types() {
    let options = serde_json::json!([true, { "ignoreMediaTypes": ["tty", "/^proj/"] }]);
    let rule = configured(&MediaTypeNoDeprecated, options);
    assert!(rule.check(&media("tty, projection"), &ctx()).is_empty());
    assert_eq!(rule.check(&media("handheld, tty"), &ctx()).len(), 1);
  }
}
