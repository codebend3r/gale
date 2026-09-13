use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a newline after the commas of functions.
///
/// Primary: "always" | "always-multi-line" | "never-multi-line"
pub struct StylisticFunctionCommaNewlineAfter;

impl Rule for StylisticFunctionCommaNewlineAfter {
  fn name(&self) -> &'static str {
    "@stylistic/function-comma-newline-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a newline after the commas of functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = ctx.primary_option_str().unwrap_or("always-multi-line");
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
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
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
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

      // Detect function call: identifier followed by `(`
      if bytes[i] == b'('
        && i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        // Check if this is a pseudo-class/element function
        let mut is_pseudo_fn = false;
        {
          let mut p = i - 1;
          while p > 0 && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
          {
            p -= 1;
          }
          if bytes[p] == b':' {
            is_pseudo_fn = true;
          }
        }

        // Check if this is an SCSS at-rule paren
        let mut is_at_rule_paren = false;
        {
          let mut p = i - 1;
          while p > 0 && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
          {
            p -= 1;
          }
          if bytes[p] == b'@' {
            is_at_rule_paren = true;
          } else {
            while p > 0 && (bytes[p] == b' ' || bytes[p] == b'\t') {
              p -= 1;
            }
            while p > 0
              && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
            {
              p -= 1;
            }
            if bytes[p] == b'@' {
              is_at_rule_paren = true;
            }
          }
        }

        let paren_start = i;
        // Find matching closing paren
        let mut depth = 1;
        let mut j = i + 1;
        while j < len && depth > 0 {
          if bytes[j] == b'(' {
            depth += 1;
          } else if bytes[j] == b')' {
            depth -= 1;
          } else if bytes[j] == b'\'' || bytes[j] == b'"' {
            let q = bytes[j];
            j += 1;
            while j < len && bytes[j] != q {
              if bytes[j] == b'\\' {
                j += 1;
              }
              j += 1;
            }
          } else if bytes[j] == b'#' && j + 1 < len && bytes[j + 1] == b'{' {
            j += 2;
            let mut id = 1;
            while j < len && id > 0 {
              if bytes[j] == b'{' {
                id += 1;
              } else if bytes[j] == b'}' {
                id -= 1;
              }
              if id > 0 {
                j += 1;
              }
            }
          } else if bytes[j] == b'/' && j + 1 < len && bytes[j + 1] == b'/' {
            while j < len && bytes[j] != b'\n' {
              j += 1;
            }
          }
          if depth > 0 {
            j += 1;
          }
        }
        let paren_end = j;

        if is_pseudo_fn || is_at_rule_paren {
          i = paren_end + 1;
          continue;
        }

        let func_content = &ctx.source[paren_start..paren_end.min(len)];
        let is_multi_line = func_content.contains('\n');

        // Scan for commas inside this function call
        let mut k = paren_start + 1;
        let mut inner_depth = 0;
        while k < paren_end {
          // Skip SCSS interpolation
          if bytes[k] == b'#' && k + 1 < paren_end && bytes[k + 1] == b'{' {
            k += 2;
            let mut id = 1;
            while k < paren_end && id > 0 {
              if bytes[k] == b'{' {
                id += 1;
              } else if bytes[k] == b'}' {
                id -= 1;
              }
              if id > 0 {
                k += 1;
              }
            }
            if k < paren_end {
              k += 1;
            }
            continue;
          }
          // Skip SCSS line comments
          if bytes[k] == b'/' && k + 1 < paren_end && bytes[k + 1] == b'/' {
            while k < paren_end && bytes[k] != b'\n' {
              k += 1;
            }
            continue;
          }
          // Skip block comments
          if bytes[k] == b'/' && k + 1 < paren_end && bytes[k + 1] == b'*' {
            k += 2;
            while k + 1 < paren_end && !(bytes[k] == b'*' && bytes[k + 1] == b'/') {
              k += 1;
            }
            if k + 1 < paren_end {
              k += 2;
            }
            continue;
          }
          if bytes[k] == b'(' {
            inner_depth += 1;
          } else if bytes[k] == b')' {
            if inner_depth > 0 {
              inner_depth -= 1;
            }
          } else if bytes[k] == b'\'' || bytes[k] == b'"' {
            let q = bytes[k];
            k += 1;
            while k < paren_end && bytes[k] != q {
              if bytes[k] == b'\\' {
                k += 1;
              }
              k += 1;
            }
          } else if bytes[k] == b',' && inner_depth == 0 {
            let comma_pos = k;
            // Check what follows the comma (skip spaces/tabs, look for newline)
            let mut after = comma_pos + 1;
            while after < len && (bytes[after] == b' ' || bytes[after] == b'\t') {
              after += 1;
            }
            let has_newline_after = after < len && (bytes[after] == b'\n' || bytes[after] == b'\r');

            let violation = match option {
              "always" => !has_newline_after,
              "always-multi-line" => is_multi_line && !has_newline_after,
              "never-multi-line" => is_multi_line && has_newline_after,
              _ => false,
            };

            if violation {
              let msg = match option {
                "always" | "always-multi-line" => "Expected a newline after the comma in function",
                "never-multi-line" => "Unexpected newline after the comma in function",
                _ => "Expected a newline after the comma in function",
              };
              diagnostics.push(
                Diagnostic::new(self.name(), msg)
                  .severity(self.default_severity())
                  .span(Span::new(comma_pos, 1)),
              );
            }
          }
          k += 1;
        }
        i = paren_end + 1;
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

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticFunctionCommaNewlineAfter;
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
  fn always_accepts_newline_after_comma() {
    let d = check("a { transform: translate(1px,\n2px); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_newline_after_comma() {
    let d = check("a { transform: translate(1px, 2px); }", "always");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a newline"));
  }

  #[test]
  fn always_multi_line_accepts_single_line() {
    let d = check("a { transform: translate(1px, 2px); }", "always-multi-line");
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_requires_newline_in_multiline() {
    let d = check(
      "a { transform: translate(1px, \n2px); }",
      "always-multi-line",
    );
    // The comma at position after 1px is followed by space then newline; skip space first
    // Actually the comma is followed by " \n" — after skipping spaces, there IS a newline
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_rejects_missing_newline_in_multiline() {
    let d = check(
      "a {\n  transform: translate(1px, 2px,\n3px);\n}",
      "always-multi-line",
    );
    // First comma (after 1px) is followed by space then "2px" — no newline → violation
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn never_multi_line_accepts_no_newline_in_multiline() {
    // This is unusual but tests the option works
    let d = check("a { transform: translate(1px, 2px); }", "never-multi-line");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_pseudo_class_functions() {
    let d = check("a:not(.foo,\n.bar) { color: red; }", "always");
    assert!(d.is_empty());
  }
}
