use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow quoted strings inside `quote()`.
pub struct ScssFunctionQuoteNoQuotedStringsInside;

impl Rule for ScssFunctionQuoteNoQuotedStringsInside {
  fn name(&self) -> &'static str {
    "scss/function-quote-no-quoted-strings-inside"
  }

  fn description(&self) -> &'static str {
    "Disallow quoted strings inside quote()"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let CssNode::Declaration(decl) = node else {
      return vec![];
    };

    let value = &decl.value;
    let bytes = value.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      // Look for "quote(" but ensure it's not part of another function name
      // like "unquote(" or "str-quote("
      if let Some(pos) = value[i..].find("quote(") {
        let abs_pos = i + pos;
        // Check that the character before "quote(" is not alphanumeric, hyphen, or underscore
        let is_standalone = abs_pos == 0 || {
          let prev = bytes[abs_pos - 1];
          !prev.is_ascii_alphanumeric() && prev != b'-' && prev != b'_'
        };

        if is_standalone {
          let after = &value[abs_pos + 6..];
          let trimmed = after.trim_start();
          if trimmed.starts_with('"') || trimmed.starts_with('\'') {
            return vec![
              Diagnostic::new(self.name(), "Unexpected quoted string inside quote()")
                .severity(self.default_severity())
                .span(Span::new(decl.span.offset, decl.span.length)),
            ];
          }
        }

        i = abs_pos + 6;
      } else {
        break;
      }
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, Syntax};

  fn scss_ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  fn decl(value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "content".to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn reports_quoted_string_inside_quote() {
    let d = ScssFunctionQuoteNoQuotedStringsInside.check(&decl("quote(\"hello\")"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_unquoted_inside_quote() {
    let d = ScssFunctionQuoteNoQuotedStringsInside.check(&decl("quote($var)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_unquote_with_quoted_string() {
    // `unquote("hello")` should NOT be flagged — it's unquote, not quote
    let d = ScssFunctionQuoteNoQuotedStringsInside.check(&decl("unquote(\"hello\")"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_str_quote_variant() {
    // Function names ending in `-quote` should not be flagged
    let d =
      ScssFunctionQuoteNoQuotedStringsInside.check(&decl("str-quote(\"hello\")"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_standalone_quote_with_quoted() {
    // Standalone `quote("hello")` should still be flagged
    let d = ScssFunctionQuoteNoQuotedStringsInside.check(&decl("quote(\"hello\")"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_quote_after_space() {
    // `foo quote("hello")` — quote after space is standalone
    let d =
      ScssFunctionQuoteNoQuotedStringsInside.check(&decl("foo quote(\"hello\")"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }
}
