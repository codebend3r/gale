use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/selector-list-comma-space-before";

declare_rule! {
  /// Require or disallow a space before the commas of selector lists.
  ///
  /// Equivalent to `@stylistic/selector-list-comma-space-before`.
  pub struct StylisticSelectorListCommaSpaceBefore => StylisticSelectorListCommaSpaceBeforeCheck {
    name: NAME,
    description: "Require or disallow a space before the commas of selector lists",
    default_severity: Warning,
  }
}

/// `@stylistic/selector-list-comma-space-before` as configured.
pub struct StylisticSelectorListCommaSpaceBeforeCheck {
  /// The primary option: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.
  option: &'static str,
}

impl StylisticSelectorListCommaSpaceBeforeCheck {
  /// The plugin's options: `"always"`, `"never"`, `"always-single-line"` or
  /// `"never-single-line"`.  Gale reads no option as `"never"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option =
      options.primary_one_of(&["always", "never", "always-single-line", "never-single-line"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("never"),
    })
  }
}

impl Check for StylisticSelectorListCommaSpaceBeforeCheck {
  /// Flags a selector list comma whose preceding space does not match the option.
  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = self.option;
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut brace_depth = 0;

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

      if bytes[i] == b'{' {
        brace_depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b'}' {
        if brace_depth > 0 {
          brace_depth -= 1;
        }
        i += 1;
        continue;
      }

      // Only check commas in selectors (outside braces)
      if bytes[i] == b',' && brace_depth == 0 {
        let has_space_before = i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t');

        match option {
          "always" => {
            if !has_space_before {
              diagnostics
                .push(Diagnostic::new(NAME, "Expected a space before \",\"").span(Span::new(i, 1)));
            }
          }
          "never" => {
            if has_space_before {
              diagnostics
                .push(Diagnostic::new(NAME, "Unexpected space before \",\"").span(Span::new(i, 1)));
            }
          }
          _ => {}
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

  use crate::testing::{configured, ctx_with_source, invalid_options};

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let opts = serde_json::json!(option);
    let rule = configured(&StylisticSelectorListCommaSpaceBefore, opts.clone());
    let ctx = ctx_with_source(source);
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn never_accepts_no_space_before_comma() {
    let d = check("a,\nb { color: red; }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space_before_comma() {
    let d = check("a ,\nb { color: red; }", "never");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space"));
  }

  #[test]
  fn always_accepts_space_before_comma() {
    let d = check("a ,\nb { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_before_comma() {
    let d = check("a,\nb { color: red; }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("always")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("always-single-line")).is_empty());
    assert!(invalid_options(NAME, serde_json::json!("never-single-line")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/selector-list-comma-space-before\""
      ]
    );
  }
}
