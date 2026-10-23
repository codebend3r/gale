use gale_css_parser::postcss_tree::PostcssTree;
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
///
/// Each type comes with its offset in `params`.
fn extract_media_types(params: &str) -> Vec<(&str, usize)> {
  let mut types = Vec::new();
  let is_trimmed = |c: char| !c.is_alphanumeric() && c != '-';
  // The whitespace-separated tokens of each comma-separated query.
  let tokens = params
    .split(',')
    .scan(0, |start, query| {
      let offset = *start;
      *start += query.len() + 1;
      Some((query, offset))
    })
    .flat_map(|(query, offset)| {
      query
        .split(|c: char| c.is_ascii_whitespace())
        .scan(offset, |start, token| {
          let at = *start;
          *start += token.len() + 1;
          Some((token, at))
        })
    });
  for (token, offset) in tokens {
    let t = token.trim_matches(is_trimmed);
    if t.is_empty() {
      continue;
    }
    let lower = t.to_ascii_lowercase();
    // Skip logical keywords
    if matches!(lower.as_str(), "and" | "or" | "not" | "only") {
      continue;
    }
    // If it looks like a media type (no parens), check it
    if is_deprecated_media_type(&lower) {
      let lead = token.len() - token.trim_start_matches(is_trimmed).len();
      types.push((t, offset + lead));
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
  /// Flags media types that have been deprecated, such as `tv`, at the type,
  /// in every `@media` Stylelint's `isStandardSyntaxAtRule` accepts.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.name().eq_ignore_ascii_case("media") || !at.is_standard_syntax() {
        continue;
      }
      let params = at.raw_params();
      for (media_type, offset) in extract_media_types(&params) {
        if self.ignore_media_types.matches(media_type) {
          continue;
        }
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected deprecated media type \"{media_type}\""),
          )
          .span(Span::new(at.params_start() + offset, media_type.len())),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::{Value, json};

  use crate::testing::lint;

  /// The reports for `@media <params> {}` in CSS.
  fn media(params: &str) -> Vec<Diagnostic> {
    media_with(json!(true), params)
  }

  fn media_with(options: Value, params: &str) -> Vec<Diagnostic> {
    lint(NAME, options, &format!("@media {params} {{}}"), Syntax::Css)
  }

  #[test]
  fn reports_deprecated_tty_at_the_type() {
    let d = media("tty");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("tty"));
    assert_eq!((d[0].span.offset, d[0].span.length), (7, 3));
  }

  #[test]
  fn reports_deprecated_handheld() {
    let d = media("not handheld and (min-width: 768px)");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("handheld"));
    assert_eq!((d[0].span.offset, d[0].span.length), (11, 8));
  }

  #[test]
  fn reports_multiple_deprecated() {
    let d = media("tty,  projection");
    assert_eq!(d.len(), 2);
    assert_eq!((d[1].span.offset, d[1].span.length), (13, 10));
  }

  #[test]
  fn allows_screen_and_all() {
    assert!(media("screen and (min-width: 768px)").is_empty());
    assert!(media("all").is_empty());
  }

  #[test]
  fn ignores_non_media_at_rule() {
    assert!(lint(NAME, json!(true), "@supports tty {}", Syntax::Css).is_empty());
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
    let options = json!([true, { "ignoreMediaTypes": ["tty", "/^proj/"] }]);
    assert!(media_with(options.clone(), "tty, projection").is_empty());
    assert_eq!(media_with(options, "handheld, tty").len(), 1);
  }
}
