use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space before the commas of selector lists.
///
/// Equivalent to `@stylistic/selector-list-comma-space-before`.
pub struct StylisticSelectorListCommaSpaceBefore;

impl Rule for StylisticSelectorListCommaSpaceBefore {
  fn name(&self) -> &'static str {
    "@stylistic/selector-list-comma-space-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space before the commas of selector lists"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("never");
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
              diagnostics.push(
                Diagnostic::new(self.name(), "Expected a space before \",\"")
                  .severity(self.default_severity())
                  .span(Span::new(i, 1)),
              );
            }
          }
          "never" => {
            if has_space_before {
              diagnostics.push(
                Diagnostic::new(self.name(), "Unexpected space before \",\"")
                  .severity(self.default_severity())
                  .span(Span::new(i, 1)),
              );
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
  use gale_css_parser::Syntax;

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticSelectorListCommaSpaceBefore;
    let opts = serde_json::json!(option);
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
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
}
