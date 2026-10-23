use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/no-eol-whitespace";

declare_rule! {
  /// Disallow end-of-line whitespace.
  ///
  /// Equivalent to `@stylistic/no-eol-whitespace`.
  pub struct StylisticNoEolWhitespace => StylisticNoEolWhitespaceCheck {
    name: NAME,
    description: "Disallow end-of-line whitespace",
    default_severity: Warning,
  }
}

/// `@stylistic/no-eol-whitespace` as configured.
pub struct StylisticNoEolWhitespaceCheck {
  /// `ignore: ["empty-lines"]`: a line holding only whitespace is allowed.
  ignore_empty_lines: bool,
}

impl StylisticNoEolWhitespaceCheck {
  /// The plugin's options: `true`, then `ignore: ["empty-lines"]`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore = options.keywords("ignore", &["empty-lines"]);
    options.finish()?;
    Ok(Self {
      ignore_empty_lines: ignore.contains(&"empty-lines"),
    })
  }

  /// Whether the whitespace run `start..end` is a problem: with
  /// `ignore: ["empty-lines"]` a run that fills its whole line is not.
  fn flags(&self, bytes: &[u8], start: usize, end: usize) -> bool {
    start < end && !(self.ignore_empty_lines && (start == 0 || bytes[start - 1] == b'\n'))
  }
}

impl Check for StylisticNoEolWhitespaceCheck {
  /// Flags whitespace at the end of a line, skipping string literals.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;

    while i < len {
      // Skip strings
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        continue;
      }

      if bytes[i] == b'\n' {
        // Check for trailing whitespace before this newline
        let trail_end = i;
        let mut trail_start = i;
        while trail_start > 0 && (bytes[trail_start - 1] == b' ' || bytes[trail_start - 1] == b'\t')
        {
          trail_start -= 1;
        }
        if self.flags(bytes, trail_start, trail_end) {
          diagnostics.push(
            Diagnostic::new(NAME, "Unexpected end-of-line whitespace")
              .span(Span::new(trail_start, trail_end - trail_start)),
          );
        }
      }

      i += 1;
    }

    // Also check the last line if it doesn't end with \n
    if len > 0 && bytes[len - 1] != b'\n' {
      let mut trail_start = len;
      while trail_start > 0 && (bytes[trail_start - 1] == b' ' || bytes[trail_start - 1] == b'\t') {
        trail_start -= 1;
      }
      if self.flags(bytes, trail_start, len) {
        diagnostics.push(
          Diagnostic::new(NAME, "Unexpected end-of-line whitespace")
            .span(Span::new(trail_start, len - trail_start)),
        );
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_no_trailing_whitespace() {
    let source = "a {\n  color: red;\n}\n";
    let d = configured(&StylisticNoEolWhitespace, None).check_source(&ctx_with_source(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_trailing_spaces() {
    let source = "a {  \n  color: red;\n}\n";
    let d = configured(&StylisticNoEolWhitespace, None).check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("end-of-line whitespace"));
  }

  #[test]
  fn ignore_empty_lines_allows_whitespace_only_lines() {
    let source = "a {\n  \n  color: red; \n\t\n}\n  ";
    let all = configured(&StylisticNoEolWhitespace, None).check_source(&ctx_with_source(source));
    assert_eq!(all.len(), 4);
    let options = serde_json::json!([true, { "ignore": ["empty-lines"] }]);
    let d = configured(&StylisticNoEolWhitespace, options).check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span.offset, 20);
  }

  #[test]
  fn reports_trailing_tab() {
    let source = "a {\t\n  color: red;\n}\n";
    let d = configured(&StylisticNoEolWhitespace, None).check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_multiple_lines() {
    let source = "a { \n  color: red; \n}\n";
    let d = configured(&StylisticNoEolWhitespace, None).check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 2);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert!(
      invalid_options(NAME, serde_json::json!([true, {"ignore": ["empty-lines"]}])).is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"@stylistic/no-eol-whitespace\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, {"ignore": ["comments"]}])),
      ["Invalid value \"comments\" for option \"ignore\" of rule \"@stylistic/no-eol-whitespace\""]
    );
  }
}
