use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify lowercase or uppercase for pseudo-element selectors.
///
/// Equivalent to Stylelint's `@stylistic/selector-pseudo-element-case` rule.
pub struct StylisticSelectorPseudoElementCase;

impl Rule for StylisticSelectorPseudoElementCase {
  fn name(&self) -> &'static str {
    "@stylistic/selector-pseudo-element-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for pseudo-element selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("lower");
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

      // Detect :: pseudo-elements
      if i + 1 < len && bytes[i] == b':' && bytes[i + 1] == b':' {
        let pseudo_start = i;
        i += 2; // skip ::

        // Read the pseudo-element name
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
          i += 1;
        }
        let name_end = i;

        if name_start == name_end {
          continue;
        }

        let name = &source[name_start..name_end];

        let is_wrong_case = match option {
          "lower" => name.chars().any(|c| c.is_ascii_uppercase()),
          "upper" => name.chars().any(|c| c.is_ascii_lowercase()),
          _ => false,
        };

        if is_wrong_case {
          let expected = match option {
            "lower" => name.to_ascii_lowercase(),
            "upper" => name.to_ascii_uppercase(),
            _ => name.to_string(),
          };
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Expected \"::{}\" to be \"::{}\"", name, expected),
            )
            .severity(self.default_severity())
            .span(Span::new(pseudo_start, name_end - pseudo_start)),
          );
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
  fn allows_lowercase_pseudo_element() {
    let opt = serde_json::Value::String("lower".to_string());
    let source = "a::before { }";
    let d = StylisticSelectorPseudoElementCase.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_uppercase_pseudo_element() {
    let opt = serde_json::Value::String("lower".to_string());
    let source = "a::Before { }";
    let d = StylisticSelectorPseudoElementCase.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("before"));
  }

  #[test]
  fn allows_uppercase_pseudo_element_with_upper() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a::AFTER { }";
    let d = StylisticSelectorPseudoElementCase.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_lowercase_when_upper_expected() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a::after { }";
    let d = StylisticSelectorPseudoElementCase.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("AFTER"));
  }
}
