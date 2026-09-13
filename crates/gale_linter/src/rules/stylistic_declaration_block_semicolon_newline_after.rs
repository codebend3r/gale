use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require a newline after the semicolon of declaration blocks.
///
/// Equivalent to `@stylistic/declaration-block-semicolon-newline-after`.
pub struct StylisticDeclarationBlockSemicolonNewlineAfter;

impl Rule for StylisticDeclarationBlockSemicolonNewlineAfter {
  fn name(&self) -> &'static str {
    "@stylistic/declaration-block-semicolon-newline-after"
  }

  fn description(&self) -> &'static str {
    "Require a newline after the semicolon of declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("always");
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

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip url() contents — they can contain semicolons (e.g., data URIs)
      if i + 4 <= len && bytes[i..].starts_with(b"url(") {
        i += 4;
        // Find the matching closing paren, handling nested parens
        let mut paren_depth = 1;
        while i < len && paren_depth > 0 {
          if bytes[i] == b'(' {
            paren_depth += 1;
          } else if bytes[i] == b')' {
            paren_depth -= 1;
          }
          i += 1;
        }
        continue;
      }

      // Check semicolons inside blocks — but only those that end
      // declarations (property: value;), not SCSS at-rules (@include;).
      if bytes[i] == b';' && depth > 0 {
        let semi_pos = i;

        // Check if this semicolon ends an at-rule (e.g. @include, @import).
        // Scan backwards to find the start of the statement, skipping over
        // nested blocks (e.g., @include foo { ... };)
        let mut is_at_rule = false;
        {
          let mut k = semi_pos;
          let mut inner_depth: i32 = 0;
          while k > 0 {
            k -= 1;
            if bytes[k] == b'}' {
              inner_depth += 1;
            } else if bytes[k] == b'{' {
              if inner_depth > 0 {
                inner_depth -= 1;
              } else {
                // Hit the opening brace of the enclosing block
                break;
              }
            } else if inner_depth == 0 {
              if bytes[k] == b';' {
                break;
              }
              if bytes[k] == b'@' {
                is_at_rule = true;
                break;
              }
            }
          }
        }
        if is_at_rule {
          i += 1;
          continue;
        }

        // Skip semicolons immediately after a closing brace (end of at-rule content block)
        // e.g., `@include mixin { ... };`
        {
          let mut k = semi_pos;
          while k > 0 {
            k -= 1;
            if bytes[k] == b' ' || bytes[k] == b'\t' || bytes[k] == b'\n' || bytes[k] == b'\r' {
              continue;
            }
            if bytes[k] == b'}' {
              // Semicolon follows a block, skip
              is_at_rule = true;
            }
            break;
          }
        }
        if is_at_rule {
          i += 1;
          continue;
        }

        // Find the next non-whitespace character (excluding newlines for checking).
        // SCSS line comments (`//`) count as "rest of line" so if we see
        // `; // comment\n`, the newline IS there.
        // Block comments (`/* ... */`) are also skipped: `; /* comment */\n`
        // is treated as having a newline after `;`.
        let mut j = i + 1;
        let mut found_newline = false;
        while j < len && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\r') {
          j += 1;
        }
        // Skip inline block comment /* ... */ then re-check
        if j + 1 < len && bytes[j] == b'/' && bytes[j + 1] == b'*' {
          let mut c = j + 2;
          while c + 1 < len && !(bytes[c] == b'*' && bytes[c + 1] == b'/') {
            c += 1;
          }
          if c + 1 < len {
            c += 2; // skip */
          }
          // Skip trailing spaces/tabs after comment
          while c < len && (bytes[c] == b' ' || bytes[c] == b'\t') {
            c += 1;
          }
          if c < len && (bytes[c] == b'\n' || bytes[c] == b'\r') {
            found_newline = true;
          }
          // Also update j so the } check below uses the right position
          if found_newline {
            j = c;
          }
        }
        if j < len && bytes[j] == b'\n' {
          found_newline = true;
        }
        // SCSS line comment after `;` — the newline at end of comment counts
        if j + 1 < len && bytes[j] == b'/' && bytes[j + 1] == b'/' {
          found_newline = true;
        }
        // If we reached } it's fine (last declaration)
        if j < len && bytes[j] == b'}' {
          i += 1;
          continue;
        }
        // If end of source, fine
        if j >= len {
          i += 1;
          continue;
        }

        let is_multi_line = if option == "always-multi-line" {
          // Check if the block is multi-line: scan from the opening { to closing }
          is_block_multiline(bytes, semi_pos)
        } else {
          true
        };

        let should_check = option == "always" || (option == "always-multi-line" && is_multi_line);

        if should_check && !found_newline {
          diagnostics.push(
            Diagnostic::new(self.name(), "Expected newline after \";\"")
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
  fn always_allows_newline_after_semicolon() {
    let opt = serde_json::json!("always");
    let source = "a {\n  color: red;\n  display: block;\n}";
    let d = StylisticDeclarationBlockSemicolonNewlineAfter
      .check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "got: {:?}",
      d.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
  }

  #[test]
  fn always_reports_missing_newline() {
    let opt = serde_json::json!("always");
    let source = "a { color: red; display: block; }";
    let d = StylisticDeclarationBlockSemicolonNewlineAfter
      .check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Expected newline"));
  }

  #[test]
  fn always_multi_line_allows_single_line() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a { color: red; display: block; }";
    let d = StylisticDeclarationBlockSemicolonNewlineAfter
      .check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn always_multi_line_reports_in_multiline_block() {
    let opt = serde_json::json!("always-multi-line");
    let source = "a {\n  color: red; display: block;\n}";
    let d = StylisticDeclarationBlockSemicolonNewlineAfter
      .check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
  }

  #[test]
  fn skips_at_include_with_content_block() {
    let opt = serde_json::json!("always");
    // @include with a content block ending in ;
    let source = "a {\n  @include mixin { color: red; };\n  display: block;\n}";
    let d = StylisticDeclarationBlockSemicolonNewlineAfter
      .check_root(&[], &ctx_with_option(source, &opt));
    assert!(
      d.is_empty(),
      "Should not flag semicolon after @include content block, got: {:?}",
      d.iter()
        .map(|d| (&d.message, d.span.offset))
        .collect::<Vec<_>>()
    );
  }

  #[test]
  fn skips_at_include_semicolon() {
    let opt = serde_json::json!("always");
    let source = "a {\n  @include mixin;\n  color: red;\n}";
    let d = StylisticDeclarationBlockSemicolonNewlineAfter
      .check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }
}
