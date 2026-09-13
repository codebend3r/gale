use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports irregular whitespace characters such as non-breaking spaces,
/// zero-width spaces, and other Unicode whitespace oddities.
///
/// Equivalent to Stylelint's `no-irregular-whitespace` rule.
pub struct NoIrregularWhitespace;

/// Find byte ranges of comments in CSS/SCSS source.
/// Returns a list of (start, end) byte offset pairs.
/// Handles both block comments (`/* ... */`) and single-line comments (`// ...`).
fn find_comment_ranges(source: &str) -> Vec<(usize, usize)> {
  let mut ranges = Vec::new();
  let bytes = source.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  // Track if we're inside a string to avoid matching comment delimiters in strings.
  let mut in_string: Option<u8> = None;

  while i < len {
    if let Some(quote) = in_string {
      if bytes[i] == b'\\' {
        i += 2; // skip escaped char
        continue;
      }
      if bytes[i] == quote {
        in_string = None;
      }
      i += 1;
      continue;
    }

    if bytes[i] == b'"' || bytes[i] == b'\'' {
      in_string = Some(bytes[i]);
      i += 1;
      continue;
    }

    if i + 1 < len && bytes[i] == b'/' {
      if bytes[i + 1] == b'*' {
        // Block comment
        let start = i;
        i += 2;
        while i + 1 < len {
          if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            i += 2;
            break;
          }
          i += 1;
        }
        ranges.push((start, i));
        continue;
      } else if bytes[i + 1] == b'/' {
        // Single-line comment (SCSS/Less)
        let start = i;
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        ranges.push((start, i));
        continue;
      }
    }

    i += 1;
  }

  ranges
}

/// Characters considered irregular whitespace.
const IRREGULAR_WHITESPACE_CHARS: &[char] = &[
  '\u{00A0}', // Non-breaking space
  '\u{1680}', // Ogham space mark
  '\u{2000}', // En quad
  '\u{2001}', // Em quad
  '\u{2002}', // En space
  '\u{2003}', // Em space
  '\u{2004}', // Three-per-em space
  '\u{2005}', // Four-per-em space
  '\u{2006}', // Six-per-em space
  '\u{2007}', // Figure space
  '\u{2008}', // Punctuation space
  '\u{2009}', // Thin space
  '\u{200A}', // Hair space
  '\u{200B}', // Zero-width space
  '\u{202F}', // Narrow no-break space
  '\u{205F}', // Medium mathematical space
  '\u{3000}', // Ideographic space
  '\u{FEFF}', // Zero-width no-break space (BOM)
];

impl Rule for NoIrregularWhitespace {
  fn name(&self) -> &'static str {
    "no-irregular-whitespace"
  }

  fn description(&self) -> &'static str {
    "Disallow irregular whitespace characters"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Build a set of byte ranges that are inside comments so we can skip them.
    // Stylelint does not flag irregular whitespace inside comments.
    let comment_ranges = find_comment_ranges(context.source);

    for (i, ch) in context.source.char_indices() {
      if IRREGULAR_WHITESPACE_CHARS.contains(&ch) {
        // Skip if inside a comment
        if comment_ranges
          .iter()
          .any(|&(start, end)| i >= start && i < end)
        {
          continue;
        }
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!(
              "Unexpected irregular whitespace character (U+{:04X})",
              ch as u32
            ),
          )
          .severity(self.default_severity())
          .span(Span::new(i, ch.len_utf8())),
        );
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  #[test]
  fn reports_non_breaking_space() {
    let rule = NoIrregularWhitespace;
    let source = "a {\u{00A0}color: red; }";
    let context = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("U+00A0"));
  }

  #[test]
  fn reports_zero_width_space() {
    let rule = NoIrregularWhitespace;
    let source = "a { color:\u{200B}red; }";
    let context = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("U+200B"));
  }

  #[test]
  fn ignores_normal_whitespace() {
    let rule = NoIrregularWhitespace;
    let context = RuleContext {
      file_path: "test.css",
      source: "a { color: red; }",
      syntax: Syntax::Css,
      options: None,
    };
    let diags = rule.check_root(&[], &context);
    assert!(diags.is_empty());
  }
}
