use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "string-quotes";

declare_rule! {
  /// Specify single or double quotes around strings.
  ///
  /// Equivalent to Stylelint's deprecated `string-quotes` rule (now
  /// `@stylistic/string-quotes`). Both rules share the same logic.
  ///
  /// Primary option: `"single"` | `"double"` (default `"double"`).
  pub struct StringQuotes => StringQuotesCheck {
    name: NAME,
    description: "Specify single or double quotes around strings",
    default_severity: Warning,
  }
}

/// `string-quotes` as configured.
pub struct StringQuotesCheck {
  /// The primary option: `"single"` or `"double"`.
  option: &'static str,
}

impl StringQuotesCheck {
  /// Stylelint's options: `"single"` or `"double"`, then `avoidEscape`,
  /// which gale accepts but does not yet apply (it reports as if it were
  /// `false`).  Gale reads no option as `"double"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["single", "double"]);
    options.flag("avoidEscape");
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("double"),
    })
  }
}

impl Check for StringQuotesCheck {
  /// Flags strings using the quote character the option forbids.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    let (bad_quote, good_quote, good_name) = match option {
      "single" => (b'"', b'\'', "single"),
      _ => (b'\'', b'"', "double"),
    };

    while i < len {
      // Skip block comments
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

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip SCSS interpolation #{...}
      if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut depth = 1;
        while i < len && depth > 0 {
          if bytes[i] == b'{' {
            depth += 1;
          } else if bytes[i] == b'}' {
            depth -= 1;
          }
          if depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip url() content — Stylelint doesn't check strings inside url()
      if i + 4 <= len && bytes[i..i + 4].eq_ignore_ascii_case(b"url(") {
        i += 4;
        while i < len && matches!(bytes[i], b' ' | b'\t') {
          i += 1;
        }
        if i < len && (bytes[i] == b'\'' || bytes[i] == b'"') {
          let url_quote = bytes[i];
          i += 1;
          while i < len {
            if bytes[i] == b'\\' && i + 1 < len {
              i += 2;
              continue;
            }
            if bytes[i] == url_quote {
              i += 1;
              break;
            }
            i += 1;
          }
          while i < len && bytes[i] != b')' {
            i += 1;
          }
          if i < len {
            i += 1;
          }
        } else {
          let mut depth = 1;
          while i < len && depth > 0 {
            if bytes[i] == b'(' {
              depth += 1;
            } else if bytes[i] == b')' {
              depth -= 1;
            }
            i += 1;
          }
        }
        continue;
      }

      if bytes[i] == bad_quote {
        let start = i;
        i += 1;
        let mut contents = Vec::new();
        while i < len {
          if bytes[i] == b'\\' && i + 1 < len {
            contents.push(bytes[i]);
            contents.push(bytes[i + 1]);
            i += 2;
            continue;
          }
          if bytes[i] == bad_quote {
            i += 1;
            break;
          }
          contents.push(bytes[i]);
          i += 1;
        }
        let span_len = i - start;

        // Build replacement with escaped quotes
        let inner = String::from_utf8_lossy(&contents);
        let escaped = if good_quote == b'"' {
          inner.replace('"', "\\\"")
        } else {
          inner.replace('\'', "\\'")
        };
        let replacement = format!("{}{}{}", good_quote as char, escaped, good_quote as char);

        diagnostics.push(
          Diagnostic::new(NAME, format!("Expected {good_name} quotes"))
            .span(Span::new(start, span_len))
            .fix(Fix::new(
              format!("Replace with {good_name} quotes"),
              vec![Edit::new(Span::new(start, span_len), &replacement)],
            )),
        );
        continue;
      } else if bytes[i] == b'\'' || bytes[i] == b'"' {
        let quote = bytes[i];
        i += 1;
        while i < len {
          if bytes[i] == b'\\' && i + 1 < len {
            i += 2;
            continue;
          }
          if bytes[i] == quote {
            i += 1;
            break;
          }
          i += 1;
        }
        continue;
      }
      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{configured, ctx_with_source_and_options, invalid_options};

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StringQuotes, opts.clone());
    let ctx = ctx_with_source_and_options(source, &opts);
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn double_accepts_double_quotes() {
    let d = check("a { content: \"hello\"; }", "double");
    assert!(d.is_empty());
  }

  #[test]
  fn double_rejects_single_quotes() {
    let d = check("a { content: 'hello'; }", "double");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("double"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn single_accepts_single_quotes() {
    let d = check("a { content: 'hello'; }", "single");
    assert!(d.is_empty());
  }

  #[test]
  fn single_rejects_double_quotes() {
    let d = check("a { content: \"hello\"; }", "single");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("single"));
  }

  #[test]
  fn skips_url_content() {
    let d = check(
      "a { background: url('data:image/svg+xml;utf8,<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>'); }",
      "single",
    );
    assert!(d.is_empty());
  }

  #[test]
  fn skips_comments() {
    let d = check("/* 'inside comment' */ a { content: \"ok\"; }", "double");
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["single", { "avoidEscape": false }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("backtick")),
      ["Invalid option value \"backtick\" for rule \"string-quotes\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["double", { "avoidEscape": "yes" }])
      ),
      ["Invalid value \"yes\" for option \"avoidEscape\" of rule \"string-quotes\""]
    );
  }
}
