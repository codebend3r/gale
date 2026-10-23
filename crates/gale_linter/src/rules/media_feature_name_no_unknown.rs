use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::is_known_media_feature;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "media-feature-name-no-unknown";

declare_rule! {
  /// Reports unknown media feature names in `@media` rules.
  ///
  /// Equivalent to Stylelint's `media-feature-name-no-unknown` rule.
  ///
  /// Secondary options:
  /// - `ignoreMediaFeatureNames`: array of media feature name strings or regex
  ///   patterns (e.g. `"/^my-/"`) to ignore.
  pub struct MediaFeatureNameNoUnknown => MediaFeatureNameNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown media feature names",
    default_severity: Warning,
  }
}

/// `media-feature-name-no-unknown` as configured.
pub struct MediaFeatureNameNoUnknownCheck {
  /// `ignoreMediaFeatureNames`: names or `/regex/` entries.
  ignore_media_feature_names: Matchers,
}

impl MediaFeatureNameNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreMediaFeatureNames`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_media_feature_names = options.matchers("ignoreMediaFeatureNames");
    options.finish()?;
    Ok(Self {
      ignore_media_feature_names,
    })
  }
}

impl Check for MediaFeatureNameNoUnknownCheck {
  /// Flags media feature names that are not standard, skipping any matched by
  /// `ignoreMediaFeatureNames` and, in SCSS, any query containing
  /// interpolation.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.name().eq_ignore_ascii_case("media") {
        continue;
      }
      // The params as Stylelint's `getAtRuleParams` reads them.
      let params = at.raw_params();
      if matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) && params.contains("#{") {
        continue;
      }
      for feature in extract_media_features(&params) {
        // Skip vendor-prefixed and custom (`--`) names, and Less (`@var`)
        // and SCSS (`$var`) variables.
        if feature.name.starts_with(['-', '@', '$'])
          || self.ignore_media_feature_names.matches(&feature.name)
          || is_known_media_feature(&feature.name)
        {
          continue;
        }
        let span = find_feature_span(&params, at.params_start(), &feature)
          .unwrap_or(Span::new(at.start, feature.name.len()));
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected unknown media feature name \"{}\"", feature.name),
          )
          .span(span),
        );
      }
    }
    diags
  }
}

/// A media feature named in the params.
struct MediaFeature {
  name: String,
}

/// Where `feature` is written in `params`, which start at `params_start`:
/// its first whole-word occurrence, in any case.
fn find_feature_span(params: &str, params_start: usize, feature: &MediaFeature) -> Option<Span> {
  let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
  let lower = params.to_ascii_lowercase();
  let target = feature.name.to_ascii_lowercase();
  let bytes = params.as_bytes();
  lower.match_indices(&target).find_map(|(at, _)| {
    let end = at + target.len();
    let before_ok = at == 0 || !is_word(bytes[at - 1]);
    let after_ok = end >= bytes.len() || !is_word(bytes[end]);
    (before_ok && after_ok).then(|| Span::new(params_start + at, feature.name.len()))
  })
}

/// Extract media feature names from a @media params string.
fn extract_media_features(params: &str) -> Vec<MediaFeature> {
  let mut features = Vec::new();
  let chars: Vec<char> = params.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    if chars[i] == '(' {
      i += 1;
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }

      // Skip comments
      while i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
      }

      if i >= len || chars[i] == ')' {
        if i < len {
          i += 1;
        }
        continue;
      }

      if chars[i] == '(' {
        continue;
      }

      let start = i;
      let first_is_value = chars[i].is_ascii_digit()
        || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit());

      if first_is_value {
        while i < len
          && !chars[i].is_ascii_whitespace()
          && chars[i] != '<'
          && chars[i] != '>'
          && chars[i] != '='
          && chars[i] != ')'
        {
          i += 1;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
        while i < len && (chars[i] == '<' || chars[i] == '>' || chars[i] == '=') {
          i += 1;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
        let feat_start = i;
        while i < len
          && (chars[i].is_ascii_alphanumeric()
            || chars[i] == '-'
            || chars[i] == '$'
            || chars[i] == '@')
        {
          i += 1;
        }
        if i > feat_start {
          let name: String = chars[feat_start..i].iter().collect();
          if !matches!(name.as_str(), "not" | "and" | "or" | "only") {
            features.push(MediaFeature { name });
          }
        }
      } else {
        let is_variable = i < len
          && (chars[i] == '@'
            || chars[i] == '$'
            || (chars[i] == '-' && i + 1 < len && chars[i + 1] == '-'));

        while i < len
          && (chars[i].is_ascii_alphanumeric()
            || chars[i] == '-'
            || chars[i] == '$'
            || chars[i] == '@'
            || chars[i] == '.'
            || chars[i] == '#'
            || chars[i] == '{'
            || chars[i] == '}')
        {
          i += 1;
        }
        if i > start {
          let name: String = chars[start..i].iter().collect();
          let is_scss_namespace = name.contains(".$");

          let mut j = i;
          while j < len && chars[j].is_ascii_whitespace() {
            j += 1;
          }

          if !is_variable
            && !is_scss_namespace
            && j < len
            && (chars[j] == ':'
              || chars[j] == ')'
              || chars[j] == '<'
              || chars[j] == '>'
              || chars[j] == '=')
            && !matches!(name.as_str(), "not" | "and" | "or" | "only")
          {
            features.push(MediaFeature { name: name.clone() });
          }

          // Scan forward for RHS feature names in range syntax
          let mut k = j;
          let first_followed_by_colon = j < len && chars[j] == ':';
          if first_followed_by_colon {
            k = len;
          }
          while k < len && chars[k] != ')' {
            if chars[k] == '<' || chars[k] == '>' || chars[k] == '=' {
              while k < len && (chars[k] == '<' || chars[k] == '>' || chars[k] == '=') {
                k += 1;
              }
              while k < len && chars[k].is_ascii_whitespace() {
                k += 1;
              }
              let val_start = k;
              let val_is_digit = k < len
                && (chars[k].is_ascii_digit()
                  || (chars[k] == '.' && k + 1 < len && chars[k + 1].is_ascii_digit()));
              let val_is_variable = k < len && (chars[k] == '$' || chars[k] == '@');

              if val_is_digit {
                while k < len
                  && !chars[k].is_ascii_whitespace()
                  && chars[k] != ')'
                  && chars[k] != '<'
                  && chars[k] != '>'
                  && chars[k] != '='
                {
                  k += 1;
                }
              } else if val_is_variable {
                while k < len
                  && (chars[k].is_ascii_alphanumeric()
                    || chars[k] == '-'
                    || chars[k] == '$'
                    || chars[k] == '@'
                    || chars[k] == '.')
                {
                  k += 1;
                }
              } else {
                while k < len
                  && (chars[k].is_ascii_alphanumeric()
                    || chars[k] == '-'
                    || chars[k] == '.'
                    || chars[k] == '$')
                {
                  k += 1;
                }
                if k > val_start {
                  let rhs_name: String = chars[val_start..k].iter().collect();
                  if rhs_name.contains(".$") || rhs_name.contains('$') {
                    // SCSS variable, skip
                  } else if !matches!(rhs_name.as_str(), "not" | "and" | "or" | "only") {
                    features.push(MediaFeature { name: rhs_name });
                  }
                }
              }
            } else {
              k += 1;
            }
          }
        }
      }
    } else {
      i += 1;
    }
  }

  features
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{Value, json};

  use crate::testing::lint;

  /// The reports for `@media <params> {}` in `syntax`.
  fn media(options: Value, params: &str, syntax: Syntax) -> Vec<Diagnostic> {
    lint(NAME, options, &format!("@media {params} {{}}"), syntax)
  }

  #[test]
  fn reports_unknown_media_feature_at_its_name() {
    let d = media(json!(true), "screen and (min-wdith: 768px)", Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("min-wdith"));
    assert_eq!((d[0].span.offset, d[0].span.length), (19, 9));
    // A feature named like the at-rule is found in the params, not in `@media`.
    let d = lint(NAME, json!(true), "@MEDIA (media: 1) {}", Syntax::Scss);
    assert_eq!((d[0].span.offset, d[0].span.length), (8, 5));
  }

  #[test]
  fn allows_known_media_features() {
    for params in [
      "(min-width: 768px)",
      "(hover: hover)",
      "(prefers-color-scheme: dark)",
      "(MIN-WIDTH: 1px)",
    ] {
      assert!(
        media(json!(true), params, Syntax::Css).is_empty(),
        "{params}"
      );
    }
  }

  #[test]
  fn allows_vendor_prefixed_and_variables() {
    let params = "(-webkit-min-device-pixel-ratio: 2) and ($bp: 1px)";
    assert!(media(json!(true), params, Syntax::Scss).is_empty());
  }

  #[test]
  fn checks_nested_media_and_skips_interpolated_queries() {
    let scss = "a { @media (colour: 1) { b: c } }\n@media (#{$f}: 1) {}";
    let d = lint(NAME, json!(true), scss, Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("colour"));
  }

  #[test]
  fn extracts_range_syntax() {
    let features = extract_media_features("(10px >= width <= 100px)");
    assert_eq!(features.len(), 1);
    assert_eq!(features[0].name, "width");
  }

  #[test]
  fn extracts_features_after_operator() {
    let features = extract_media_features("($tablet = unknown)");
    assert!(features.iter().any(|f| f.name == "unknown"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreMediaFeatureNames": "/^my-/" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreMediaFeatures": ["a"] }])
      ),
      ["Invalid option name \"ignoreMediaFeatures\" for rule \"media-feature-name-no-unknown\""]
    );
  }

  #[test]
  fn ignore_media_feature_names_takes_names_and_regexes() {
    let options = json!([true, { "ignoreMediaFeatureNames": ["min-wdith", "/^my-/"] }]);
    assert!(media(options.clone(), "(min-wdith: 768px)", Syntax::Css).is_empty());
    assert!(media(options.clone(), "(my-feature: 1)", Syntax::Css).is_empty());
    assert_eq!(media(options, "(max-wdith: 768px)", Syntax::Css).len(), 1);
  }
}
