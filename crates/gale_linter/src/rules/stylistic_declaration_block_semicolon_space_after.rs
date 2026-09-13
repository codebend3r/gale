use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the semicolons of declaration blocks.
///
/// Equivalent to `@stylistic/declaration-block-semicolon-space-after`.
pub struct StylisticDeclarationBlockSemicolonSpaceAfter;

impl Rule for StylisticDeclarationBlockSemicolonSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-block-semicolon-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the semicolons of declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("always-single-line");
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut depth: i32 = 0;

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

      if bytes[i] == b'{' {
        depth += 1;
        i += 1;
        continue;
      }

      if bytes[i] == b'}' {
        depth -= 1;
        if depth < 0 {
          depth = 0;
        }
        i += 1;
        continue;
      }

      // Check semicolons inside blocks
      if bytes[i] == b';' && depth > 0 {
        let semi_pos = i;

        // Look at the character after the semicolon (skip whitespace for analysis)
        let j = i + 1;

        // If next non-whitespace is `}`, skip (last declaration)
        let mut peek = j;
        while peek < len
          && (bytes[peek] == b' '
            || bytes[peek] == b'\t'
            || bytes[peek] == b'\n'
            || bytes[peek] == b'\r')
        {
          peek += 1;
        }
        if peek >= len || bytes[peek] == b'}' {
          i += 1;
          continue;
        }

        // Check the immediate character after `;`
        let has_space = j < len && bytes[j] == b' ';

        let is_single_line = !is_block_multiline(bytes, semi_pos);

        let violation = match option {
          "always" => !has_space,
          "never" => has_space,
          "always-single-line" => is_single_line && !has_space,
          "never-single-line" => is_single_line && has_space,
          _ => false,
        };

        if violation {
          let msg = match option {
            "always" | "always-single-line" => "Expected single space after \";\"",
            "never" | "never-single-line" => "Unexpected whitespace after \";\"",
            _ => unreachable!(),
          };
          diagnostics.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(semi_pos, 1)),
          );
        }
      }

      i += 1;
    }

    diagnostics
  }
}

/// Check if the enclosing block around `pos` is multi-line.
fn is_block_multiline(bytes: &[u8], pos: usize) -> bool {
  // Find the opening { before pos
  let mut brace_depth = 0;
  let mut k = pos;
  let mut open_brace = None;
  while k > 0 {
    k -= 1;
    if bytes[k] == b'}' {
      brace_depth += 1;
    } else if bytes[k] == b'{' {
      if brace_depth == 0 {
        open_brace = Some(k);
        break;
      }
      brace_depth -= 1;
    }
  }

  if let Some(open) = open_brace {
    // Find the closing }
    let mut depth = 1;
    let mut j = open + 1;
    while j < bytes.len() && depth > 0 {
      if bytes[j] == b'{' {
        depth += 1;
      } else if bytes[j] == b'}' {
        depth -= 1;
      }
      if depth > 0 {
        j += 1;
      }
    }
    // Check if there's a newline between open and close
    for b in &bytes[open..j] {
      if *b == b'\n' {
        return true;
      }
    }
  }
  false
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
  fn always_single_line_allows_space_after_semicolon() {
    let opt = serde_json::json!("always-single-line");
    let source = "a { color: red; display: block; }";
    let d =
      StylisticDeclarationBlockSemicolonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn always_single_line_reports_missing_space() {
    let opt = serde_json::json!("always-single-line");
    let source = "a { color: red;display: block; }";
    let d =
      StylisticDeclarationBlockSemicolonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected single space"));
  }

  #[test]
  fn always_single_line_ignores_multiline_block() {
    let opt = serde_json::json!("always-single-line");
    let source = "a {\n  color: red;display: block;\n}";
    let d =
      StylisticDeclarationBlockSemicolonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "should not report in multi-line block, got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn never_reports_space_after_semicolon() {
    let opt = serde_json::json!("never");
    let source = "a {\n  color: red; display: block;\n}";
    let d =
      StylisticDeclarationBlockSemicolonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected whitespace"));
  }

  #[test]
  fn always_requires_space_after_semicolon() {
    let opt = serde_json::json!("always");
    let source = "a { color: red;display: block; }";
    let d =
      StylisticDeclarationBlockSemicolonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected single space"));
  }

  #[test]
  fn skips_last_declaration_before_closing_brace() {
    let opt = serde_json::json!("always");
    let source = "a { color: red;}";
    let d =
      StylisticDeclarationBlockSemicolonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "should skip semicolon before }}, got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn never_single_line_reports_space_in_single_line() {
    let opt = serde_json::json!("never-single-line");
    let source = "a { color: red; display: block; }";
    let d =
      StylisticDeclarationBlockSemicolonSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected whitespace"));
  }
}
