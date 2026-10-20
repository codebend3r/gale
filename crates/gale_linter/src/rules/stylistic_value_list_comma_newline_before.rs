use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/value-list-comma-newline-before";

declare_rule! {
  /// Require a newline or disallow whitespace before the commas of value lists.
  ///
  /// Equivalent to `@stylistic/value-list-comma-newline-before`.
  pub struct StylisticValueListCommaNewlineBefore => StylisticValueListCommaNewlineBeforeCheck {
    name: NAME,
    description: "Require a newline or disallow whitespace before the commas of value lists",
    default_severity: Warning,
  }
}

/// `@stylistic/value-list-comma-newline-before` as configured.
pub struct StylisticValueListCommaNewlineBeforeCheck {
  /// The primary option: `"always"`, `"always-multi-line"`,
  /// `"never-multi-line"` or gale's own `"never"`.
  option: &'static str,
}

impl StylisticValueListCommaNewlineBeforeCheck {
  /// The plugin's options: `"always"`, `"always-multi-line"` or
  /// `"never-multi-line"`.  Gale also accepts `"never"`, which the plugin does
  /// not.  Gale reads no option as `"never"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "always-multi-line", "never-multi-line", "never"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticValueListCommaNewlineBeforeCheck {
  /// Flags a value list comma whose preceding newline does not match the option.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut in_value = false;
    let mut paren_depth = 0;
    let mut value_start = 0;

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
        i += 1;
        continue;
      }

      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
        continue;
      }

      if bytes[i] == b':' && !in_value && paren_depth == 0 {
        in_value = true;
        value_start = i + 1;
        i += 1;
        continue;
      }

      if bytes[i] == b'(' {
        paren_depth += 1;
        i += 1;
        continue;
      }

      if bytes[i] == b')' {
        if paren_depth > 0 {
          paren_depth -= 1;
        }
        i += 1;
        continue;
      }

      if (bytes[i] == b';' || bytes[i] == b'}') && paren_depth == 0 {
        in_value = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'{' {
        in_value = false;
        i += 1;
        continue;
      }

      // Check commas in value lists (not inside function parens)
      if bytes[i] == b',' && in_value && paren_depth == 0 {
        let comma_pos = i;

        // Check for newline before the comma
        let has_newline_before = if comma_pos > 0 {
          let mut j = comma_pos - 1;
          while j > 0 && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j -= 1;
          }
          bytes[j] == b'\n' || bytes[j] == b'\r'
        } else {
          false
        };

        // For always-multi-line, check if the value list is multi-line
        let is_multi_line = if option.ends_with("-multi-line") {
          let mut ve = comma_pos + 1;
          while ve < len && bytes[ve] != b';' && bytes[ve] != b'}' {
            ve += 1;
          }
          source[value_start..ve.min(len)].contains('\n')
        } else {
          true
        };

        let violation = match option {
          "always" => !has_newline_before,
          "never" => has_newline_before,
          "always-multi-line" => is_multi_line && !has_newline_before,
          "never-multi-line" => {
            is_multi_line && comma_pos > 0 && bytes[comma_pos - 1].is_ascii_whitespace()
          }
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-multi-line" => "Expected newline before \",\"",
            "never" => "Unexpected whitespace before \",\"",
            "never-multi-line" => "Unexpected whitespace before \",\" in a multi-line list",
            _ => continue,
          };
          diagnostics.push(Diagnostic::new(NAME, msg).span(Span::new(comma_pos, 1)));
        }
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
    let rule = configured(&StylisticValueListCommaNewlineBefore, opts.clone());
    let ctx = ctx_with_source_and_options(source, &opts);
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn always_accepts_newline_before_comma() {
    let d = check("a { font-family: Arial\n, sans-serif; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_newline_before_comma() {
    let d = check("a { font-family: Arial, sans-serif; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn never_accepts_no_newline_before_comma() {
    let d = check("a { font-family: Arial, sans-serif; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_newline_before_comma() {
    let d = check("a { font-family: Arial\n, sans-serif; }", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn always_multi_line_allows_single_line() {
    let d = check("a { font-family: Arial, sans-serif; }", "always-multi-line");
    assert!(d.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-multi-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-multi-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/value-list-comma-newline-before\""
      ]
    );
  }

  #[test]
  fn never_multi_line_checks_multi_line_lists_only() {
    use gale_css_parser::Syntax;

    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-multi-line"),
      "a { transition: a 1s ,b 1s\n  , c 1s; }",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
      messages,
      [
        "Unexpected whitespace before \",\" in a multi-line list",
        "Unexpected whitespace before \",\" in a multi-line list"
      ] as [&str; 2]
    );
    let d = crate::testing::lint(
      NAME,
      serde_json::json!("never-multi-line"),
      "a { transition: a 1s ,b 1s; }",
      Syntax::Css,
    );
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, [] as [&str; 0]);
  }
}
