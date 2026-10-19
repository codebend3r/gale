use std::sync::Arc;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Options, Setting};
use crate::pattern;
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "max-line-length";

declare_rule! {
  /// Warn when a line exceeds a configurable maximum number of characters.
  ///
  /// Equivalent to Stylelint's `max-line-length` rule.
  ///
  /// Primary option: the maximum number of characters (integer). Default: 120.
  /// Secondary options:
  ///   - `ignorePattern`: a regex pattern string (or array of patterns). Lines
  ///     matching any pattern are skipped.
  pub struct MaxLineLength => MaxLineLengthCheck {
    name: NAME,
    description: "Limit the length of a line",
    default_severity: Warning,
  }
}

/// `max-line-length` as configured.
pub struct MaxLineLengthCheck {
  /// The longest a line may be, in characters.
  max_length: usize,
  /// `ignorePattern`: lines any of these match may be longer.
  ignore_patterns: Vec<Arc<pattern::Regex>>,
}

impl MaxLineLengthCheck {
  /// The options of Stylelint's former core rule: a number, then
  /// `ignore: ["non-comments", "comments"]` and `ignorePattern`.  Gale
  /// reads no option, or one that is not a whole number, as 120, and
  /// accepts but does not yet apply `ignore`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max_length = options
      .primary_test(serde_json::Value::is_number)
      .and_then(serde_json::Value::as_u64)
      .map_or(DEFAULT_MAX_LENGTH, |max| max as usize);
    options.keywords("ignore", &["non-comments", "comments"]);
    let ignore_patterns = options.strings("ignorePattern");
    let ignore_patterns = compile_ignore_patterns(&ignore_patterns, &mut options);
    options.finish()?;
    Ok(Self {
      max_length,
      ignore_patterns,
    })
  }
}

const DEFAULT_MAX_LENGTH: usize = 120;

impl Check for MaxLineLengthCheck {
  /// Flags source lines longer than the configured maximum, skipping any that
  /// match an `ignorePattern`.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let max_length = self.max_length;
    let mut diags = Vec::new();
    let mut offset = 0;

    for (line_num, line) in context.source.split('\n').enumerate() {
      // Use the visible length (strip trailing \r if present)
      let visible = line.strip_suffix('\r').unwrap_or(line);
      let char_count = visible.chars().count();
      if char_count > max_length {
        // Check if the line matches any ignore pattern
        if self
          .ignore_patterns
          .iter()
          .any(|re| pattern::is_match(re, visible))
        {
          offset += line.len() + 1;
          continue;
        }

        diags.push(
          Diagnostic::new(
            NAME,
            format!(
              "Expected line {num} to be no more than {max_length} characters, but found {char_count}",
              num = line_num + 1,
            ),
          )
          .span(Span::new(offset, visible.len())),
        );
      }
      // +1 for the newline character
      offset += line.len() + 1;
    }

    diags
  }
}

/// Compile the `ignorePattern` entries.  Stylelint accepts both a raw regex
/// and one wrapped in slashes like "/https?://.*/"; `pattern::compile`
/// understands either.  A raw regex that does not compile is an invalid
/// option; a `/regex/` literal that does not compile is left out, since the
/// rule set's scan of every rule's options already reports it.
fn compile_ignore_patterns(
  entries: &[String],
  options: &mut Options<'_>,
) -> Vec<Arc<pattern::Regex>> {
  entries
    .iter()
    .map(|entry| entry.trim())
    .filter_map(|entry| match pattern::compile(entry) {
      Ok(re) => Some(re),
      Err(reason) => {
        if pattern::split_literal(entry).is_none() {
          options.report(InvalidOption::pattern(NAME, entry, &reason));
        }
        None
      }
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source};

  #[test]
  fn reports_long_line() {
    let rule = configured(&MaxLineLength, None);
    let long_line = "a".repeat(121);
    let source = format!(".foo {{ color: {}; }}", long_line);
    let d = rule.check_root(&[], &ctx_with_source(&source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("121") || d[0].message.contains("120"));
  }

  #[test]
  fn allows_short_line() {
    let rule = configured(&MaxLineLength, None);
    let source = ".foo { color: red; }";
    let d = rule.check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_correct_line_number() {
    let rule = configured(&MaxLineLength, None);
    let source = "a { }\n".to_string() + &"b".repeat(121);
    let d = rule.check_root(&[], &ctx_with_source(&source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("line 2"));
  }

  #[test]
  fn reads_max_length_from_config() {
    // Config: max-line-length: 80
    let opts = serde_json::json!(80);
    let rule = configured(&MaxLineLength, opts.clone());
    let source = "a".repeat(81);
    let d = rule.check_root(&[], &ctx_with_source(&source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("80 characters"));
  }

  #[test]
  fn reads_max_length_from_array_config() {
    // Config: max-line-length: [100, { "ignorePattern": "/https?://.*/" }]
    let opts = serde_json::json!([100, { "ignorePattern": "/https?://.*/" }]);
    let rule = configured(&MaxLineLength, opts.clone());
    // Line with URL that exceeds 100 chars
    let source = format!("// See https://example.com/{}", "x".repeat(100));
    let d = rule.check_root(&[], &ctx_with_source(&source));
    assert!(d.is_empty(), "Should ignore lines matching ignorePattern");
  }

  #[test]
  fn reports_non_matching_long_line_with_ignore_pattern() {
    let opts = serde_json::json!([100, { "ignorePattern": "/https?://.*/" }]);
    let rule = configured(&MaxLineLength, opts.clone());
    let source = "a".repeat(101);
    let d = rule.check_root(&[], &ctx_with_source(&source));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([80, { "ignore": ["comments"], "ignorePattern": ["/^@import/", "url\\("] }])
      )
      .is_empty()
    );
    // The primary option is tested by a function, not a list.
    assert_eq!(
      invalid_options(NAME, serde_json::json!("80")),
      ["Invalid option \"80\" for rule \"max-line-length\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([80, { "ignore": ["urls"], "tabSize": 2 }])
      ),
      [
        "Invalid value \"urls\" for option \"ignore\" of rule \"max-line-length\"",
        "Invalid option name \"tabSize\" for rule \"max-line-length\"",
      ]
    );
    let broken = invalid_options(NAME, serde_json::json!([80, { "ignorePattern": "a(" }]));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"a(\" for rule \"max-line-length\""),
      "{broken:?}"
    );
  }
}
