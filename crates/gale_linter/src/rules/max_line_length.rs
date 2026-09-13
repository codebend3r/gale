use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Warn when a line exceeds a configurable maximum number of characters.
///
/// Equivalent to Stylelint's `max-line-length` rule.
///
/// Primary option: the maximum number of characters (integer). Default: 120.
/// Secondary options:
///   - `ignorePattern`: a regex pattern string (or array of patterns). Lines
///     matching any pattern are skipped.
pub struct MaxLineLength;

const DEFAULT_MAX_LENGTH: usize = 120;

impl Rule for MaxLineLength {
  fn name(&self) -> &'static str {
    "max-line-length"
  }

  fn description(&self) -> &'static str {
    "Limit the length of a line"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    // Read the max length from the primary option (integer), defaulting to 120.
    let max_length = context
      .primary_option()
      .and_then(|v| v.as_u64())
      .map(|v| v as usize)
      .unwrap_or(DEFAULT_MAX_LENGTH);

    // Read ignorePattern from secondary options.
    let ignore_patterns = build_ignore_patterns(context);

    let mut diags = Vec::new();
    let mut offset = 0;

    for (line_num, line) in context.source.split('\n').enumerate() {
      // Use the visible length (strip trailing \r if present)
      let visible = line.strip_suffix('\r').unwrap_or(line);
      let char_count = visible.chars().count();
      if char_count > max_length {
        // Check if the line matches any ignore pattern
        if !ignore_patterns.is_empty() && ignore_patterns.iter().any(|re| re.is_match(visible)) {
          offset += line.len() + 1;
          continue;
        }

        diags.push(
                    Diagnostic::new(
                        self.name(),
                        format!(
                            "Expected line {num} to be no more than {max_length} characters, but found {char_count}",
                            num = line_num + 1,
                        ),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(offset, visible.len())),
                );
      }
      // +1 for the newline character
      offset += line.len() + 1;
    }

    diags
  }
}

/// Build compiled regex patterns from the `ignorePattern` secondary option.
fn build_ignore_patterns(context: &RuleContext) -> Vec<Regex> {
  let secondary = match context.secondary_options() {
    Some(v) => v,
    None => return Vec::new(),
  };

  let obj = match secondary.as_object() {
    Some(o) => o,
    None => return Vec::new(),
  };

  let raw = match obj.get("ignorePattern") {
    Some(v) => v,
    None => return Vec::new(),
  };

  let pattern_strings: Vec<&str> = match raw {
    serde_json::Value::String(s) => vec![s.as_str()],
    serde_json::Value::Array(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
    _ => return Vec::new(),
  };

  pattern_strings
    .into_iter()
    .filter_map(|s| {
      // Stylelint accepts patterns wrapped in slashes like "/https?://.*/".
      // Strip the surrounding slashes to get the raw regex.
      let trimmed = s.trim();
      let regex_str = if trimmed.starts_with('/') && trimmed.ends_with('/') && trimmed.len() > 1 {
        &trimmed[1..trimmed.len() - 1]
      } else {
        trimmed
      };
      Regex::new(regex_str).ok()
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx_with_source(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_with_options<'a>(source: &'a str, opts: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  #[test]
  fn reports_long_line() {
    let long_line = "a".repeat(121);
    let source = format!(".foo {{ color: {}; }}", long_line);
    let d = MaxLineLength.check_root(&[], &ctx_with_source(&source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("121") || d[0].message.contains("120"));
  }

  #[test]
  fn allows_short_line() {
    let source = ".foo { color: red; }";
    let d = MaxLineLength.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_correct_line_number() {
    let source = "a { }\n".to_string() + &"b".repeat(121);
    let d = MaxLineLength.check_root(&[], &ctx_with_source(&source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("line 2"));
  }

  #[test]
  fn reads_max_length_from_config() {
    // Config: max-line-length: 80
    let opts = serde_json::json!(80);
    let source = "a".repeat(81);
    let d = MaxLineLength.check_root(&[], &ctx_with_options(&source, &opts));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("80 characters"));
  }

  #[test]
  fn reads_max_length_from_array_config() {
    // Config: max-line-length: [100, { "ignorePattern": "/https?://.*/" }]
    let opts = serde_json::json!([100, { "ignorePattern": "/https?://.*/" }]);
    // Line with URL that exceeds 100 chars
    let source = format!("// See https://example.com/{}", "x".repeat(100));
    let d = MaxLineLength.check_root(&[], &ctx_with_options(&source, &opts));
    assert!(d.is_empty(), "Should ignore lines matching ignorePattern");
  }

  #[test]
  fn reports_non_matching_long_line_with_ignore_pattern() {
    let opts = serde_json::json!([100, { "ignorePattern": "/https?://.*/" }]);
    let source = "a".repeat(101);
    let d = MaxLineLength.check_root(&[], &ctx_with_options(&source, &opts));
    assert_eq!(d.len(), 1);
  }
}
