use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify lowercase or uppercase for pseudo-class selectors.
///
/// Equivalent to `@stylistic/selector-pseudo-class-case`.
pub struct StylisticSelectorPseudoClassCase;

impl Rule for StylisticSelectorPseudoClassCase {
  fn name(&self) -> &'static str {
    "@stylistic/selector-pseudo-class-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for pseudo-class selectors"
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
    let mut in_selector = true;

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
        in_selector = false;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' || bytes[i] == b';' {
        in_selector = true;
        i += 1;
        continue;
      }

      // Detect pseudo-class (single colon, not double colon for pseudo-elements)
      if bytes[i] == b':' && in_selector {
        // Check for double colon (pseudo-element) -- skip those
        if i + 1 < len && bytes[i + 1] == b':' {
          i += 2;
          // Skip past pseudo-element name
          while i < len
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
          {
            i += 1;
          }
          continue;
        }

        let colon_pos = i;
        i += 1;

        // Read the pseudo-class name
        let name_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
        {
          i += 1;
        }
        let name_end = i;

        if name_start == name_end {
          continue;
        }

        let name = &source[name_start..name_end];

        let is_wrong = match option {
          "lower" => name.chars().any(|c| c.is_ascii_uppercase()),
          "upper" => name.chars().any(|c| c.is_ascii_lowercase()),
          _ => false,
        };

        if is_wrong {
          let expected = if option == "lower" {
            "lowercase"
          } else {
            "uppercase"
          };
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Expected pseudo-class \":{}\" to be {}", name, expected),
            )
            .severity(self.default_severity())
            .span(Span::new(colon_pos, name_end - colon_pos)),
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

  fn ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn allows_lowercase_pseudo_class() {
    let source = "a:hover { }";
    let d = StylisticSelectorPseudoClassCase.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_uppercase_pseudo_class() {
    let source = "a:Hover { }";
    let d = StylisticSelectorPseudoClassCase.check_root(&[], &ctx(source));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("lowercase"));
  }

  #[test]
  fn allows_uppercase_with_upper_option() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a:HOVER { }";
    let d = StylisticSelectorPseudoClassCase.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_lowercase_with_upper_option() {
    let opt = serde_json::Value::String("upper".to_string());
    let source = "a:hover { }";
    let d = StylisticSelectorPseudoClassCase.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("uppercase"));
  }

  #[test]
  fn does_not_flag_pseudo_elements() {
    let source = "a::Before { }";
    let d = StylisticSelectorPseudoClassCase.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }

  #[test]
  fn skips_comments() {
    let source = "/* a:Hover {} */ a:hover { }";
    let d = StylisticSelectorPseudoClassCase.check_root(&[], &ctx(source));
    assert!(d.is_empty());
  }
}
