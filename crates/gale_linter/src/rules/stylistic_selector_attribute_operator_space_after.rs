use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the operator within attribute selectors.
///
/// Equivalent to `@stylistic/selector-attribute-operator-space-after`.
pub struct StylisticSelectorAttributeOperatorSpaceAfter;

/// CSS attribute selector operators (longest first to match greedily).
const ATTR_OPERATORS: &[&str] = &["~=", "|=", "^=", "$=", "*=", "="];

impl Rule for StylisticSelectorAttributeOperatorSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/selector-attribute-operator-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the operator within attribute selectors"
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

      // Look for attribute selectors [...]
      if bytes[i] == b'[' {
        let bracket_start = i;
        i += 1;
        let mut bracket_depth = 1;

        // Find the content inside brackets
        while i < len && bracket_depth > 0 {
          if bytes[i] == b'[' {
            bracket_depth += 1;
          } else if bytes[i] == b']' {
            bracket_depth -= 1;
            if bracket_depth == 0 {
              break;
            }
          }
          i += 1;
        }

        let bracket_end = i;
        // Now scan the content between bracket_start+1 and bracket_end for operators
        let content = &bytes[bracket_start + 1..bracket_end.min(len)];
        let content_offset = bracket_start + 1;

        self.check_attr_content(content, content_offset, option, &mut diagnostics);

        if i < len {
          i += 1;
        }
        continue;
      }

      i += 1;
    }

    diagnostics
  }
}

impl StylisticSelectorAttributeOperatorSpaceAfter {
  fn check_attr_content(
    &self,
    content: &[u8],
    base_offset: usize,
    option: &str,
    diagnostics: &mut Vec<Diagnostic>,
  ) {
    let clen = content.len();
    let mut j = 0;

    while j < clen {
      // Try to match an operator
      let mut matched_op: Option<&str> = None;
      for op in ATTR_OPERATORS {
        let op_bytes = op.as_bytes();
        if j + op_bytes.len() <= clen && &content[j..j + op_bytes.len()] == op_bytes {
          matched_op = Some(op);
          break;
        }
      }

      if let Some(op) = matched_op {
        let op_end = j + op.len();
        let has_space_after =
          op_end < clen && (content[op_end] == b' ' || content[op_end] == b'\t');

        match option {
          "always" => {
            if !has_space_after {
              diagnostics.push(
                Diagnostic::new(self.name(), format!("Expected a space after \"{}\"", op))
                  .severity(self.default_severity())
                  .span(Span::new(base_offset + j, op.len())),
              );
            }
          }
          "never" => {
            if has_space_after {
              diagnostics.push(
                Diagnostic::new(self.name(), format!("Unexpected space after \"{}\"", op))
                  .severity(self.default_severity())
                  .span(Span::new(base_offset + j, op.len())),
              );
            }
          }
          _ => {}
        }

        j = op_end;
        continue;
      }

      j += 1;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn always_allows_space_after_operator() {
    let opt = serde_json::json!("always");
    let source = "[attr= value] { }";
    let d =
      StylisticSelectorAttributeOperatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_no_space_after_operator() {
    let opt = serde_json::json!("always");
    let source = "[attr=value] { }";
    let d =
      StylisticSelectorAttributeOperatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space after"));
  }

  #[test]
  fn never_allows_no_space_after() {
    let opt = serde_json::json!("never");
    let source = "[attr=value] { }";
    let d =
      StylisticSelectorAttributeOperatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn never_reports_space_after() {
    let opt = serde_json::json!("never");
    let source = "[attr= value] { }";
    let d =
      StylisticSelectorAttributeOperatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected space after"));
  }

  #[test]
  fn handles_complex_operators() {
    let opt = serde_json::json!("always");
    let source = "[class^=foo] { }";
    let d =
      StylisticSelectorAttributeOperatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("^="));
  }
}
