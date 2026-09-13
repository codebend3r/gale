use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::data::is_known_media_feature;
use crate::rule::{Rule, RuleContext};

/// Reports unknown media feature names in `@media` rules.
///
/// Equivalent to Stylelint's `media-feature-name-no-unknown` rule.
///
/// Secondary options:
/// - `ignoreMediaFeatureNames`: array of media feature name strings or regex
///   patterns (e.g. `"/^my-/"`) to ignore.
pub struct MediaFeatureNameNoUnknown;

impl Rule for MediaFeatureNameNoUnknown {
  fn name(&self) -> &'static str {
    "media-feature-name-no-unknown"
  }

  fn description(&self) -> &'static str {
    "Disallow unknown media feature names"
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

    // Parse ignoreMediaFeatureNames option
    let ignore_list = parse_ignore_list(ctx.options);

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

      if is_ignored(&feature.name, &ignore_list) {
        continue;
      }

      if !is_known_media_feature(&feature.name) {
        // Find the feature name directly in the source text
        let span = find_feature_span_in_source(ctx.source, at.span.offset, &feature);

        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected unknown media feature name \"{}\"", feature.name),
          )
          .severity(self.default_severity())
          .span(span),
        );
      }
    }
    diags
  }
}

/// A media feature with its name and byte offset within the params string.
struct MediaFeature {
  name: String,
  /// Byte offset of the feature name within the params string.
  #[allow(dead_code)]
  offset_in_params: usize,
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

/// Parse `ignoreMediaFeatureNames` from the secondary options.
fn parse_ignore_list(options: Option<&serde_json::Value>) -> Vec<String> {
  let Some(opts) = options else {
    return vec![];
  };
  let obj = match opts {
    serde_json::Value::Object(o) => o,
    serde_json::Value::Array(arr) => {
      for item in arr {
        if let serde_json::Value::Object(o) = item
          && let Some(serde_json::Value::Array(names)) = o.get("ignoreMediaFeatureNames")
        {
          return names
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        }
      }
      return vec![];
    }
    _ => return vec![],
  };
  if let Some(serde_json::Value::Array(names)) = obj.get("ignoreMediaFeatureNames") {
    names
      .iter()
      .filter_map(|v| v.as_str().map(String::from))
      .collect()
  } else {
    vec![]
  }
}

/// Check if a media feature name matches any ignore pattern.
fn is_ignored(name: &str, ignore_list: &[String]) -> bool {
  for pattern in ignore_list {
    if let Some(re) = parse_regex_pattern(pattern) {
      if re.is_match(name) {
        return true;
      }
    } else if pattern == name {
      return true;
    }
  }
  false
}

/// Parse a Stylelint-style regex pattern like `/pattern/` or `/pattern/i`.
fn parse_regex_pattern(s: &str) -> Option<Regex> {
  if let Some(rest) = s.strip_prefix('/') {
    if let Some(end) = rest.rfind('/') {
      let pattern = &rest[..end];
      let flags = &rest[end + 1..];
      let full_pattern = if flags.contains('i') {
        format!("(?i){pattern}")
      } else {
        pattern.to_string()
      };
      Regex::new(&full_pattern).ok()
    } else {
      None
    }
  } else {
    None
  }
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
            let byte_offset: usize = chars[..feat_start].iter().map(|c| c.len_utf8()).sum();
            features.push(MediaFeature {
              name,
              offset_in_params: byte_offset,
            });
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
            let byte_offset: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
            features.push(MediaFeature {
              name: name.clone(),
              offset_in_params: byte_offset,
            });
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
                    let byte_offset: usize = chars[..val_start].iter().map(|c| c.len_utf8()).sum();
                    features.push(MediaFeature {
                      name: rhs_name,
                      offset_in_params: byte_offset,
                    });
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
  use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_unknown_media_feature() {
    let d = MediaFeatureNameNoUnknown.check(&media("(min-wdith: 768px)"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("min-wdith"));
  }

  #[test]
  fn allows_known_media_features() {
    assert!(
      MediaFeatureNameNoUnknown
        .check(&media("(min-width: 768px)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaFeatureNameNoUnknown
        .check(&media("(hover: hover)"), &ctx())
        .is_empty()
    );
    assert!(
      MediaFeatureNameNoUnknown
        .check(&media("(prefers-color-scheme: dark)"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_vendor_prefixed() {
    assert!(
      MediaFeatureNameNoUnknown
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
}
