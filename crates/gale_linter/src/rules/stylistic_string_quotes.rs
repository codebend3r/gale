use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/string-quotes";

declare_rule! {
  /// Specify single or double quotes around strings.
  ///
  /// Primary: "single" | "double"
  pub struct StylisticStringQuotes => StylisticStringQuotesCheck {
    name: NAME,
    description: "Specify single or double quotes around strings",
    default_severity: Warning,
  }
}

/// `@stylistic/string-quotes` as configured.
pub struct StylisticStringQuotesCheck {
  /// The primary option: `"single"` or `"double"`.
  option: &'static str,
}

impl StylisticStringQuotesCheck {
  /// The plugin's options: `"single"` or `"double"`, then `avoidEscape`.  Gale
  /// reads no option as `"double"`.  Gale accepts but does not yet apply the
  /// secondary options.
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

impl Check for StylisticStringQuotesCheck {
  /// Flags strings using the quote character the option forbids.
  fn check(&self, _tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    let (bad_quote, good_name) = match option {
      "single" => (b'"', "single"),
      _ => (b'\'', "double"),
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

      // Skip SCSS interpolation #{...} — quotes inside are SCSS, not CSS
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

      // Detect url( and skip all content inside — Stylelint does not check
      // strings inside url() functions.
      if i + 4 <= len && bytes[i..i + 4].eq_ignore_ascii_case(b"url(") {
        i += 4;
        // Skip whitespace after url(
        while i < len && matches!(bytes[i], b' ' | b'\t') {
          i += 1;
        }
        if i < len && (bytes[i] == b'\'' || bytes[i] == b'"') {
          // Quoted url — skip to closing quote then closing paren
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
          // Skip to closing paren
          while i < len && bytes[i] != b')' {
            i += 1;
          }
          if i < len {
            i += 1;
          }
        } else {
          // Unquoted url — skip to closing paren
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
        // Walk to closing quote
        while i < len {
          if bytes[i] == b'\\' && i + 1 < len {
            i += 2;
            continue;
          }
          if bytes[i] == bad_quote {
            i += 1;
            break;
          }
          i += 1;
        }
        let span_len = i - start;
        diagnostics.push(
          Diagnostic::new(NAME, format!("Expected {good_name} quotes"))
            .span(Span::new(start, span_len)),
        );
        continue;
      } else if bytes[i] == b'\'' || bytes[i] == b'"' {
        // Skip over good quotes
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

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StylisticStringQuotes, opts.clone());
    let ctx = ctx_with_source(source);
    rule.check_source(&ctx)
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
  fn skips_comments() {
    let d = check("/* 'inside comment' */ a { content: \"ok\"; }", "double");
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("single")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("double")).is_empty());
    assert!(
      invalid_options(NAME, serde_json::json!(["single", {"avoidEscape": false}])).is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"@stylistic/string-quotes\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["single", {"avoidEscape": "yes"}])),
      ["Invalid value \"yes\" for option \"avoidEscape\" of rule \"@stylistic/string-quotes\""]
    );
  }
}
