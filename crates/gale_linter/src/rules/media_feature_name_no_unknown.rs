use gale_css_parser::CssNode;
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
  /// `ignoreMediaFeatureNames` and any query containing interpolation.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "media" {
      return vec![];
    }

    // In SCSS/Sass, skip @media rules whose params contain interpolation
    if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    ) {
      let at_end = (at.span.offset + at.span.length).min(ctx.source.len());
      let at_source = if at.span.offset < at_end {
        &ctx.source[at.span.offset..at_end]
      } else {
        ""
      };
      if at_source.contains("#{") || at.params.contains("#{") {
        return vec![];
      }
    }

    let mut diags = Vec::new();
    for feature in extract_media_features(&at.params) {
      // Skip vendor-prefixed features
      if feature.name.starts_with('-') {
        continue;
      }
      // Skip custom media feature names (--prefix)
      if feature.name.starts_with("--") {
        continue;
      }
      // Skip Less variables (@var)
      if feature.name.starts_with('@') {
        continue;
      }
      // Skip SCSS variables ($var)
      if feature.name.starts_with('$') {
        continue;
      }

      if self.ignore_media_feature_names.matches(&feature.name) {
        continue;
      }

      if !is_known_media_feature(&feature.name) {
        // Find the feature name directly in the source text
        let span = find_feature_span_in_source(ctx.source, at.span.offset, &feature);

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

/// Find the feature name directly in the source text by searching for it
/// as a whole word.
fn find_feature_span_in_source(source: &str, at_offset: usize, feature: &MediaFeature) -> Span {
  let at_source = &source[at_offset..];
  let feature_lower = feature.name.to_ascii_lowercase();
  let at_lower = at_source.to_ascii_lowercase();
  let mut search_start = 0;

  while search_start < at_lower.len() {
    if let Some(pos) = at_lower[search_start..].find(&feature_lower) {
      let abs_pos = search_start + pos;

      let before_ok = if abs_pos == 0 {
        true
      } else {
        let prev = at_source.as_bytes()[abs_pos - 1];
        !prev.is_ascii_alphanumeric() && prev != b'-' && prev != b'_'
      };

      let after_pos = abs_pos + feature.name.len();
      let after_ok = if after_pos >= at_source.len() {
        true
      } else {
        let next = at_source.as_bytes()[after_pos];
        !next.is_ascii_alphanumeric() && next != b'-' && next != b'_'
      };

      if before_ok && after_ok {
        return Span::new(at_offset + abs_pos, feature.name.len());
      }

      search_start = abs_pos + 1;
    } else {
      break;
    }
  }

  Span::new(at_offset, feature.name.len())
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
  use gale_css_parser::AtRule;

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
  fn reports_unknown_media_feature() {
    let rule = configured(&MediaFeatureNameNoUnknown, None);
    let d = rule.check(&media("(min-wdith: 768px)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("min-wdith"));
  }

  #[test]
  fn allows_known_media_features() {
    let rule = configured(&MediaFeatureNameNoUnknown, None);
    assert!(rule.check(&media("(min-width: 768px)"), &ctx()).is_empty());
    assert!(rule.check(&media("(hover: hover)"), &ctx()).is_empty());
    assert!(
      rule
        .check(&media("(prefers-color-scheme: dark)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_vendor_prefixed() {
    let rule = configured(&MediaFeatureNameNoUnknown, None);
    assert!(
      rule
        .check(&media("(-webkit-min-device-pixel-ratio: 2)"), &ctx())
        .is_empty()
    );
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
    let options = serde_json::json!([true, { "ignoreMediaFeatureNames": ["min-wdith", "/^my-/"] }]);
    let rule = configured(&MediaFeatureNameNoUnknown, options);
    assert!(rule.check(&media("(min-wdith: 768px)"), &ctx()).is_empty());
    assert!(rule.check(&media("(my-feature: 1)"), &ctx()).is_empty());
    assert_eq!(rule.check(&media("(max-wdith: 768px)"), &ctx()).len(), 1);
  }
}
