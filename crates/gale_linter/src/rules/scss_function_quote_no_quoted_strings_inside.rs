use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/function-quote-no-quoted-strings-inside";

declare_rule! {
  /// Disallow quoted strings inside `quote()`.
  pub struct ScssFunctionQuoteNoQuotedStringsInside => ScssFunctionQuoteNoQuotedStringsInsideCheck {
    name: NAME,
    description: "Disallow quoted strings inside quote()",
    default_severity: Warning,
  }
}

/// `scss/function-quote-no-quoted-strings-inside`, which takes no options.
pub struct ScssFunctionQuoteNoQuotedStringsInsideCheck;

impl ScssFunctionQuoteNoQuotedStringsInsideCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for ScssFunctionQuoteNoQuotedStringsInsideCheck {
  /// Flags `quote()` calls whose argument is already a quoted string.
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
              Diagnostic::new(NAME, "Unexpected quoted string inside quote()")
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
  use gale_css_parser::{Declaration, Span as ParserSpan};

  use crate::testing::{configured, invalid_options, scss_ctx};

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
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    let d = rule.check(&decl("quote(\"hello\")"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_unquoted_inside_quote() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    let d = rule.check(&decl("quote($var)"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_unquote_with_quoted_string() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // `unquote("hello")` should NOT be flagged — it's unquote, not quote
    let d = rule.check(&decl("unquote(\"hello\")"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_str_quote_variant() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // Function names ending in `-quote` should not be flagged
    let d = rule.check(&decl("str-quote(\"hello\")"), &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_standalone_quote_with_quoted() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // Standalone `quote("hello")` should still be flagged
    let d = rule.check(&decl("quote(\"hello\")"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_quote_after_space() {
    let rule = configured(&ScssFunctionQuoteNoQuotedStringsInside, None);
    // `foo quote("hello")` — quote after space is standalone
    let d = rule.check(&decl("foo quote(\"hello\")"), &scss_ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      [
        "Unexpected option value \"always\" for rule \"scss/function-quote-no-quoted-strings-inside\""
      ]
    );
  }
}
