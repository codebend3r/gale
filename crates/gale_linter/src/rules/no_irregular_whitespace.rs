use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-irregular-whitespace";

declare_rule! {
  /// Reports irregular whitespace characters such as non-breaking spaces,
  /// zero-width spaces, and other Unicode whitespace oddities.
  ///
  /// Equivalent to Stylelint's `no-irregular-whitespace` rule.
  pub struct NoIrregularWhitespace => NoIrregularWhitespaceCheck {
    name: NAME,
    description: "Disallow irregular whitespace characters",
    default_severity: Warning,
  }
}

/// `no-irregular-whitespace`, which takes no options.
pub struct NoIrregularWhitespaceCheck;

impl NoIrregularWhitespaceCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

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

impl Check for NoIrregularWhitespaceCheck {
  /// Flags irregular Unicode whitespace outside comments, which Stylelint allows.
  fn check(&self, _tree: &PostcssTree, context: &RuleContext) -> Vec<Diagnostic> {
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
            NAME,
            format!(
              "Unexpected irregular whitespace character (U+{:04X})",
              ch as u32
            ),
          )
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

  use crate::testing::{CheckSource, configured, ctx_with_source};

  #[test]
  fn reports_non_breaking_space() {
    let rule = configured(&NoIrregularWhitespace, None);
    let source = "a {\u{00A0}color: red; }";
    let context = ctx_with_source(source);
    let diags = rule.check_source(&context);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("U+00A0"));
  }

  #[test]
  fn reports_zero_width_space() {
    let rule = configured(&NoIrregularWhitespace, None);
    let source = "a { color:\u{200B}red; }";
    let context = ctx_with_source(source);
    let diags = rule.check_source(&context);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("U+200B"));
  }

  #[test]
  fn ignores_normal_whitespace() {
    let rule = configured(&NoIrregularWhitespace, None);
    let context = ctx_with_source("a { color: red; }");
    let diags = rule.check_source(&context);
    assert!(diags.is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options("no-irregular-whitespace", serde_json::json!(true)).is_empty());
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "no-irregular-whitespace",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options("no-irregular-whitespace", serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"no-irregular-whitespace\""]
    );
  }
}
