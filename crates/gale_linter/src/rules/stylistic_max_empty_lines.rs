use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/max-empty-lines";

declare_rule! {
  /// Limit the number of adjacent empty lines.
  ///
  /// Equivalent to Stylelint's `@stylistic/max-empty-lines` rule.
  pub struct StylisticMaxEmptyLines => StylisticMaxEmptyLinesCheck {
    name: NAME,
    description: "Limit the number of adjacent empty lines",
    default_severity: Warning,
  }
}

/// `@stylistic/max-empty-lines` as configured.
pub struct StylisticMaxEmptyLinesCheck {
  /// The most empty lines allowed in a row.
  max: usize,
}

impl StylisticMaxEmptyLinesCheck {
  /// The plugin's options: a number (`possible: isNumber`), then
  /// `ignore: ["comments"]`, which gale accepts but does not yet apply.  Gale
  /// reads no number as 1, and a number that is not a whole number of zero or
  /// more as 1 too, as it always has.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let max = options.primary_test(serde_json::Value::is_number);
    options.keywords("ignore", &["comments"]);
    options.finish()?;
    Ok(Self {
      max: max.and_then(serde_json::Value::as_u64).unwrap_or(1) as usize,
    })
  }
}

impl Check for StylisticMaxEmptyLinesCheck {
  /// Flags runs of blank lines longer than the maximum.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let max = self.max;

    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();

    // We need to skip strings and comments while counting empty lines
    let mut i = 0;
    let mut consecutive_empty = 0;
    let mut line_start = 0;
    let mut line_is_empty = true;

    while i < len {
      // Skip strings
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        line_is_empty = false;
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          if i < len && bytes[i] == b'\n' {
            // Newline inside string - reset tracking
            consecutive_empty = 0;
            line_is_empty = true;
            line_start = i + 1;
          }
          i += 1;
        }
        i += 1;
        continue;
      }

      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        line_is_empty = false;
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          if bytes[i] == b'\n' {
            consecutive_empty = 0;
            line_is_empty = true;
            line_start = i + 1;
          }
          i += 1;
        }
        i += 2; // skip */
        // The closing `*/` is non-empty content on the current line, so
        // reset line_is_empty. Without this, a multi-line comment whose
        // body ends with a newline (e.g. "/*\n * text\n */") would leave
        // line_is_empty=true, making the `\n` after `*/` look like an
        // extra empty line.
        line_is_empty = false;
        continue;
      }

      if bytes[i] == b'\n' {
        if line_is_empty {
          consecutive_empty += 1;
        } else {
          consecutive_empty = 0;
        }

        if consecutive_empty > max {
          diagnostics.push(
            Diagnostic::new(NAME, format!("Expected no more than {} empty line(s)", max))
              .span(Span::new(line_start, i - line_start)),
          );
        }

        line_start = i + 1;
        line_is_empty = true;
        i += 1;
        continue;
      }

      if bytes[i] == b'\r' {
        i += 1;
        continue;
      }

      if bytes[i] != b' ' && bytes[i] != b'\t' {
        line_is_empty = false;
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_single_empty_line() {
    let max = serde_json::json!(1);
    let source = "a { }\n\nb { }";
    let d =
      configured(&StylisticMaxEmptyLines, max.clone()).check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_too_many_empty_lines() {
    let max = serde_json::json!(1);
    let source = "a { }\n\n\nb { }";
    let d =
      configured(&StylisticMaxEmptyLines, max.clone()).check_root(&[], &ctx_with_source(source));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("no more than 1"));
  }

  #[test]
  fn allows_two_empty_lines_when_max_is_two() {
    let max = serde_json::json!(2);
    let source = "a { }\n\n\nb { }";
    let d =
      configured(&StylisticMaxEmptyLines, max.clone()).check_root(&[], &ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_three_empty_lines_when_max_is_two() {
    let max = serde_json::json!(2);
    let source = "a { }\n\n\n\nb { }";
    let d =
      configured(&StylisticMaxEmptyLines, max.clone()).check_root(&[], &ctx_with_source(source));
    assert!(!d.is_empty());
  }

  #[test]
  fn multiline_comment_close_does_not_count_as_empty_line() {
    // A multi-line comment whose last body line ends with \n before `*/`
    // must not cause the line containing `*/` to count as empty.
    // Bootstrap's cover.css starts with exactly this pattern.
    let max = serde_json::json!(2);
    let source = "/*\n * Globals\n */\n\n\n/* Custom button */\n.foo { }";
    let d =
      configured(&StylisticMaxEmptyLines, max.clone()).check_root(&[], &ctx_with_source(source));
    assert!(
      d.is_empty(),
      "two empty lines after a multi-line comment should be allowed with max=2"
    );
  }

  #[test]
  fn three_empty_lines_after_multiline_comment_is_flagged() {
    let max = serde_json::json!(2);
    let source = "/*\n * Globals\n */\n\n\n\n/* Custom button */\n.foo { }";
    let d =
      configured(&StylisticMaxEmptyLines, max.clone()).check_root(&[], &ctx_with_source(source));
    assert!(
      !d.is_empty(),
      "three empty lines after a multi-line comment should be flagged with max=2"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(0)).is_empty());
    assert!(invalid_options(NAME, serde_json::json!(2)).is_empty());
    assert!(invalid_options(NAME, serde_json::json!([2, {"ignore": ["comments"]}])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("2")),
      ["Invalid option \"2\" for rule \"@stylistic/max-empty-lines\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([2, {"ignore": ["selectors"]}])),
      ["Invalid value \"selectors\" for option \"ignore\" of rule \"@stylistic/max-empty-lines\""]
    );
  }
}
