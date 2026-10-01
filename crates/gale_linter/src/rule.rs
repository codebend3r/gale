use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity};

/// Context passed to each rule when checking a node.
pub struct RuleContext<'a> {
  /// The path to the file being linted.
  pub file_path: &'a str,
  /// The full source text of the file.
  pub source: &'a str,
  /// The CSS syntax variant.
  pub syntax: Syntax,
  /// Per-rule options from the config (e.g. max value, ignore lists).
  pub options: Option<&'a serde_json::Value>,
}

impl<'a> RuleContext<'a> {
  /// Extract the **primary option** from Stylelint-format options.
  ///
  /// Options may arrive in several shapes depending on how the config was
  /// written and resolved:
  ///
  /// - A plain value (string, number, bool): `"always"`, `4`, `true`
  /// - An array `[primary, secondary]`: `["always", { "except": [...] }]`
  ///
  /// This method normalises all forms and returns the primary option value.
  pub fn primary_option(&self) -> Option<&'a serde_json::Value> {
    let value = self.options?;
    match value {
      serde_json::Value::Array(arr) => arr.first(),
      other => Some(other),
    }
  }

  /// Extract the primary option as a `&str`, handling both plain strings
  /// and array-format options `["value", { ... }]`.
  pub fn primary_option_str(&self) -> Option<&'a str> {
    self.primary_option().and_then(|v| v.as_str())
  }

  /// Extract the **secondary options object** from Stylelint-format options.
  ///
  /// Present when options are in array form `[primary, secondary]` (returns
  /// the second element), OR when options is a bare object (returned directly,
  /// e.g. from preset configs that store `{"ignore": [...]}` without a
  /// primary option wrapper).
  pub fn secondary_options(&self) -> Option<&'a serde_json::Value> {
    secondary_options_of(self.options?)
  }

  /// `source[start..end]`, or `None` when the range is out of bounds,
  /// reversed, or splits a multibyte character.
  ///
  /// Offsets a rule computes (a node span plus the length of some AST text,
  /// say) do not always line up with the source the author wrote, and a
  /// plain slice panics the moment one lands inside `é` or `😀`.  Slice
  /// through this instead and decide what a miss means for the rule.
  pub fn source_slice(&self, start: usize, end: usize) -> Option<&'a str> {
    self.source.get(start..end)
  }

  /// `source[start..]`, or `None` when `start` is past the end or inside a
  /// multibyte character.  See [`Self::source_slice`].
  pub fn source_from(&self, start: usize) -> Option<&'a str> {
    self.source.get(start..)
  }

  /// The selector of the style rule that starts at `offset`, exactly as
  /// written: everything up to the `{` that opens its block, with trailing
  /// whitespace removed.
  ///
  /// The parsed selector is no substitute when positions matter: the CSS
  /// parser re-serialises it, so its length need not match the source.
  /// Returns `None` when `offset` is not a character boundary in the source.
  pub fn selector_source(&self, offset: usize) -> Option<&'a str> {
    let rest = self.source_from(offset)?;
    let end = selector_end(rest, !matches!(self.syntax, Syntax::Css));
    rest.get(..end).map(str::trim_end)
  }
}

/// Byte length of the selector at the start of `text`: the offset of the
/// first `{` that is not inside a string, comment, attribute selector,
/// parentheses or `#{...}` interpolation, or the whole text if there is none.
///
/// `line_comments` treats `//` as a comment to the end of the line, as in
/// SCSS and Less.
fn selector_end(text: &str, line_comments: bool) -> usize {
  let bytes = text.as_bytes();
  let mut depth = 0usize;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'{' if depth == 0 => return i,
      quote @ (b'"' | b'\'') => {
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = text[i + 2..]
          .find("*/")
          .map_or(bytes.len(), |end| i + 2 + end + 1);
      }
      b'/' if line_comments && bytes.get(i + 1) == Some(&b'/') => {
        i = text[i..].find('\n').map_or(bytes.len(), |end| i + end);
      }
      b'#' if bytes.get(i + 1) == Some(&b'{') => {
        depth += 1;
        i += 1;
      }
      b'(' | b'[' => depth += 1,
      b')' | b']' | b'}' => depth = depth.saturating_sub(1),
      _ => {}
    }
    i += 1;
  }
  bytes.len()
}

/// The secondary options object inside a Stylelint-format rule setting.
///
/// Array form `[primary, secondary]` yields the second element; a bare object
/// (as preset configs store `{"ignore": [...]}`) is returned as-is.
pub fn secondary_options_of(value: &serde_json::Value) -> Option<&serde_json::Value> {
  match value {
    serde_json::Value::Array(arr) => arr.get(1),
    serde_json::Value::Object(_) => Some(value),
    _ => None,
  }
}

/// A single lint rule that can inspect CSS AST nodes and emit diagnostics.
pub trait Rule: Send + Sync {
  /// Unique rule name, e.g. "block-no-empty".
  fn name(&self) -> &'static str;

  /// Human-readable description of what this rule checks.
  fn description(&self) -> &'static str;

  /// The default severity for diagnostics produced by this rule.
  fn default_severity(&self) -> Severity;

  /// Check a single CSS node and return any diagnostics.
  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    let _ = (node, context);
    vec![]
  }

  /// Check the entire document (all top-level nodes). Used for rules that need
  /// document-level context like duplicate detection or source-level checks.
  fn check_root(&self, _nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A context over `source` with no options.
  fn context(source: &str, syntax: Syntax) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax,
      options: None,
    }
  }

  #[test]
  fn source_slice_refuses_ranges_that_split_a_character() {
    let ctx = context("a{b:é}", Syntax::Css);
    assert_eq!(ctx.source_slice(4, 6), Some("é"));
    assert_eq!(ctx.source_slice(4, 5), None, "inside é");
    assert_eq!(ctx.source_slice(5, 7), None, "starts inside é");
    assert_eq!(ctx.source_slice(3, 99), None, "past the end");
    assert_eq!(ctx.source_slice(5, 4), None, "reversed");
    assert_eq!(ctx.source_from(6), Some("}"));
    assert_eq!(ctx.source_from(5), None);
  }

  #[test]
  fn selector_source_reads_up_to_the_opening_brace() {
    let ctx = context("a ,\n  b中 { color: red; }", Syntax::Css);
    assert_eq!(ctx.selector_source(0), Some("a ,\n  b中"));
    assert_eq!(ctx.selector_source(8), None, "inside 中");
  }

  #[test]
  fn selector_source_skips_braces_that_do_not_open_the_block() {
    let scss = context(
      ".a-#{$b}, [x=\"{\"] /* { */ .c(d) // {\n.e { }",
      Syntax::Scss,
    );
    assert_eq!(
      scss.selector_source(0),
      Some(".a-#{$b}, [x=\"{\"] /* { */ .c(d) // {\n.e")
    );
    // Plain CSS has no line comments, so `//` is just text.
    let css = context("a // b { }", Syntax::Css);
    assert_eq!(css.selector_source(0), Some("a // b"));
  }
}
